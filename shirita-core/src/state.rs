//! 变量状态沙箱：声明 schema、合并有效状态、解析/应用 <state_update> 指令。
//! 纯函数、无 I/O；写侧（apply）与读侧（effective_state）共用同一 schema 兜底。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, PromptNode};

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
        VarDecl {
            name: "$assistant_name".into(),
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

/// 当前变量清单文本（仅非系统变量，取当前值否则初值）。无用户变量时返回 None，
/// 这同时作为 state_update 协议的注入触发条件。
pub fn variables_block(schema: &[VarDecl], state: &Value) -> Option<String> {
    let lines: Vec<String> = schema
        .iter()
        .filter(|d| d.scope.as_deref() != Some("system"))
        .map(|d| {
            let val = state.get(&d.name).cloned().unwrap_or_else(|| d.initial.clone());
            let ty = match d.var_type {
                VarType::Number => "number",
                VarType::Bool => "bool",
                VarType::String => "string",
                VarType::List => "list",
            };
            format!("- {} ({}) = {}", d.name, ty, val)
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(format!("Current variables:\n{}", lines.join("\n")))
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

/// 指令动作集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Set,
    Add,
    Sub,
    Toggle,
    Append,
    Remove,
}

impl Action {
    pub fn parse(s: &str) -> Option<Action> {
        match s.to_ascii_uppercase().as_str() {
            "SET" => Some(Action::Set),
            "ADD" => Some(Action::Add),
            "SUB" => Some(Action::Sub),
            "TOGGLE" => Some(Action::Toggle),
            "APPEND" => Some(Action::Append),
            "REMOVE" => Some(Action::Remove),
            _ => None,
        }
    }
}

/// 一条解析后的状态更新指令。
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub action: Action,
    pub key: String,
    pub value: Option<String>,
}

/// 从流式文本中提取所有 `<state_update action=".." key=".." value=".."/>`（按出现顺序）。
pub fn parse_state_updates(text: &str) -> Vec<Update> {
    static TAG_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#"(?is)<state_update\b([^>]*?)/?>"#).unwrap());
    static ATTR_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#"(\w+)\s*=\s*"([^"]*)""#).unwrap());
    let tag_re = &*TAG_RE;
    let attr_re = &*ATTR_RE;
    let mut out = Vec::new();
    for caps in tag_re.captures_iter(text) {
        let mut action = None;
        let mut key = None;
        let mut value = None;
        for a in attr_re.captures_iter(&caps[1]) {
            match a[1].to_ascii_lowercase().as_str() {
                "action" => action = Action::parse(&a[2]),
                "key" => key = Some(a[2].to_string()),
                "value" => value = Some(a[2].to_string()),
                _ => {}
            }
        }
        if let (Some(action), Some(key)) = (action, key) {
            out.push(Update { action, key, value });
        }
    }
    out
}

/// 移除所有 state_update 标签（用于展示文本）。
pub fn strip_state_tags(text: &str) -> String {
    static TAG_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r#"(?is)<state_update\b[^>]*?/?>"#).unwrap());
    TAG_RE.replace_all(text, "").trim().to_string()
}

fn parse_decls(v: Option<&Value>, scope: &str) -> Vec<VarDecl> {
    let Some(arr) = v.and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let mut d: VarDecl = serde_json::from_value(item.clone()).ok()?;
            d.scope = Some(scope.to_string());
            Some(d)
        })
        .collect()
}

fn merge_decls(out: &mut Vec<VarDecl>, decls: Vec<VarDecl>) {
    for d in decls {
        if let Some(existing) = out.iter_mut().find(|x| x.name == d.name) {
            *existing = d; // 后者覆盖（precedence: system < template < local）
        } else {
            out.push(d);
        }
    }
}

/// Resolve a session's effective schema: system ∪ template `meta.variables` ∪
/// each mounted pack's `meta.variables` ∪ session `override_config.local_variables`.
/// Later sources win on name collision (local is authoritative).
pub fn resolve_schema_with_packs(
    template_meta: Option<&Value>,
    pack_metas: &[Value],
    override_config: &Value,
) -> Vec<VarDecl> {
    let mut out = system_variables();
    merge_decls(&mut out, parse_decls(template_meta.and_then(|m| m.get("variables")), "template"));
    for pm in pack_metas {
        merge_decls(&mut out, parse_decls(pm.get("variables"), "pack"));
    }
    merge_decls(&mut out, parse_decls(override_config.get("local_variables"), "local"));
    out
}

/// Back-compat: resolve a schema with no mounted packs.
pub fn resolve_schema(template_meta: Option<&Value>, override_config: &Value) -> Vec<VarDecl> {
    resolve_schema_with_packs(template_meta, &[], override_config)
}

/// Parse a brick's `meta.decls` array into VarDecls (scope left `None`; the
/// caller tags scope when merging).
fn decls_of(meta: &Value) -> Vec<VarDecl> {
    meta.get("decls")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| serde_json::from_value::<VarDecl>(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn tag_scope(mut decls: Vec<VarDecl>, scope: &str) -> Vec<VarDecl> {
    for d in &mut decls {
        d.scope = Some(scope.to_string());
    }
    decls
}

/// Extract VarDecls from `variables` bricks referenced by enabled `ref` nodes in
/// one tree, in `sort_order`. Decls come from each brick's `meta.decls`; scope is
/// left `None` (the schema resolver tags it per source).
pub fn variables_from_nodes(nodes: &[PromptNode], defs: &HashMap<String, Definition>) -> Vec<VarDecl> {
    let mut refs: Vec<&PromptNode> =
        nodes.iter().filter(|n| n.kind == NodeKind::Ref && n.enabled).collect();
    refs.sort_by_key(|n| n.sort_order);
    let mut out = Vec::new();
    for n in refs {
        let Some(def) = n.definition_id.as_deref().and_then(|id| defs.get(id)) else {
            continue;
        };
        if def.def_type == "variables" {
            out.extend(decls_of(&def.meta));
        }
    }
    out
}

/// Resolve a session's effective schema from `variables` bricks: system ∪
/// template-tree decls ∪ each mounted pack's decls (mount order) ∪ session
/// `override_config.local_variables`. Later sources win on name collision.
pub fn resolve_schema_from_bricks(
    template_decls: Vec<VarDecl>,
    pack_decls: Vec<Vec<VarDecl>>,
    override_config: &Value,
) -> Vec<VarDecl> {
    let mut out = system_variables();
    merge_decls(&mut out, tag_scope(template_decls, "template"));
    for pd in pack_decls {
        merge_decls(&mut out, tag_scope(pd, "pack"));
    }
    merge_decls(&mut out, parse_decls(override_config.get("local_variables"), "local"));
    out
}

fn num_value(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        Value::Number(serde_json::Number::from(n as i64))
    } else {
        serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::Null)
    }
}

fn coerce(value: &Option<String>, vt: VarType) -> Option<Value> {
    let s = value.as_ref()?;
    match vt {
        VarType::Number => s.parse::<f64>().ok().map(num_value),
        VarType::Bool => match s.to_ascii_lowercase().as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        VarType::String => Some(Value::String(s.clone())),
        VarType::List => serde_json::from_str::<Vec<Value>>(s).ok().map(Value::Array),
    }
}

/// 按 schema 类型逐条应用更新；未声明的 key 或类型不符的动作一律忽略（沙箱不执行代码）。
pub fn apply_updates(state: &Value, schema: &[VarDecl], updates: &[Update]) -> Value {
    let mut obj = state.as_object().cloned().unwrap_or_default();
    for u in updates {
        let Some(vt) = schema.iter().find(|d| d.name == u.key).map(|d| d.var_type) else {
            continue; // 未声明
        };
        match (u.action, vt) {
            (Action::Set, _) => {
                if let Some(v) = coerce(&u.value, vt) {
                    obj.insert(u.key.clone(), v);
                }
            }
            (Action::Add, VarType::Number) | (Action::Sub, VarType::Number) => {
                let cur = obj.get(&u.key).and_then(|v| v.as_f64()).unwrap_or(0.0);
                if let Some(n) = u.value.as_ref().and_then(|s| s.parse::<f64>().ok()) {
                    let next = if u.action == Action::Add { cur + n } else { cur - n };
                    obj.insert(u.key.clone(), num_value(next));
                }
            }
            (Action::Toggle, VarType::Bool) => {
                let cur = obj.get(&u.key).and_then(|v| v.as_bool()).unwrap_or(false);
                obj.insert(u.key.clone(), Value::Bool(!cur));
            }
            (Action::Append, VarType::List) => {
                if let Some(val) = &u.value {
                    let mut arr = obj.get(&u.key).and_then(|v| v.as_array().cloned()).unwrap_or_default();
                    arr.push(Value::String(val.clone()));
                    obj.insert(u.key.clone(), Value::Array(arr));
                }
            }
            (Action::Remove, VarType::List) => {
                if let Some(val) = &u.value {
                    let mut arr = obj.get(&u.key).and_then(|v| v.as_array().cloned()).unwrap_or_default();
                    if let Some(pos) = arr.iter().position(|e| e.as_str() == Some(val.as_str())) {
                        arr.remove(pos);
                    }
                    obj.insert(u.key.clone(), Value::Array(arr));
                }
            }
            _ => {} // 动作/类型不匹配
        }
    }
    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pack_variables_merge_between_template_and_local() {
        let tmeta = serde_json::json!({ "variables": [ { "name": "tone", "type": "string", "initial": "calm" } ] });
        let pmeta = serde_json::json!({ "variables": [ { "name": "affection", "type": "number", "initial": "0" } ] });
        let cfg = serde_json::json!({});
        let schema = resolve_schema_with_packs(Some(&tmeta), std::slice::from_ref(&pmeta), &cfg);
        assert!(schema.iter().any(|d| d.name == "tone"));
        assert!(schema.iter().any(|d| d.name == "affection"), "pack variable is in the schema");
    }

    #[test]
    fn local_overrides_pack_variable() {
        let pmeta = serde_json::json!({ "variables": [ { "name": "affection", "type": "number", "initial": "0" } ] });
        let cfg = serde_json::json!({ "local_variables": [ { "name": "affection", "type": "string", "initial": "x" } ] });
        let schema = resolve_schema_with_packs(None, std::slice::from_ref(&pmeta), &cfg);
        let d = schema.iter().find(|d| d.name == "affection").unwrap();
        assert_eq!(d.var_type, VarType::String, "local declaration wins over pack");
    }

    fn schema() -> Vec<VarDecl> {
        vec![
            VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(100), scope: None },
            VarDecl { name: "gold".into(), var_type: VarType::Number, initial: json!(0), scope: None },
            VarDecl { name: "reputation".into(), var_type: VarType::Number, initial: json!(50), scope: None },
        ]
    }

    #[test]
    fn variables_block_lists_only_user_vars() {
        let schema = vec![
            VarDecl { name: "$avatar".into(), var_type: VarType::String, initial: Value::String("".into()), scope: Some("system".into()) },
            VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(100), scope: Some("template".into()) },
        ];
        let state = json!({ "hp": 80 });
        let block = variables_block(&schema, &state).unwrap();
        assert!(block.contains("- hp (number) = 80"));
        assert!(!block.contains("$avatar")); // system vars excluded
    }

    #[test]
    fn variables_block_is_none_without_user_vars() {
        let schema = system_variables(); // only $-vars
        assert_eq!(variables_block(&schema, &json!({})), None);
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

    #[test]
    fn parses_multiple_updates_in_order() {
        let text = "You take a hit. <state_update action=\"SUB\" key=\"hp\" value=\"5\"/> \
                    <state_update action=\"TOGGLE\" key=\"alarmed\"/>";
        let ups = parse_state_updates(text);
        assert_eq!(ups.len(), 2);
        assert_eq!(ups[0], Update { action: Action::Sub, key: "hp".into(), value: Some("5".into()) });
        assert_eq!(ups[1], Update { action: Action::Toggle, key: "alarmed".into(), value: None });
    }

    #[test]
    fn strips_tags_from_display() {
        let text = "Hello there. <state_update action=\"SET\" key=\"$avatar\" value=\"a.png\"/>";
        assert_eq!(strip_state_tags(text), "Hello there.");
    }

    #[test]
    fn unknown_action_is_dropped() {
        assert!(parse_state_updates("<state_update action=\"NUKE\" key=\"hp\" value=\"1\"/>").is_empty());
    }

    fn full_schema() -> Vec<VarDecl> {
        vec![
            VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(100), scope: None },
            VarDecl { name: "alarmed".into(), var_type: VarType::Bool, initial: json!(false), scope: None },
            VarDecl { name: "name".into(), var_type: VarType::String, initial: json!(""), scope: None },
            VarDecl { name: "bag".into(), var_type: VarType::List, initial: json!([]), scope: None },
        ]
    }

    #[test]
    fn applies_typed_actions_and_ignores_invalid() {
        let s = full_schema();
        let st = json!({ "hp": 100, "alarmed": false, "bag": [] });
        let ups = vec![
            Update { action: Action::Sub, key: "hp".into(), value: Some("30".into()) },
            Update { action: Action::Toggle, key: "alarmed".into(), value: None },
            Update { action: Action::Set, key: "name".into(), value: Some("Ada".into()) },
            Update { action: Action::Append, key: "bag".into(), value: Some("key".into()) },
            Update { action: Action::Add, key: "hp".into(), value: Some("oops".into()) }, // non-numeric -> ignored
            Update { action: Action::Set, key: "ghost".into(), value: Some("x".into()) }, // undeclared -> ignored
        ];
        let out = apply_updates(&st, &s, &ups);
        assert_eq!(out["hp"], 70);
        assert_eq!(out["alarmed"], true);
        assert_eq!(out["name"], "Ada");
        assert_eq!(out["bag"], json!(["key"]));
        assert!(out.get("ghost").is_none());
    }

    #[test]
    fn remove_drops_first_match() {
        let s = full_schema();
        let st = json!({ "bag": ["key", "map", "key"] });
        let out = apply_updates(&st, &s, &[Update { action: Action::Remove, key: "bag".into(), value: Some("key".into()) }]);
        assert_eq!(out["bag"], json!(["map", "key"]));
    }

    #[test]
    fn resolve_schema_unions_system_template_local() {
        let tmeta = json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
        let cfg = json!({ "local_variables": [ {"name":"reputation","type":"number","initial":0} ] });
        let s = resolve_schema(Some(&tmeta), &cfg);
        let names: Vec<&str> = s.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"$avatar")); // system always present
        assert!(names.contains(&"$assistant_name"));
        assert!(names.contains(&"hp")); // template
        assert!(names.contains(&"reputation")); // local
        assert_eq!(s.iter().find(|d| d.name == "hp").unwrap().scope.as_deref(), Some("template"));
    }

    #[test]
    fn local_overrides_template_on_name_clash() {
        let tmeta = json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
        let cfg = json!({ "local_variables": [ {"name":"hp","type":"number","initial":250} ] });
        let s = resolve_schema(Some(&tmeta), &cfg);
        let hp = s.iter().find(|d| d.name == "hp").unwrap();
        assert_eq!(hp.initial, 250);
        assert_eq!(hp.scope.as_deref(), Some("local"));
    }

    #[test]
    fn variables_from_nodes_reads_enabled_variables_bricks_in_order() {
        use crate::models::definition::Definition;
        use crate::models::prompt_node::{OwnerKind, PromptNode};
        use std::collections::HashMap;

        let mut a = Definition::new("variables", "A", "");
        a.id = "a".into();
        a.meta = json!({ "decls": [{ "name": "hp", "type": "number", "initial": 100 }] });
        let mut b = Definition::new("variables", "B", "");
        b.id = "b".into();
        b.meta = json!({ "decls": [{ "name": "mood", "type": "string", "initial": "calm" }] });
        let mut other = Definition::new("char", "C", "x"); // not a variables brick
        other.id = "c".into();
        let mut disabled = Definition::new("variables", "D", "");
        disabled.id = "d".into();
        disabled.meta = json!({ "decls": [{ "name": "secret", "type": "string", "initial": "x" }] });

        let r_b = PromptNode::new_ref(OwnerKind::Pack, "p", None, 1, "b");
        let r_a = PromptNode::new_ref(OwnerKind::Pack, "p", None, 0, "a");
        let r_c = PromptNode::new_ref(OwnerKind::Pack, "p", None, 2, "c");
        let mut r_d = PromptNode::new_ref(OwnerKind::Pack, "p", None, 3, "d");
        r_d.enabled = false;
        let nodes = vec![r_b, r_a, r_c, r_d];

        let mut defs = HashMap::new();
        for d in [a, b, other, disabled] {
            defs.insert(d.id.clone(), d);
        }

        let decls = variables_from_nodes(&nodes, &defs);
        let names: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["hp", "mood"], "sort_order, only enabled variables bricks");
    }

    #[test]
    fn resolve_schema_from_bricks_merges_with_mount_order_precedence() {
        let template = vec![VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(100), scope: None }];
        let pack_a = vec![VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(50), scope: None }];
        let pack_b = vec![VarDecl { name: "gold".into(), var_type: VarType::Number, initial: json!(7), scope: None }];
        let cfg = json!({ "local_variables": [{ "name": "hp", "type": "number", "initial": 250 }] });

        let schema = resolve_schema_from_bricks(template, vec![pack_a, pack_b], &cfg);
        let hp = schema.iter().find(|d| d.name == "hp").unwrap();
        assert_eq!(hp.initial, json!(250), "local wins over template/packs");
        assert_eq!(hp.scope.as_deref(), Some("local"));
        let gold = schema.iter().find(|d| d.name == "gold").unwrap();
        assert_eq!(gold.scope.as_deref(), Some("pack"));
        // system variables ($avatar/$background) are always present
        assert!(schema.iter().any(|d| d.name == "$avatar"));
    }
}
