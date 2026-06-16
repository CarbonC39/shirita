//! 变量状态沙箱：声明 schema、合并有效状态、解析/应用 <state_update> 指令。
//! 纯函数、无 I/O；写侧（apply）与读侧（effective_state）共用同一 schema 兜底。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 变量类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarType {
    Number,
    Bool,
    String,
    List,
}

/// 一条变量声明。`scope` 仅供前端分组（system/template/local），存储时可省略。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VarDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: VarType,
    #[serde(default)]
    pub initial: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// 内置系统变量注册表（保留 `$` 命名空间，恒存在，绑定到渲染行为）。
pub fn system_variables() -> Vec<VarDecl> {
    vec![
        VarDecl {
            name: "$avatar".into(),
            var_type: VarType::String,
            initial: Value::String(String::new()),
            scope: Some("system".into()),
        },
        VarDecl {
            name: "$background".into(),
            var_type: VarType::String,
            initial: Value::String(String::new()),
            scope: Some("system".into()),
        },
    ]
}

/// schema 的初值映射 {name: initial}。
pub fn schema_initials(schema: &[VarDecl]) -> Map<String, Value> {
    schema.iter().map(|d| (d.name.clone(), d.initial.clone())).collect()
}

/// 读侧唯一真相：schema 初值 < seed(session.current_state) < 分支叶子快照（后者覆盖前者）。
/// 保证新增变量在旧快照分支上回填初值，旧快照对 schema 增长免疫。
pub fn effective_state(schema: &[VarDecl], seed: &Value, leaf_snapshot: &Value) -> Value {
    let mut out = schema_initials(schema);
    if let Some(o) = seed.as_object() {
        for (k, v) in o {
            out.insert(k.clone(), v.clone());
        }
    }
    if let Some(o) = leaf_snapshot.as_object() {
        for (k, v) in o {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Vec<VarDecl> {
        vec![
            VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(100), scope: None },
            VarDecl { name: "gold".into(), var_type: VarType::Number, initial: json!(0), scope: None },
            VarDecl { name: "reputation".into(), var_type: VarType::Number, initial: json!(50), scope: None },
        ]
    }

    #[test]
    fn effective_state_backfills_new_vars_and_leaf_wins() {
        // seed predates `reputation`; leaf snapshot has evolved hp/gold but no reputation.
        let seed = json!({ "hp": 100, "gold": 0 });
        let leaf = json!({ "hp": 80, "gold": 30 });
        let eff = effective_state(&schema(), &seed, &leaf);
        assert_eq!(eff["hp"], 80); // leaf wins
        assert_eq!(eff["gold"], 30); // leaf wins
        assert_eq!(eff["reputation"], 50); // backfilled from schema initial
    }

    #[test]
    fn seed_overrides_schema_initial_when_leaf_silent() {
        let seed = json!({ "hp": 120 }); // a session that started richer than the declared 100
        let leaf = json!({});
        let eff = effective_state(&schema(), &seed, &leaf);
        assert_eq!(eff["hp"], 120); // seed beats schema initial
        assert_eq!(eff["gold"], 0); // untouched -> schema initial
    }
}
