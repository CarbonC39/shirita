//! Prompt 组装：局部覆盖 → 变量渲染 → XML 封包；以及 regex_rule 输出清洗。

use std::collections::{HashMap, HashSet};

use crate::keyword::KeywordIndex;
use crate::models::definition::{Definition, DefinitionType};
use crate::models::prompt_node::{NodeKind, PromptNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    Constant,
    Keyword,
    Random,
}

#[derive(Debug, Clone)]
pub struct Trigger {
    pub mode: TriggerMode,
    pub keys: Vec<String>,
    pub probability: u8, // 0..=100
}

/// 一个待激活条目（来自某 ref 节点解析后的定义）。
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub trigger: Trigger,
    pub content: String, // 已是有效内容（局部覆盖优先），用于递归扫描
}

/// 从 `definition.meta`（即整个 meta 对象）解析 trigger；缺省 constant。
pub fn parse_trigger(meta: &serde_json::Value) -> Trigger {
    let t = meta.get("trigger");
    let mode = match t.and_then(|v| v.get("mode")).and_then(|v| v.as_str()) {
        Some("keyword") => TriggerMode::Keyword,
        Some("random") => TriggerMode::Random,
        _ => TriggerMode::Constant,
    };
    let keys = t
        .and_then(|v| v.get("keys"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let probability = t
        .and_then(|v| v.get("probability"))
        .and_then(|v| v.as_u64())
        .unwrap_or(100)
        .min(100) as u8;
    Trigger { mode, keys, probability }
}

/// 计算激活集：constant 恒激活；random 按 roll；keyword 命中扫描缓冲；
/// recursive 时把已激活内容并入缓冲再扫，直到收敛（限 3 轮）。
/// `roll() -> f64 in [0,1)`。
pub fn activate(
    entries: &[Entry],
    buffer: &str,
    recursive: bool,
    roll: &mut impl FnMut() -> f64,
) -> HashSet<String> {
    let mut active: HashSet<String> = HashSet::new();

    // constant + random 先定。
    for e in entries {
        match e.trigger.mode {
            TriggerMode::Constant => {
                active.insert(e.id.clone());
            }
            TriggerMode::Random => {
                if roll() < e.trigger.probability as f64 / 100.0 {
                    active.insert(e.id.clone());
                }
            }
            TriggerMode::Keyword => {}
        }
    }

    // keyword：构建一次自动机，对缓冲（可递归扩充）扫描。
    let kw: Vec<(String, Vec<String>)> = entries
        .iter()
        .filter(|e| e.trigger.mode == TriggerMode::Keyword)
        .map(|e| (e.id.clone(), e.trigger.keys.clone()))
        .collect();
    let index = KeywordIndex::build(&kw);

    let mut scan_text = buffer.to_string();
    // 仅递归模式下把已激活条目内容并入缓冲（constant 也作为递归来源）；
    // 非递归仅扫聊天缓冲。
    if recursive {
        for e in entries {
            if active.contains(&e.id) {
                scan_text.push('\n');
                scan_text.push_str(&e.content);
            }
        }
    }

    let max_passes = if recursive { 3 } else { 1 };
    for _ in 0..max_passes {
        let hits = index.scan(&scan_text);
        let mut grew = false;
        for id in hits {
            if active.insert(id.clone()) {
                grew = true;
                if recursive {
                    if let Some(e) = entries.iter().find(|e| e.id == id) {
                        scan_text.push('\n');
                        scan_text.push_str(&e.content);
                    }
                }
            }
        }
        if !grew || !recursive {
            break;
        }
    }

    active
}

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

/// 段落落点：历史消息节点之前 / 之后。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    BeforeHistory,
    AfterHistory,
}

/// 组装产物中的一个 system 段落（未拼接，保留来源与落点）。
#[derive(Debug, Clone)]
pub struct PromptSegment {
    pub placement: Placement,
    pub content: String,
    /// 来源标识（容器 tag 或定义 id），便于调试/导出。
    pub source: String,
}

/// 树驱动组装的结构化结果：段落 + 是否启用真实历史。
#[derive(Debug, Clone)]
pub struct AssembledPlan {
    pub segments: Vec<PromptSegment>,
    pub history_enabled: bool,
}

/// 取定义的有效 trigger：会话局部覆盖优先，否则用 definition.meta。
fn effective_trigger(def: &Definition, overrides: &serde_json::Value) -> Trigger {
    if let Some(t) = overrides.get(&def.id).and_then(|o| o.get("trigger")) {
        return parse_trigger(&serde_json::json!({ "trigger": t }));
    }
    parse_trigger(&def.meta)
}

/// 取定义的有效内容：会话局部覆盖（结构化 {content}）优先，否则用全局 content。
fn effective_def_content(def: &Definition, overrides: &serde_json::Value) -> String {
    overrides
        .get(&def.id)
        .and_then(|o| o.get("content"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| def.content.clone())
}

/// 树驱动组装：遍历节点树，按触发激活筛选 ref，容器封包，history 节点切分前后。
///
/// - 仅启用 + 激活的 ref 进入结果；空容器被省略。
/// - history 节点（启用）把后续段落落点切到 `AfterHistory` 并置 `history_enabled`。
#[allow(clippy::too_many_arguments)]
pub fn assemble_from_nodes(
    nodes: &[PromptNode],
    definitions: &HashMap<String, Definition>,
    overrides: &serde_json::Value,
    state: &serde_json::Value,
    recent_msgs: &[String],
    recursive: bool,
    _scan_depth: usize,
    roll: &mut impl FnMut() -> f64,
) -> AssembledPlan {
    // 1) 从所有 ref 节点构建 Entry（用于激活计算）。
    let mut entries: Vec<Entry> = Vec::new();
    for n in nodes {
        if n.kind != NodeKind::Ref {
            continue;
        }
        let Some(def) = n.definition_id.as_ref().and_then(|id| definitions.get(id)) else {
            continue;
        };
        entries.push(Entry {
            id: def.id.clone(),
            trigger: effective_trigger(def, overrides),
            content: render_vars(&effective_def_content(def, overrides), state),
        });
    }

    // 2) 计算激活集（聊天缓冲 + 可选递归扩充）。
    let active = activate(&entries, &recent_msgs.join("\n"), recursive, roll);

    // ref 是否纳入及其渲染内容。
    let resolve = |n: &PromptNode| -> Option<String> {
        if !n.enabled || n.kind != NodeKind::Ref {
            return None;
        }
        let def = n.definition_id.as_ref().and_then(|id| definitions.get(id))?;
        if !active.contains(&def.id) {
            return None;
        }
        Some(render_vars(&effective_def_content(def, overrides), state))
    };

    // 3) 按 sort_order 遍历根节点；history 切换落点。
    let mut roots: Vec<&PromptNode> = nodes.iter().filter(|n| n.parent_id.is_none()).collect();
    roots.sort_by_key(|n| n.sort_order);

    let mut segments: Vec<PromptSegment> = Vec::new();
    let mut placement = Placement::BeforeHistory;
    let mut history_enabled = false;

    for root in roots {
        match root.kind {
            NodeKind::History => {
                if root.enabled {
                    placement = Placement::AfterHistory;
                    history_enabled = true;
                }
            }
            NodeKind::Folder => {
                if !root.enabled {
                    continue;
                }
                let tag = root.tag.clone().unwrap_or_default();
                let mut children: Vec<&PromptNode> = nodes
                    .iter()
                    .filter(|n| n.parent_id.as_deref() == Some(root.id.as_str()))
                    .collect();
                children.sort_by_key(|n| n.sort_order);
                let bodies: Vec<String> = children.iter().filter_map(|c| resolve(c)).collect();
                if bodies.is_empty() {
                    continue;
                }
                segments.push(PromptSegment {
                    placement,
                    content: format!("<{tag}>\n{}\n</{tag}>", bodies.join("\n")),
                    source: tag,
                });
            }
            NodeKind::Ref => {
                if let Some(content) = resolve(root) {
                    segments.push(PromptSegment {
                        placement,
                        content,
                        source: root.definition_id.clone().unwrap_or_default(),
                    });
                }
            }
        }
    }

    AssembledPlan { segments, history_enabled }
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

    fn ent(id: &str, mode: TriggerMode, keys: &[&str], content: &str) -> Entry {
        Entry {
            id: id.to_string(),
            trigger: Trigger {
                mode,
                keys: keys.iter().map(|s| s.to_string()).collect(),
                probability: 100,
            },
            content: content.to_string(),
        }
    }

    #[test]
    fn parse_trigger_defaults_to_constant() {
        let t = parse_trigger(&json!({}));
        assert_eq!(t.mode, TriggerMode::Constant);
    }

    #[test]
    fn parse_trigger_reads_keyword() {
        let t = parse_trigger(&json!({ "trigger": { "mode": "keyword", "keys": ["zion"] } }));
        assert_eq!(t.mode, TriggerMode::Keyword);
        assert_eq!(t.keys, vec!["zion".to_string()]);
    }

    #[test]
    fn activate_constant_always_keyword_on_match() {
        let entries = vec![
            ent("neo", TriggerMode::Constant, &[], "Neo body"),
            ent("zion", TriggerMode::Keyword, &["zion"], "Zion body"),
            ent("trinity", TriggerMode::Keyword, &["trinity"], "Trinity body"),
        ];
        let active = activate(&entries, "tell me about zion", false, &mut || 0.0);
        assert!(active.contains("neo"));
        assert!(active.contains("zion"));
        assert!(!active.contains("trinity"));
    }

    #[test]
    fn activate_random_uses_roll() {
        let entries = vec![Entry {
            id: "r".into(),
            trigger: Trigger { mode: TriggerMode::Random, keys: vec![], probability: 50 },
            content: String::new(),
        }];
        assert!(activate(&entries, "", false, &mut || 0.2).contains("r")); // 0.2 < 0.5
        assert!(!activate(&entries, "", false, &mut || 0.9).contains("r"));
    }

    #[test]
    fn activate_recursive_chains() {
        // "zion" not in chat, but "neo" constant content mentions zion → recursion activates zion.
        let entries = vec![
            ent("neo", TriggerMode::Constant, &[], "Neo lives in zion"),
            ent("zion", TriggerMode::Keyword, &["zion"], "Zion body"),
        ];
        assert!(!activate(&entries, "hi", false, &mut || 0.0).contains("zion"));
        assert!(activate(&entries, "hi", true, &mut || 0.0).contains("zion"));
    }

    use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};

    fn folder_node(owner: &str, sort: i64, tag: &str) -> PromptNode {
        PromptNode::new_folder(OwnerKind::Template, owner, None, sort, tag)
    }
    fn child_ref(owner: &str, parent: &str, sort: i64, d: &str) -> PromptNode {
        PromptNode::new_ref(OwnerKind::Template, owner, Some(parent.to_string()), sort, d)
    }
    fn root_ref(owner: &str, sort: i64, d: &str) -> PromptNode {
        PromptNode::new_ref(OwnerKind::Template, owner, None, sort, d)
    }
    fn history_node(owner: &str, sort: i64) -> PromptNode {
        let mut n = PromptNode::new_folder(OwnerKind::Template, owner, None, sort, "history");
        n.kind = NodeKind::History;
        n.tag = None;
        n
    }

    #[test]
    fn assemble_wraps_containers_splits_history() {
        let neo = def(DefinitionType::Char, "Neo", "Neo body");
        let jb = def(DefinitionType::Prompt, "JB", "Jailbreak body");
        let charf = folder_node("t", 0, "char");
        let cref = child_ref("t", &charf.id, 0, &neo.id);
        let hist = history_node("t", 1);
        let after = root_ref("t", 2, &jb.id);

        let mut defs = std::collections::HashMap::new();
        defs.insert(neo.id.clone(), neo.clone());
        defs.insert(jb.id.clone(), jb.clone());

        let nodes = vec![charf, cref, hist, after];
        let plan = assemble_from_nodes(
            &nodes, &defs, &json!({}), &json!({}), &["hi".to_string()], true, 4, &mut || 0.0,
        );
        let before: Vec<_> = plan
            .segments
            .iter()
            .filter(|s| s.placement == Placement::BeforeHistory)
            .collect();
        let after_s: Vec<_> = plan
            .segments
            .iter()
            .filter(|s| s.placement == Placement::AfterHistory)
            .collect();
        assert_eq!(before.len(), 1);
        assert!(before[0].content.contains("<char>\nNeo body\n</char>"));
        assert_eq!(after_s.len(), 1);
        assert_eq!(after_s[0].content, "Jailbreak body");
        assert!(plan.history_enabled);
    }

    #[test]
    fn assemble_omits_empty_container_and_inactive_refs() {
        let mut lore = def(DefinitionType::World, "Zion", "Zion body");
        lore.meta = json!({ "trigger": { "mode": "keyword", "keys": ["zion"] } });
        let wf = folder_node("t", 0, "world");
        let wref = child_ref("t", &wf.id, 0, &lore.id);
        let mut defs = std::collections::HashMap::new();
        defs.insert(lore.id.clone(), lore.clone());
        let nodes = vec![wf, wref];
        // No "zion" in buffer → world container empty → omitted.
        let plan = assemble_from_nodes(
            &nodes, &defs, &json!({}), &json!({}), &["hi".into()], false, 4, &mut || 0.0,
        );
        assert!(plan.segments.is_empty());
    }
}
