//! Prompt 组装：局部覆盖 → 变量渲染 → XML 封包；以及 regex_rule 输出清洗。

use crate::models::definition::{Definition, DefinitionType};

/// 取定义的"有效内容"：若 local_overrides 含该 id 则用覆盖文本，否则用全局 content。
fn effective_content(def: &Definition, local_overrides: &serde_json::Value) -> String {
    local_overrides
        .get(&def.id)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| def.content.clone())
}

/// 用 state 渲染 `{{var}}`；未知键保留原占位符。
pub fn render_vars(content: &str, state: &serde_json::Value) -> String {
    let re = regex::Regex::new(r"\{\{\s*([A-Za-z0-9_]+)\s*\}\}").unwrap();
    re.replace_all(content, |caps: &regex::Captures| {
        let key = &caps[1];
        match state.get(key) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Number(n)) => n.to_string(),
            Some(serde_json::Value::Bool(b)) => b.to_string(),
            _ => caps[0].to_string(),
        }
    })
    .into_owned()
}

/// type → 封包标签；返回 None 表示不进 system（regex_rule/tool）。
fn wrap_tag(t: &DefinitionType) -> Option<&'static str> {
    match t {
        DefinitionType::Persona => Some("personas"),
        DefinitionType::Char => Some("characters"),
        DefinitionType::World => Some("world_rules"),
        DefinitionType::Item => Some("items"),
        DefinitionType::Prompt => Some("prompts"),
        DefinitionType::RegexRule | DefinitionType::Tool => None,
    }
}

/// 组装 system 文本：按固定 type 顺序分组，每组用 `<tag>…</tag>` 包裹，组内按挂载顺序拼接。
pub fn assemble_system_prompt(
    mounted: &[Definition],
    local_overrides: &serde_json::Value,
    state: &serde_json::Value,
) -> String {
    // 固定分组顺序。
    let order = [
        DefinitionType::Persona,
        DefinitionType::Char,
        DefinitionType::World,
        DefinitionType::Item,
        DefinitionType::Prompt,
    ];
    let mut blocks: Vec<String> = Vec::new();
    for group in order {
        let tag = wrap_tag(&group).unwrap();
        let bodies: Vec<String> = mounted
            .iter()
            .filter(|d| d.def_type == group)
            .map(|d| render_vars(&effective_content(d, local_overrides), state))
            .collect();
        if !bodies.is_empty() {
            blocks.push(format!("<{tag}>\n{}\n</{tag}>", bodies.join("\n")));
        }
    }
    blocks.join("\n")
}

/// 依挂载顺序对文本应用 regex_rule（meta: {pattern, replacement}）。无规则返回 None。
pub fn apply_regex_rules(text: &str, rules: &[Definition]) -> Option<String> {
    if rules.is_empty() {
        return None;
    }
    let mut out = text.to_string();
    for rule in rules {
        let pattern = rule.meta.get("pattern").and_then(|v| v.as_str());
        let replacement = rule
            .meta
            .get("replacement")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(p) = pattern {
            if let Ok(re) = regex::Regex::new(p) {
                out = re.replace_all(&out, replacement).into_owned();
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::definition::{Definition, DefinitionType};
    use serde_json::json;

    fn def(t: DefinitionType, name: &str, content: &str) -> Definition {
        Definition::new(t, name, content)
    }

    #[test]
    fn render_vars_known_and_unknown() {
        let s = json!({ "name": "Alice", "hp": 80 });
        assert_eq!(
            render_vars("Hi {{name}}, hp={{hp}} {{missing}}", &s),
            "Hi Alice, hp=80 {{missing}}"
        );
    }

    #[test]
    fn assemble_groups_in_order_with_tags() {
        let mounted = vec![
            def(DefinitionType::World, "w", "rule1"),
            def(DefinitionType::Char, "c", "I am {{name}}"),
            def(DefinitionType::RegexRule, "r", "ignored"),
        ];
        let out = assemble_system_prompt(&mounted, &json!({}), &json!({ "name": "Bob" }));
        assert!(out.contains("<characters>\nI am Bob\n</characters>"));
        assert!(out.contains("<world_rules>\nrule1\n</world_rules>"));
        assert!(out.find("<characters>").unwrap() < out.find("<world_rules>").unwrap());
        assert!(!out.contains("ignored"));
    }

    #[test]
    fn local_override_replaces_content() {
        let d = def(DefinitionType::Char, "c", "global");
        let overrides = json!({ d.id.clone(): "overridden" });
        let out = assemble_system_prompt(std::slice::from_ref(&d), &overrides, &json!({}));
        assert!(out.contains("overridden"));
        assert!(!out.contains("global"));
    }

    #[test]
    fn regex_rules_clean_text() {
        let mut r = def(DefinitionType::RegexRule, "r", "");
        r.meta = json!({ "pattern": "<think>.*?</think>", "replacement": "" });
        assert_eq!(
            apply_regex_rules("a<think>x</think>b", &[r]).as_deref(),
            Some("ab")
        );
        assert_eq!(apply_regex_rules("abc", &[]), None);
    }
}
