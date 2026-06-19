//! Prompt 组装：局部覆盖 → 变量渲染 → XML 封包；以及 regex_rule 输出清洗。

use std::collections::{HashMap, HashSet};

use crate::keyword::KeywordIndex;
use crate::model::ChatMessage;
use crate::models::definition::Definition;
use crate::models::message::Role;
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

/// Per-entry scan settings live on `definition.meta.scan` now (not in global
/// Settings); these are the fallbacks when an entry doesn't specify them.
pub const DEFAULT_SCAN_DEPTH: usize = 4;
pub const DEFAULT_RECURSIVE: bool = true;

/// 一个待激活条目（来自某 ref 节点解析后的定义）。
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub trigger: Trigger,
    pub content: String, // 已是有效内容（局部覆盖优先），用于递归扫描
    /// How many recent chat messages this entry's keywords scan.
    pub scan_depth: usize,
    /// Whether this entry takes part in recursive activation (as a source of
    /// scan text and as a target that recursion can activate).
    pub recursive: bool,
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

/// 计算激活集：constant 恒激活；random 按 roll；keyword 命中扫描缓冲（每条目
/// 按自己的 scan_depth 取最近若干条消息）；带 recursive 的条目把已激活内容并入
/// 缓冲再扫，直到收敛（限 3 轮）。`recent` 为最近消息（旧→新），`roll() -> [0,1)`。
pub fn activate(
    entries: &[Entry],
    recent: &[String],
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

    // keyword：按 scan_depth 分组，每组对各自的「最近 N 条」窗口扫描一次。
    let mut by_depth: HashMap<usize, Vec<(String, Vec<String>)>> = HashMap::new();
    for e in entries {
        if e.trigger.mode == TriggerMode::Keyword {
            by_depth
                .entry(e.scan_depth.max(1))
                .or_default()
                .push((e.id.clone(), e.trigger.keys.clone()));
        }
    }
    for (depth, kw) in &by_depth {
        let start = recent.len().saturating_sub(*depth);
        let window = recent[start..].join("\n");
        let index = KeywordIndex::build(kw);
        for id in index.scan(&window) {
            active.insert(id);
        }
    }

    // 递归：仅 recursive=true 的条目参与（既作扫描来源，也可被递归命中激活）。
    if entries.iter().any(|e| e.recursive) {
        let mut scan_text = String::new();
        for e in entries {
            if e.recursive && active.contains(&e.id) {
                scan_text.push('\n');
                scan_text.push_str(&e.content);
            }
        }
        let rec_kw: Vec<(String, Vec<String>)> = entries
            .iter()
            .filter(|e| e.recursive && e.trigger.mode == TriggerMode::Keyword)
            .map(|e| (e.id.clone(), e.trigger.keys.clone()))
            .collect();
        let index = KeywordIndex::build(&rec_kw);
        for _ in 0..3 {
            let mut grew = false;
            for id in index.scan(&scan_text) {
                if active.insert(id.clone()) {
                    grew = true;
                    if let Some(e) = entries.iter().find(|e| e.id == id) {
                        scan_text.push('\n');
                        scan_text.push_str(&e.content);
                    }
                }
            }
            if !grew {
                break;
            }
        }
    }

    active
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

/// 校验一条 regex_rule 的 pattern 能否编译（创作期使用；空 pattern 视为合法/无操作）。
pub fn is_valid_regex(pattern: &str) -> bool {
    regex::Regex::new(pattern).is_ok()
}

/// 依挂载顺序对文本应用 regex_rule（meta: {pattern, replacement}）。无规则返回 None。
/// 运行期宽容：非法 pattern 仅 warn 并跳过，绝不中断生成（校验在创作期做）。
pub fn apply_regex_rules(text: &str, rules: &[Definition]) -> Option<String> {
    if rules.is_empty() {
        return None;
    }
    let mut out = text.to_string();
    for rule in rules {
        // Honor ST-derived switches: disabled rules are skipped; prompt-only
        // rules don't apply to display output (prompt-side application is a
        // later slice). Default scope is "display".
        if rule.meta.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let scope = rule.meta.get("scope").and_then(|v| v.as_str()).unwrap_or("display");
        if scope == "prompt" {
            continue;
        }
        // This is the AI-output (display) path: honor `targets`. A rule that
        // explicitly lists targets but not "ai_output" doesn't apply here.
        // Missing/empty targets stay broad (apply), preserving older rules.
        if let Some(targets) = rule.meta.get("targets").and_then(|v| v.as_array()) {
            if !targets.is_empty()
                && !targets.iter().any(|t| t.as_str() == Some("ai_output"))
            {
                continue;
            }
        }
        let pattern = rule.meta.get("pattern").and_then(|v| v.as_str());
        let replacement = rule
            .meta
            .get("replacement")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(p) = pattern {
            match regex::Regex::new(p) {
                Ok(re) => out = re.replace_all(&out, replacement).into_owned(),
                Err(e) => tracing::warn!(rule = %rule.id, error = %e, "invalid regex_rule pattern, skipping"),
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
    /// regex_rule definitions referenced by enabled refs in this tree — the
    /// regex rules in effect for this loreset (scoped, not global).
    pub regex_rules: Vec<Definition>,
    /// `first_message` refs with `meta.depth` set: a message injected at a
    /// fixed distance from the end of chat history (ST's "Author's Note"
    /// depth_prompt), rather than seeded once at session creation.
    pub depth_inserts: Vec<DepthInsert>,
}

/// A message to splice into chat history at `depth` messages from the end.
#[derive(Debug, Clone)]
pub struct DepthInsert {
    pub depth: usize,
    pub role: Role,
    pub content: String,
}

/// regex_rule / first_message refs do not render as prompt segments — they are
/// consumed by their own subsystems (regex engine / session greeting seeder).
fn is_non_rendering(def_type: &str) -> bool {
    matches!(def_type, "regex_rule" | "first_message")
}

/// 取定义的有效 trigger：会话局部覆盖优先，否则用 definition.meta。
fn effective_trigger(def: &Definition, overrides: &serde_json::Value) -> Trigger {
    if let Some(t) = overrides.get(&def.id).and_then(|o| o.get("trigger")) {
        return parse_trigger(&serde_json::json!({ "trigger": t }));
    }
    parse_trigger(&def.meta)
}

/// 取定义的有效扫描设置：会话局部覆盖优先，否则取 `meta.scan`，再否则默认值。
fn effective_scan(def: &Definition, overrides: &serde_json::Value) -> (usize, bool) {
    let scan = overrides
        .get(&def.id)
        .and_then(|o| o.get("scan"))
        .or_else(|| def.meta.get("scan"));
    let depth = scan
        .and_then(|s| s.get("depth"))
        .and_then(|v| v.as_u64())
        .map(|d| d as usize)
        .unwrap_or(DEFAULT_SCAN_DEPTH);
    let recursive = scan
        .and_then(|s| s.get("recursive"))
        .and_then(|v| v.as_bool())
        .unwrap_or(DEFAULT_RECURSIVE);
    (depth.max(1), recursive)
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

/// 把定义名净化为可用作 XML 标签的字符串：trim → 连续空白折叠为单个 `_` →
/// 移除 XML 致命字符 `< > & " ' /` → 保留其余（含中文/字母/数字/`_`/`-`）。
/// 结果可能为空（名字全是被剔字符）；兜底由调用方负责。
pub fn sanitize_tag(name: &str) -> String {
    let mut out = String::new();
    let mut pending_us = false;
    for ch in name.trim().chars() {
        if ch.is_whitespace() {
            if !out.is_empty() {
                pending_us = true;
            }
            continue;
        }
        if matches!(ch, '<' | '>' | '&' | '"' | '\'' | '/') {
            continue;
        }
        if pending_us {
            out.push('_');
            pending_us = false;
        }
        out.push(ch);
    }
    out
}

/// 若 `wrap_in_tag` 开着，用其 `sanitize_tag(name)`（空则兜底 def_type）包裹内容。节点级
/// `meta.wrap_in_tag`（这一处使用的覆盖）优先于定义自身的设置，未设置则回退到定义的值。
fn maybe_wrap(def: &Definition, node: &PromptNode, content: String) -> String {
    let on = node
        .meta
        .get("wrap_in_tag")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| def.meta.get("wrap_in_tag").and_then(|v| v.as_bool()).unwrap_or(false));
    if !on {
        return content;
    }
    let mut tag = sanitize_tag(&def.name);
    if tag.is_empty() {
        tag = def.def_type.clone();
    }
    format!("<{tag}>\n{content}\n</{tag}>")
}

/// 树驱动组装：遍历节点树，按触发激活筛选 ref，容器封包，history 节点切分前后。
///
/// - 仅启用 + 激活的 ref 进入结果；空容器被省略。
/// - history 节点（启用）把后续段落落点切到 `AfterHistory` 并置 `history_enabled`。
pub fn assemble_from_nodes(
    nodes: &[PromptNode],
    definitions: &HashMap<String, Definition>,
    overrides: &serde_json::Value,
    state: &serde_json::Value,
    recent_msgs: &[String],
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
        if is_non_rendering(&def.def_type) {
            continue;
        }
        let (scan_depth, recursive) = effective_scan(def, overrides);
        entries.push(Entry {
            id: def.id.clone(),
            trigger: effective_trigger(def, overrides),
            content: render_vars(&effective_def_content(def, overrides), state),
            scan_depth,
            recursive,
        });
    }

    // 2) 计算激活集（每条目按自己的 scan_depth/recursive）。
    let active = activate(&entries, recent_msgs, roll);

    // ref 是否纳入及其渲染内容。
    let resolve = |n: &PromptNode| -> Option<String> {
        if !n.enabled || n.kind != NodeKind::Ref {
            return None;
        }
        let def = n.definition_id.as_ref().and_then(|id| definitions.get(id))?;
        if is_non_rendering(&def.def_type) {
            return None;
        }
        if !active.contains(&def.id) {
            return None;
        }
        let body = render_vars(&effective_def_content(def, overrides), state);
        Some(maybe_wrap(def, n, body))
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

    // Collect the regex_rule definitions referenced by enabled refs in this
    // tree — these are the regex rules scoped to this loreset.
    let regex_rules: Vec<Definition> = nodes
        .iter()
        .filter(|n| n.enabled && n.kind == NodeKind::Ref)
        .filter_map(|n| n.definition_id.as_ref().and_then(|id| definitions.get(id)))
        .filter(|d| d.def_type == "regex_rule")
        .cloned()
        .collect();

    // `first_message` refs that set `meta.depth` are depth inserts, not
    // session-start greetings — collected unconditionally (like the greeting
    // itself, they don't go through world-info trigger activation).
    let depth_inserts: Vec<DepthInsert> = nodes
        .iter()
        .filter(|n| n.enabled && n.kind == NodeKind::Ref)
        .filter_map(|n| n.definition_id.as_ref().and_then(|id| definitions.get(id)))
        .filter(|d| d.def_type == "first_message")
        .filter_map(|d| {
            let depth = d.meta.get("depth").and_then(|v| v.as_u64())? as usize;
            let role = match d.meta.get("role").and_then(|v| v.as_str()) {
                Some("user") => Role::User,
                Some("assistant") => Role::Assistant,
                _ => Role::System,
            };
            let content = render_vars(&effective_def_content(d, overrides), state);
            Some(DepthInsert { depth, role, content })
        })
        .collect();

    AssembledPlan { segments, history_enabled, regex_rules, depth_inserts }
}

/// 段 + 真实历史 → provider 消息数组；末了合并相邻同角色。
///
/// before-history 段作为 system 注入历史之前，after-history 段注入历史之后。
/// 落点切分已编码在各段的 `placement` 里（启用的 history 节点会把后续段切到
/// after），故此处只按调用方意图 `history_enabled` 决定是否编入真实历史：无
/// history 节点时全部段都是 before，历史自然追加其后。
pub fn build_chat_messages(
    plan: &AssembledPlan,
    history: &[ChatMessage],
    history_enabled: bool,
) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::new();
    let push_sys = |out: &mut Vec<ChatMessage>, c: &str| {
        out.push(ChatMessage { role: Role::System, content: c.to_string(), ..Default::default() });
    };

    for s in plan.segments.iter().filter(|s| s.placement == Placement::BeforeHistory) {
        push_sys(&mut out, &s.content);
    }
    if history_enabled {
        out.extend(history.iter().cloned());
    }
    for s in plan.segments.iter().filter(|s| s.placement == Placement::AfterHistory) {
        push_sys(&mut out, &s.content);
    }

    // Splice in depth inserts: each lands `depth` messages from the end of
    // `out` as currently built. Computed against the pre-insertion length and
    // sorted by position so multiple inserts don't shift each other's targets.
    let mut inserts: Vec<(usize, ChatMessage)> = plan
        .depth_inserts
        .iter()
        .map(|d| {
            let idx = out.len().saturating_sub(d.depth);
            (idx, ChatMessage { role: d.role, content: d.content.clone(), ..Default::default() })
        })
        .collect();
    inserts.sort_by_key(|(idx, _)| *idx);
    for (offset, (idx, msg)) in inserts.into_iter().enumerate() {
        out.insert(idx + offset, msg);
    }

    // 合并相邻同角色（多个 system 合一；Claude 要求 system/user 不连发）。
    let mut merged: Vec<ChatMessage> = Vec::new();
    for m in out {
        if let Some(last) = merged.last_mut() {
            if last.role == m.role {
                last.content.push('\n');
                last.content.push_str(&m.content);
                continue;
            }
        }
        merged.push(m);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::definition::Definition;
    use serde_json::json;

    fn def(t: &str, name: &str, content: &str) -> Definition {
        Definition::new(t, name, content)
    }

    #[test]
    fn sanitize_tag_folds_spaces_and_strips_fatal() {
        assert_eq!(sanitize_tag("Alice Smith"), "Alice_Smith");
        assert_eq!(sanitize_tag("  Hello   World  "), "Hello_World");
        assert_eq!(sanitize_tag("a <b>/c"), "a_bc");
        assert_eq!(sanitize_tag("主角·凛"), "主角·凛");
    }

    #[test]
    fn sanitize_tag_empty_when_all_stripped() {
        assert_eq!(sanitize_tag("<>&\"'/"), "");
    }

    fn plain_ref_node() -> PromptNode {
        PromptNode::new_ref(OwnerKind::Template, "t", None, 0, "unused")
    }

    #[test]
    fn maybe_wrap_wraps_only_when_flag_on() {
        let mut d = Definition::new("char", "Alice Smith", "body");
        let n = plain_ref_node();
        assert_eq!(maybe_wrap(&d, &n, "body".into()), "body"); // 默认关
        d.meta = serde_json::json!({ "wrap_in_tag": true });
        assert_eq!(maybe_wrap(&d, &n, "body".into()), "<Alice_Smith>\nbody\n</Alice_Smith>");
    }

    #[test]
    fn maybe_wrap_falls_back_to_def_type_when_name_empty() {
        let mut d = Definition::new("world", "<>", "body");
        d.meta = serde_json::json!({ "wrap_in_tag": true });
        assert_eq!(maybe_wrap(&d, &plain_ref_node(), "body".into()), "<world>\nbody\n</world>");
    }

    #[test]
    fn maybe_wrap_node_override_takes_priority_over_definition() {
        let mut d = Definition::new("char", "Hero", "body");
        d.meta = serde_json::json!({ "wrap_in_tag": true });
        let mut n = plain_ref_node();
        // node-level override turns wrapping back off even though the definition has it on
        n.meta = serde_json::json!({ "wrap_in_tag": false });
        assert_eq!(maybe_wrap(&d, &n, "body".into()), "body");

        // and the reverse: definition off, node-level override turns it on
        d.meta = serde_json::json!({});
        n.meta = serde_json::json!({ "wrap_in_tag": true });
        assert_eq!(maybe_wrap(&d, &n, "body".into()), "<Hero>\nbody\n</Hero>");
    }

    #[test]
    fn ref_node_wraps_content_when_definition_flag_on() {
        let mut d = def("char", "Hero", "I am hero");
        d.meta = serde_json::json!({ "wrap_in_tag": true });
        let r = PromptNode::new_ref(OwnerKind::Template, "t", None, 0, &d.id);
        let mut defs = std::collections::HashMap::new();
        defs.insert(d.id.clone(), d);
        let plan = assemble_from_nodes(
            &[r],
            &defs,
            &serde_json::json!({}),
            &serde_json::json!({}),
            &[],
            &mut || 0.0,
        );
        assert_eq!(plan.segments.len(), 1);
        assert_eq!(plan.segments[0].content, "<Hero>\nI am hero\n</Hero>");
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
    fn regex_rules_clean_text() {
        let mut r = def("regex_rule", "r", "");
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
            scan_depth: DEFAULT_SCAN_DEPTH,
            recursive: DEFAULT_RECURSIVE,
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
        let active = activate(&entries, &["tell me about zion".to_string()], &mut || 0.0);
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
            scan_depth: DEFAULT_SCAN_DEPTH,
            recursive: DEFAULT_RECURSIVE,
        }];
        assert!(activate(&entries, &[], &mut || 0.2).contains("r")); // 0.2 < 0.5
        assert!(!activate(&entries, &[], &mut || 0.9).contains("r"));
    }

    #[test]
    fn activate_recursive_chains() {
        // "zion" not in chat, but "neo" constant content mentions zion → recursion activates zion.
        let neo = ent("neo", TriggerMode::Constant, &[], "Neo lives in zion");
        let zion = ent("zion", TriggerMode::Keyword, &["zion"], "Zion body");
        assert!(activate(&[neo.clone(), zion.clone()], &["hi".to_string()], &mut || 0.0).contains("zion"));

        // With recursion disabled per-entry, the chain doesn't form.
        let mut neo_nr = neo;
        neo_nr.recursive = false;
        let mut zion_nr = zion;
        zion_nr.recursive = false;
        assert!(!activate(&[neo_nr, zion_nr], &["hi".to_string()], &mut || 0.0).contains("zion"));
    }

    #[test]
    fn activate_keyword_respects_per_entry_scan_depth() {
        // "zion" appears only in the oldest of three messages; depth 1 misses it,
        // depth 3 catches it.
        let recent = ["mention zion".to_string(), "b".to_string(), "c".to_string()];
        let mut shallow = ent("z", TriggerMode::Keyword, &["zion"], "Z");
        shallow.scan_depth = 1;
        shallow.recursive = false;
        assert!(!activate(&[shallow], &recent, &mut || 0.0).contains("z"));

        let mut deep = ent("z", TriggerMode::Keyword, &["zion"], "Z");
        deep.scan_depth = 3;
        deep.recursive = false;
        assert!(activate(&[deep], &recent, &mut || 0.0).contains("z"));
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
        let neo = def("char", "Neo", "Neo body");
        let jb = def("prompt", "JB", "Jailbreak body");
        let charf = folder_node("t", 0, "char");
        let cref = child_ref("t", &charf.id, 0, &neo.id);
        let hist = history_node("t", 1);
        let after = root_ref("t", 2, &jb.id);

        let mut defs = std::collections::HashMap::new();
        defs.insert(neo.id.clone(), neo.clone());
        defs.insert(jb.id.clone(), jb.clone());

        let nodes = vec![charf, cref, hist, after];
        let plan = assemble_from_nodes(
            &nodes, &defs, &json!({}), &json!({}), &["hi".to_string()], &mut || 0.0,
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
        let mut lore = def("world", "Zion", "Zion body");
        lore.meta = json!({ "trigger": { "mode": "keyword", "keys": ["zion"] } });
        let wf = folder_node("t", 0, "world");
        let wref = child_ref("t", &wf.id, 0, &lore.id);
        let mut defs = std::collections::HashMap::new();
        defs.insert(lore.id.clone(), lore.clone());
        let nodes = vec![wf, wref];
        // No "zion" in buffer → world container empty → omitted.
        let plan = assemble_from_nodes(
            &nodes, &defs, &json!({}), &json!({}), &["hi".into()], &mut || 0.0,
        );
        assert!(plan.segments.is_empty());
    }

    use crate::model::ChatMessage;
    use crate::models::message::Role;

    fn seg(p: Placement, content: &str) -> PromptSegment {
        PromptSegment { placement: p, content: content.into(), source: content.into() }
    }

    #[test]
    fn non_rendering_refs_skipped_and_regex_collected() {
        let mut rx = def("regex_rule", "R", "");
        rx.meta = serde_json::json!({ "pattern": "X", "replacement": "Y" });
        let fm = def("first_message", "Hi", "hello there");
        let neo = def("char", "Neo", "Neo body");
        let charf = folder_node("t", 0, "char");
        let cref = child_ref("t", &charf.id, 0, &neo.id);
        let rxref = root_ref("t", 1, &rx.id);
        let fmref = root_ref("t", 2, &fm.id);

        let mut defs = HashMap::new();
        for d in [&rx, &fm, &neo] {
            defs.insert(d.id.clone(), d.clone());
        }
        let nodes = vec![charf, cref, rxref, fmref];
        let mut roll = || 0.0;
        let plan = assemble_from_nodes(
            &nodes,
            &defs,
            &serde_json::json!({}),
            &serde_json::json!({}),
            &[],
            &mut roll,
        );

        // regex_rule / first_message do not render into prompt segments
        let joined: String = plan.segments.iter().map(|s| s.content.clone()).collect();
        assert!(joined.contains("Neo body"));
        assert!(!joined.contains("hello there"));
        // regex rules collected from the tree
        assert_eq!(plan.regex_rules.len(), 1);
        assert_eq!(plan.regex_rules[0].name, "R");
    }

    #[test]
    fn apply_regex_honors_disabled_and_scope() {
        let mut on = Definition::new("regex_rule", "on", "");
        on.meta = serde_json::json!({ "pattern": "a", "replacement": "b" });
        let mut off = Definition::new("regex_rule", "off", "");
        off.meta = serde_json::json!({ "pattern": "a", "replacement": "Z", "disabled": true });
        let mut prompt_only = Definition::new("regex_rule", "po", "");
        prompt_only.meta = serde_json::json!({ "pattern": "b", "replacement": "Q", "scope": "prompt" });
        // disabled is skipped; scope=prompt doesn't touch display: only `on` applies a->b
        let out = apply_regex_rules("aaa", &[on, off, prompt_only]).unwrap();
        assert_eq!(out, "bbb");
    }

    #[test]
    fn apply_regex_honors_targets_for_ai_output() {
        // The display path operates on AI output. A rule scoped to user_input
        // only must not touch it; ai_output (and missing/empty targets) apply.
        let mut ai = Definition::new("regex_rule", "ai", "");
        ai.meta = serde_json::json!({ "pattern": "a", "replacement": "b", "targets": ["ai_output"] });
        let mut user = Definition::new("regex_rule", "user", "");
        user.meta = serde_json::json!({ "pattern": "b", "replacement": "Z", "targets": ["user_input"] });
        // `ai` applies a->b; the user_input-only rule is skipped on this path.
        let out = apply_regex_rules("aaa", &[ai, user]).unwrap();
        assert_eq!(out, "bbb");
    }

    #[test]
    fn build_messages_merges_same_role_and_inserts_history() {
        let plan = AssembledPlan {
            segments: vec![
                seg(Placement::BeforeHistory, "A"),
                seg(Placement::BeforeHistory, "B"),
                seg(Placement::AfterHistory, "JB"),
            ],
            history_enabled: true,
            regex_rules: vec![],
            depth_inserts: vec![],
        };
        let history = vec![ChatMessage { role: Role::User, content: "hi".into(), ..Default::default() }];
        let msgs = build_chat_messages(&plan, &history, true);
        // [system "A\nB", user "hi", system "JB"]
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, Role::System);
        assert_eq!(msgs[0].content, "A\nB");
        assert_eq!(msgs[1].role, Role::User);
        assert_eq!(msgs[2].role, Role::System);
        assert_eq!(msgs[2].content, "JB");
    }

    #[test]
    fn build_messages_history_disabled_drops_history_and_merges() {
        let plan = AssembledPlan {
            segments: vec![seg(Placement::BeforeHistory, "A"), seg(Placement::AfterHistory, "B")],
            history_enabled: false,
            regex_rules: vec![],
            depth_inserts: vec![],
        };
        let history = vec![ChatMessage { role: Role::User, content: "hi".into(), ..Default::default() }];
        let msgs = build_chat_messages(&plan, &history, false);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::System);
        assert_eq!(msgs[0].content, "A\nB");
    }

    #[test]
    fn build_messages_splices_depth_insert_into_history() {
        let plan = AssembledPlan {
            segments: vec![],
            history_enabled: true,
            regex_rules: vec![],
            depth_inserts: vec![DepthInsert { depth: 1, role: Role::System, content: "note".into() }],
        };
        let history = vec![
            ChatMessage { role: Role::User, content: "a".into(), ..Default::default() },
            ChatMessage { role: Role::Assistant, content: "b".into(), ..Default::default() },
            ChatMessage { role: Role::User, content: "c".into(), ..Default::default() },
        ];
        let msgs = build_chat_messages(&plan, &history, true);
        // depth 1 lands before the last message ("c").
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[2].role, Role::System);
        assert_eq!(msgs[2].content, "note");
        assert_eq!(msgs[3].content, "c");
    }

    #[test]
    fn assemble_collects_depth_insert_from_first_message_ref() {
        let mut d = Definition::new("first_message", "note", "Remember: {{x}}");
        d.meta = json!({ "depth": 2, "role": "system" });
        let mut defs = HashMap::new();
        defs.insert(d.id.clone(), d.clone());
        let node = root_ref("template", 0, &d.id);
        let state = json!({ "x": "stay calm" });
        let plan = assemble_from_nodes(&[node], &defs, &json!({}), &state, &[], &mut || 0.0);
        assert_eq!(plan.depth_inserts.len(), 1);
        assert_eq!(plan.depth_inserts[0].depth, 2);
        assert_eq!(plan.depth_inserts[0].role, Role::System);
        assert_eq!(plan.depth_inserts[0].content, "Remember: stay calm");
    }
}
