//! Prompt assembly: partial substitution → variable rendering → XML wrapping; and regex_rule output sanitization.

use std::collections::{HashMap, HashSet};

use crate::keyword::KeywordIndex;
use crate::model::ChatMessage;
use crate::models::definition::Definition;
use crate::models::message::Role;
use crate::models::prompt_node::{NodeKind, PromptNode};
use crate::state::{Action, Update};

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

/// An entry awaiting activation (from a definition parsed from a ref node).
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub trigger: Trigger,
    pub content: String, // Already valid content (partial overwriting takes precedence), used for recursive scanning
    /// How many recent chat messages this entry's keywords scan.
    pub scan_depth: usize,
    /// Whether this entry takes part in recursive activation (as a source of
    /// scan text and as a target that recursion can activate).
    pub recursive: bool,
}

/// Parse the trigger from `definition.meta` (i.e., the entire meta object); the default is `constant`.
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

/// Calculate the activation set: constant = always active; random = based on roll; keyword = hits the scan buffer (For each entry
/// retrieve the most recent messages based on its own `scan_depth`); entries marked as `recursive` merge the already activated content into
/// the buffer and scan again, until convergence (limited to 3 rounds). `recent` refers to the most recent messages (old to new), and `roll()` returns [0,1).
pub fn activate(
    entries: &[Entry],
    recent: &[String],
    roll: &mut impl FnMut() -> f64,
) -> HashSet<String> {
    let mut active: HashSet<String> = HashSet::new();

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

    // keyword: Group by scan_depth, and scan each group's “most recent N” window once.
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

    // Recursion: Only entries with `recursive=true` are included (they serve as both scan sources and can be activated by recursive hits).
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

/// Render `{{var}}` using state; unknown keys retain their original placeholders.
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

/// Strip `{{// ... }}` authoring comments. Linear scan (no regex → no
/// catastrophic backtracking): find each `{{//`, drop through the next `}}`.
/// A comment alone on its line takes the line's leading whitespace and one
/// trailing newline with it. An unterminated `{{//` strips to end of input.
pub fn strip_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{//") {
        out.push_str(&rest[..start]);

        // Find the close, balancing nested {{ }} so the comment body may contain
        // {{var}}-looking text. Depth starts at 1 for the opening "{{". `{`/`}`
        // are ASCII, so byte scanning is UTF-8 safe.
        let bytes = rest.as_bytes();
        let len = rest.len();
        let mut depth = 1i32;
        let mut j = start + 2; // scan after the opening "{{"
        let mut end = len; // unterminated → strip to end
        while j + 1 < len {
            if bytes[j] == b'{' && bytes[j + 1] == b'{' {
                depth += 1;
                j += 2;
            } else if bytes[j] == b'}' && bytes[j + 1] == b'}' {
                depth -= 1;
                j += 2;
                if depth == 0 {
                    end = j;
                    break;
                }
            } else {
                j += 1;
            }
        }
        let after = &rest[end..];

        // Whole-line comment: emitted text ends at a line start (only ws since
        // the last newline) → drop the line's leading ws and one trailing '\n'.
        let line_start = out.rsplit_once('\n').map(|(_, t)| t).unwrap_or(&out[..]);
        if line_start.trim().is_empty() {
            let cut = out.len() - line_start.len();
            out.truncate(cut);
            let trimmed = after.trim_start_matches([' ', '\t']);
            rest = trimmed.strip_prefix('\n').unwrap_or(trimmed);
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// ST stores `findRegex` either as a raw pattern or as a JS regex literal
/// `/pattern/flags` (e.g. `/<update>(.*)<\/update>/gsi`). Strip the delimiters
/// and translate supported flags into fancy-regex's inline `(?ismx)` syntax
/// so both forms compile to the same regex (`g` has no inline equivalent —
/// `replace_all` is already "replace every match").
pub(crate) fn normalize_js_regex_literal(pattern: &str) -> String {
    let bytes = pattern.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'/' {
        return pattern.to_string();
    }
    let Some(close) = pattern.rfind('/').filter(|&i| i > 0) else { return pattern.to_string() };
    let flags = &pattern[close + 1..];
    if !flags.bytes().all(|b| matches!(b, b'i' | b's' | b'm' | b'x' | b'g' | b'u' | b'y')) {
        return pattern.to_string(); // trailing "/..." isn't a flag set — treat as a literal pattern
    }
    let body = &pattern[1..close];
    let inline: String = flags.chars().filter(|c| matches!(c, 'i' | 's' | 'm' | 'x')).collect();
    if inline.is_empty() {
        body.to_string()
    } else {
        format!("(?{inline}){body}")
    }
}

/// Checks whether a regex_rule pattern can be compiled (for use during development; an empty pattern is considered valid and results in no action).
/// Uses the fancy-regex engine (supports lookarounds and backreferences, and is ST-compatible).
pub fn is_valid_regex(pattern: &str) -> bool {
    fancy_regex::Regex::new(&normalize_js_regex_literal(pattern)).is_ok()
}

/// Compilation error message (a valid or empty pattern returns `None`), used by the UI to flag invalid rules.
pub fn regex_error(pattern: &str) -> Option<String> {
    if pattern.is_empty() {
        return None;
    }
    fancy_regex::Regex::new(&normalize_js_regex_literal(pattern)).err().map(|e| e.to_string())
}

/// The target of the regex_rule (which side of the message).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegexTarget {
    AiOutput,
    UserInput,
}
/// regex_rule processing stage (modifies the content displayed / sent to the model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegexPhase {
    Display,
    Prompt,
}

/// Applies the applicable regex_rule to the text in the order they are mounted. Filters by (target, phase):
/// `disabled` skips the rule; phase must match `scope` (`display` → {`display`, `both`}, `prompt` → {`prompt`, `both`});
/// `target` must be in `targets` (empty/default = generic). Returning `None` indicates that no applicable rules were actually executed.
/// runtime tolerance: Invalid patterns are simply skipped with a warning (validation is performed at creation time).
pub fn apply_regex_rules_for(
    text: &str,
    rules: &[Definition],
    target: RegexTarget,
    phase: RegexPhase,
) -> Option<String> {
    let target_key = match target {
        RegexTarget::AiOutput => "ai_output",
        RegexTarget::UserInput => "user_input",
    };
    let mut out = text.to_string();
    let mut ran = false;
    for rule in rules {
        if rule.meta.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let scope = rule.meta.get("scope").and_then(|v| v.as_str()).unwrap_or("display");
        let phase_ok = match phase {
            RegexPhase::Display => scope == "display" || scope == "both",
            RegexPhase::Prompt => scope == "prompt" || scope == "both",
        };
        if !phase_ok {
            continue;
        }
        if let Some(targets) = rule.meta.get("targets").and_then(|v| v.as_array()) {
            if !targets.is_empty() && !targets.iter().any(|t| t.as_str() == Some(target_key)) {
                continue;
            }
        }
        let pattern = rule.meta.get("pattern").and_then(|v| v.as_str());
        let replacement = rule.meta.get("replacement").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(p) = pattern {
            match fancy_regex::Regex::new(&normalize_js_regex_literal(p)) {
                Ok(re) => {
                    out = re.replace_all(&out, replacement).into_owned();
                    ran = true;
                }
                Err(e) => tracing::warn!(rule = %rule.id, error = %e, "invalid regex_rule pattern, skipping"),
            }
        }
    }
    ran.then_some(out)
}

/// Convenient wrapper for AI output and display (preserving the semantics of the old call points).
pub fn apply_regex_rules(text: &str, rules: &[Definition]) -> Option<String> {
    apply_regex_rules_for(text, rules, RegexTarget::AiOutput, RegexPhase::Display)
}

/// Pull values for a converted status-bar panel's variables out of one AI
/// turn's raw text, using the same `pattern` that also drives the
/// compatibility-layer display replace in `apply_regex_rules_for` — but
/// read-only (`captures`, never `replace_all`): this never touches what's
/// shown to the user, only extracts values to fold into the session's
/// persistent variable state via the same `apply_updates` call already used
/// for `<state_update>` tags. Only rules carrying `meta.capture_vars`
/// (written by `adapters::charcard::try_convert_status_panel`) participate;
/// every other regex_rule is untouched and contributes nothing here.
pub fn capture_panel_updates(text: &str, rules: &[Definition]) -> Vec<Update> {
    let mut out = Vec::new();
    for rule in rules {
        if rule.meta.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some(capture_vars) = rule.meta.get("capture_vars").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(pattern) = rule.meta.get("pattern").and_then(|v| v.as_str()) else { continue };
        let Ok(re) = fancy_regex::Regex::new(&normalize_js_regex_literal(pattern)) else { continue };
        let Ok(Some(caps)) = re.captures(text) else { continue };
        for (i, name) in capture_vars.iter().enumerate() {
            let Some(name) = name.as_str() else { continue }; // null slot -> no variable for this group
            if let Some(m) = caps.get(i + 1) {
                out.push(Update { action: Action::Set, key: name.to_string(), value: Some(m.as_str().to_string()) });
            }
        }
    }
    out
}

/// Paragraph break: Before / after the historical message node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    BeforeHistory,
    AfterHistory,
}

/// A “system” section from the assembled output (un concatenated, with source and destination retained).
#[derive(Debug, Clone)]
pub struct PromptSegment {
    pub placement: Placement,
    pub content: String,
    /// Source identifier (container tag or defined ID) for debugging and exporting.
    pub source: String,
}

/// Structured results of tree-driven assembly: paragraphs + whether true history is enabled.
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

/// regex_rule / first_message / html / css / variables refs do not render as prompt
/// segments — they are consumed by their own subsystems (regex engine /
/// session greeting seeder / panel builder / state manager), never emitted into the LLM prompt.
fn is_non_rendering(def_type: &str) -> bool {
    matches!(def_type, "regex_rule" | "first_message" | "html" | "css" | "variables")
}

/// Retrieve the valid trigger from the definition: session-local overrides take precedence; otherwise, use `definition.meta`.
fn effective_trigger(def: &Definition, overrides: &serde_json::Value) -> Trigger {
    if let Some(t) = overrides.get(&def.id).and_then(|o| o.get("trigger")) {
        return parse_trigger(&serde_json::json!({ "trigger": t }));
    }
    parse_trigger(&def.meta)
}

/// Retrieve the valid scan settings: session-local overrides take precedence; otherwise, use `meta.scan`; otherwise, use the default value.
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

/// Retrieve the valid content from the definition: session-local overrides (structured {content}) take precedence; otherwise, use the global content.
fn effective_def_content(def: &Definition, overrides: &serde_json::Value) -> String {
    overrides
        .get(&def.id)
        .and_then(|o| o.get("content"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| def.content.clone())
}

/// Sanitize the defined name into a string suitable for use as an XML tag: trim → collapse consecutive whitespace into a single `_` →
/// Remove XML-invalid characters `< > & " ' /` → retain the rest (including Chinese characters, letters, numbers, `_`, and `-`).
/// The result may be empty (if the name consists entirely of disallowed characters); the caller is responsible for handling such cases.
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
    // Guarantee a valid XML name start: a non-empty name that starts with a
    // digit/punctuation is prefixed; empty stays empty (callers fall back to def_type).
    match out.chars().next() {
        Some(c) if c.is_alphabetic() || c == '_' => out,
        Some(_) => format!("tag_{out}"),
        None => out,
    }
}

/// If `wrap_in_tag` is enabled, wrap the content using its `sanitize_tag(name)` (falling back to `def_type` if empty). Node-level
/// `meta.wrap_in_tag` (overridden here) takes precedence over the setting defined within the node itself; if not set, it falls back to the defined value.
fn maybe_wrap(def: &Definition, node: &PromptNode, content: String) -> String {
    let on = node
        .meta
        .get("wrap_in_tag")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| def.meta.get("wrap_in_tag").and_then(|v| v.as_bool()).unwrap_or(false));
    // Don't wrap empty/whitespace content — a bare `<tag></tag>` is noise the
    // model has to read past (and may misread as a real, empty section).
    if !on || content.trim().is_empty() {
        return content;
    }
    let mut tag = sanitize_tag(&def.name);
    if tag.is_empty() {
        tag = def.def_type.clone();
    }
    format!("<{tag}>\n{content}\n</{tag}>")
}

/// Back-compat: assemble a single owner tree with no mounted packs.
pub fn assemble_from_nodes(
    nodes: &[PromptNode],
    definitions: &HashMap<String, Definition>,
    overrides: &serde_json::Value,
    state: &serde_json::Value,
    recent_msgs: &[String],
    roll: &mut impl FnMut() -> f64,
) -> AssembledPlan {
    assemble_from_nodes_with_packs(nodes, &[], definitions, overrides, state, recent_msgs, roll)
}

/// Tree-driven assembly: Traverse the node tree, filter refs based on triggers and activations, package containers, and split history nodes into “before” and “after” segments.
/// Accepts an optional set of mount package trees (pack_trees) and injects package content into content nodes grouped by type.
///
/// - Only refs that are both enabled and activated are included in the result; empty containers are omitted.
/// - The history node (when enabled) shifts the landing point of subsequent segments to `AfterHistory` and sets `history_enabled`.
pub fn assemble_from_nodes_with_packs(
    nodes: &[PromptNode],
    pack_trees: &[Vec<PromptNode>],
    definitions: &HashMap<String, Definition>,
    overrides: &serde_json::Value,
    state: &serde_json::Value,
    recent_msgs: &[String],
    roll: &mut impl FnMut() -> f64,
) -> AssembledPlan {
    // Build an Entry from all ref nodes (used for activation computation).
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
            content: render_vars(&strip_comments(&effective_def_content(def, overrides)), state),
            scan_depth,
            recursive,
        });
    }

    // Pack refs share the single activation pass (spec §8): same scan buffer and
    // recursion budget as the template/session tree.
    for pack in pack_trees {
        for n in pack {
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
                content: render_vars(&strip_comments(&effective_def_content(def, overrides)), state),
                scan_depth,
                recursive,
            });
        }
    }

    // Compute the activation set (each entry is processed according to its own `scan_depth/recursive`).
    let active = activate(&entries, recent_msgs, roll);

    // Whether to include the reference and its rendered content.
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
        let body = render_vars(&strip_comments(&effective_def_content(def, overrides)), state);
        Some(maybe_wrap(def, n, body))
    };

    // Renders a pack ref's body WITHOUT a per-node tag wrap (the type grouping at
    // the content node owns the wrapping). Returns (def_type, body) when enabled,
    // world-info-active, rendering, and non-empty.
    let render_pack_body = |n: &PromptNode| -> Option<(String, String)> {
        if !n.enabled || n.kind != NodeKind::Ref {
            return None;
        }
        let def = n.definition_id.as_ref().and_then(|id| definitions.get(id))?;
        if is_non_rendering(&def.def_type) || !active.contains(&def.id) {
            return None;
        }
        let body = render_vars(&strip_comments(&effective_def_content(def, overrides)), state);
        (!body.trim().is_empty()).then(|| (def.def_type.clone(), body))
    };

    // Walks one pack tree (root refs + one level of folders), honoring select=one,
    // yielding (def_type, body) pairs in walk order. Pack folder tags are ignored
    // here — the content node groups by type. (Nested folder-tag wrapping in packs
    // is out of scope for this plan.)
    let pack_pairs = |pack: &[PromptNode]| -> Vec<(String, String)> {
        let mut roots: Vec<&PromptNode> = pack.iter().filter(|n| n.parent_id.is_none()).collect();
        roots.sort_by_key(|n| n.sort_order);
        let mut pairs: Vec<(String, String)> = Vec::new();
        for root in roots {
            match root.kind {
                NodeKind::Ref => {
                    if let Some(p) = render_pack_body(root) {
                        pairs.push(p);
                    }
                }
                NodeKind::Folder => {
                    if !root.enabled {
                        continue;
                    }
                    let select_one =
                        root.meta.get("select").and_then(|v| v.as_str()) == Some("one");
                    let mut kids: Vec<&PromptNode> = pack
                        .iter()
                        .filter(|n| n.parent_id.as_deref() == Some(root.id.as_str()))
                        .collect();
                    kids.sort_by_key(|n| n.sort_order);
                    for k in kids {
                        if let Some(p) = render_pack_body(k) {
                            pairs.push(p);
                            if select_one {
                                break;
                            }
                        }
                    }
                }
                _ => {} // packs hold no history/content nodes
            }
        }
        pairs
    };

    // 3) Iterate through the root nodes in sort_order; use history to switch the landing point.
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
            NodeKind::Content => {
                if !root.enabled {
                    continue;
                }
                // Gather (type, body) from all packs (mount order), group by type
                // preserving first-appearance order, emit one <type>…</type> segment.
                let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
                for pack in pack_trees {
                    for (ty, body) in pack_pairs(pack) {
                        match grouped.iter_mut().find(|(t, _)| *t == ty) {
                            Some((_, bodies)) => bodies.push(body),
                            None => grouped.push((ty, vec![body])),
                        }
                    }
                }
                for (ty, bodies) in grouped {
                    let mut tag = sanitize_tag(&ty);
                    if tag.is_empty() {
                        tag = "content".to_string();
                    }
                    segments.push(PromptSegment {
                        placement,
                        content: format!("<{tag}>\n{}\n</{tag}>", bodies.join("\n")),
                        source: format!("pack:{ty}"),
                    });
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
                let bodies: Vec<String> = children
                    .iter()
                    .filter_map(|c| resolve(c))
                    .filter(|b| !b.trim().is_empty())
                    .collect();
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
                    if content.trim().is_empty() {
                        continue;
                    }
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
            let content = render_vars(&strip_comments(&effective_def_content(d, overrides)), state);
            Some(DepthInsert { depth, role, content })
        })
        .collect();

    AssembledPlan { segments, history_enabled, regex_rules, depth_inserts }
}

/// Segment + actual history → provider message array; finally, merge adjacent segments with the same role.
///
/// The “before-history” segment is before the history is injected as “system,” and the “after-history” segment is after the history is injected.
/// The placement of segments is encoded in the `placement` of each segment (if a `history` node is enabled, subsequent segments are moved to
/// `after`); therefore, whether to include them in the actual history is determined solely by the caller’s intent via `history_enabled`: if there is no
/// `history` node, all segments are `before`, and the history is naturally appended after them.
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

    #[test]
    fn content_node_injects_packs_grouped_by_type_with_select_one() {
        use std::collections::HashMap;
        // template: content node (sort 0) then history (sort 1)
        let mut content = PromptNode::new_folder(OwnerKind::Template, "t", None, 0, "content");
        content.kind = NodeKind::Content;
        content.tag = None;
        let mut hist = PromptNode::new_folder(OwnerKind::Template, "t", None, 1, "history");
        hist.kind = NodeKind::History;
        hist.tag = None;
        let tmpl = vec![content, hist];

        // pack: ref char "Alice profile" + a select=one folder of two char variants
        let mut alice = PromptNode::new_ref(OwnerKind::Pack, "p", None, 0, "d_alice");
        let mut mood = PromptNode::new_folder(OwnerKind::Pack, "p", None, 1, "mood");
        mood.tag = None;
        mood.meta = serde_json::json!({ "select": "one" });
        let happy = PromptNode::new_ref(OwnerKind::Pack, "p", Some(mood.id.clone()), 0, "d_happy");
        let angry = PromptNode::new_ref(OwnerKind::Pack, "p", Some(mood.id.clone()), 1, "d_angry");
        let pack = vec![alice.clone(), mood, happy, angry];

        let mut defs: HashMap<String, Definition> = HashMap::new();
        for (id, body) in [("d_alice", "Alice profile"), ("d_happy", "Happy Alice"), ("d_angry", "Angry Alice")] {
            let mut d = Definition::new("char", id, body);
            d.id = id.to_string();
            defs.insert(d.id.clone(), d);
        }
        let _ = &mut alice;

        let plan = assemble_from_nodes_with_packs(
            &tmpl, std::slice::from_ref(&pack), &defs,
            &serde_json::json!({}), &serde_json::json!({}), &[], &mut || 0.0,
        );
        let char_seg = plan.segments.iter().find(|s| s.source == "pack:char").expect("a char content segment");
        assert_eq!(char_seg.placement, Placement::BeforeHistory);
        assert!(char_seg.content.starts_with("<char>") && char_seg.content.ends_with("</char>"));
        assert!(char_seg.content.contains("Alice profile"));
        assert!(char_seg.content.contains("Happy Alice"));
        assert!(!char_seg.content.contains("Angry Alice"), "select=one keeps only the first child");
    }

    #[test]
    fn empty_pack_trees_render_no_content_segments() {
        use std::collections::HashMap;
        let mut content = PromptNode::new_folder(OwnerKind::Template, "t", None, 0, "content");
        content.kind = NodeKind::Content;
        content.tag = None;
        let plan = assemble_from_nodes_with_packs(
            &[content], &[], &HashMap::new(),
            &serde_json::json!({}), &serde_json::json!({}), &[], &mut || 0.0,
        );
        assert!(plan.segments.iter().all(|s| !s.source.starts_with("pack:")));
    }

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

    #[test]
    fn sanitize_tag_prefixes_invalid_xml_start() {
        assert_eq!(sanitize_tag("123 核心"), "tag_123_核心"); // digit start -> prefixed
        assert_eq!(sanitize_tag("Alice Smith"), "Alice_Smith"); // letter start -> unchanged
        assert_eq!(sanitize_tag("主角·凛"), "主角·凛"); // CJK letter start -> unchanged
        assert_eq!(sanitize_tag("<>&\"'/"), ""); // all stripped -> empty (caller falls back)
    }

    fn plain_ref_node() -> PromptNode {
        PromptNode::new_ref(OwnerKind::Template, "t", None, 0, "unused")
    }

    #[test]
    fn maybe_wrap_wraps_only_when_flag_on() {
        let mut d = Definition::new("char", "Alice Smith", "body");
        let n = plain_ref_node();
        assert_eq!(maybe_wrap(&d, &n, "body".into()), "body");
        d.meta = serde_json::json!({ "wrap_in_tag": true });
        assert_eq!(maybe_wrap(&d, &n, "body".into()), "<Alice_Smith>\nbody\n</Alice_Smith>");
    }

    #[test]
    fn maybe_wrap_skips_empty_content_to_avoid_empty_tag() {
        // A wrap_in_tag def whose rendered content is empty/whitespace must not
        // emit a bare `<tag></tag>` placeholder.
        let mut d = Definition::new("char", "Alice", "");
        d.meta = serde_json::json!({ "wrap_in_tag": true });
        let n = plain_ref_node();
        assert_eq!(maybe_wrap(&d, &n, "".into()), "");
        assert_eq!(maybe_wrap(&d, &n, "   \n".into()), "   \n"); // whitespace passes through unwrapped
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
    fn strip_comments_inline_and_whole_line() {
        assert_eq!(strip_comments("Hi {{// note}}there"), "Hi there");
        assert_eq!(strip_comments("a\n{{// c}}\nb"), "a\nb");
        assert_eq!(strip_comments("keep {{// x}} mid {{// y}} end"), "keep  mid  end");
        assert_eq!(strip_comments("x {{// unterminated"), "x ");
        assert_eq!(strip_comments("plain {{name}} text"), "plain {{name}} text");
    }

    #[test]
    fn strip_comments_runs_before_var_render() {
        // a comment may contain {{var}}-looking text; it must not be substituted
        let s = json!({ "name": "Neo" });
        assert_eq!(render_vars(&strip_comments("{{// {{name}} }}hi {{name}}"), &s), "hi Neo");
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

    #[test]
    fn capture_panel_updates_extracts_named_groups_into_set_updates() {
        let mut r = def("regex_rule", "status", "");
        r.meta = json!({
            "pattern": "<hp>(\\d+)</hp> <mood>(\\w+)</mood>",
            "replacement": "HP: $1 ($2)",
            "capture_vars": ["hp", "mood"]
        });
        let updates = capture_panel_updates("text <hp>42</hp> <mood>calm</mood> more", &[r]);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0], Update { action: Action::Set, key: "hp".to_string(), value: Some("42".to_string()) });
        assert_eq!(updates[1], Update { action: Action::Set, key: "mood".to_string(), value: Some("calm".to_string()) });
    }

    #[test]
    fn capture_panel_updates_skips_rules_without_capture_vars() {
        let mut r = def("regex_rule", "plain", "");
        r.meta = json!({ "pattern": "x", "replacement": "y" }); // no capture_vars -> not a panel-sync rule
        assert_eq!(capture_panel_updates("x", &[r]), Vec::new());
    }

    #[test]
    fn capture_panel_updates_no_match_yields_no_updates() {
        let mut r = def("regex_rule", "status", "");
        r.meta = json!({ "pattern": "<hp>(\\d+)</hp>", "replacement": "$1", "capture_vars": ["hp"] });
        assert_eq!(capture_panel_updates("no tags here", &[r]), Vec::new());
    }

    #[test]
    fn capture_panel_updates_skips_disabled_rules() {
        let mut r = def("regex_rule", "status", "");
        r.meta = json!({
            "pattern": "<hp>(\\d+)</hp>", "replacement": "$1", "capture_vars": ["hp"],
            "disabled": true,
        });
        assert_eq!(capture_panel_updates("<hp>42</hp>", &[r]), Vec::new());
    }

    #[test]
    fn capture_panel_updates_honors_null_slots_in_capture_vars() {
        // group 1 has no associated variable (out-of-range $N case from Task 3) -> skipped.
        let mut r = def("regex_rule", "status", "");
        r.meta = json!({
            "pattern": "<a>(.)</a><b>(.)</b>",
            "replacement": "$2",
            "capture_vars": [null, "field2"]
        });
        let updates = capture_panel_updates("<a>X</a><b>Y</b>", &[r]);
        assert_eq!(updates, vec![Update { action: Action::Set, key: "field2".to_string(), value: Some("Y".to_string()) }]);
    }

    #[test]
    fn is_valid_regex_accepts_lookaround() {
        // Plain `regex` rejects lookahead; fancy-regex accepts it.
        assert!(is_valid_regex(r"foo(?=bar)"));
        assert!(is_valid_regex(r"(?<=\d)px"));
        assert!(is_valid_regex(r"(\w+)\s+\1")); // backreference
        assert!(!is_valid_regex(r"foo(")); // still invalid: unbalanced paren
    }

    #[test]
    fn regex_error_reports_only_invalid() {
        assert!(regex_error(r"\d+").is_none());
        assert!(regex_error(r"foo(?=bar)").is_none()); // valid under fancy-regex
        assert!(regex_error(r"foo(").is_some()); // unbalanced paren -> error string
    }

    #[test]
    fn apply_regex_rules_supports_lookaround() {
        // Strip a trailing "px" only when preceded by digits (lookbehind).
        let mut r = def("regex_rule", "r", "");
        r.meta = json!({ "pattern": r"(?<=\d)px", "replacement": "" });
        assert_eq!(apply_regex_rules("12px and apx", &[r]).as_deref(), Some("12 and apx"));
    }

    #[test]
    fn apply_regex_rules_supports_js_literal_pattern() {
        // ST regex_scripts may store `findRegex` as a JS literal `/pattern/flags`
        // instead of a raw pattern; both forms must work identically.
        let mut r = def("regex_rule", "r", "");
        r.meta = json!({ "pattern": "/<update>(.*)<\\/update>/gsi", "replacement": "[$1]" });
        assert_eq!(
            apply_regex_rules("a<update>hp 10</update>b", &[r]).as_deref(),
            Some("a[hp 10]b")
        );
    }

    #[test]
    fn apply_regex_rules_js_literal_case_insensitive_flag() {
        let mut r = def("regex_rule", "r", "");
        r.meta = json!({ "pattern": "/HELLO/i", "replacement": "hi" });
        assert_eq!(apply_regex_rules("hello world", &[r]).as_deref(), Some("hi world"));
    }

    #[test]
    fn apply_for_filters_by_phase_and_target() {
        let mut ai_disp = def("regex_rule", "ai_disp", "");
        ai_disp.meta = json!({ "pattern": "X", "replacement": "", "scope": "display", "targets": ["ai_output"] });
        let mut user_prompt = def("regex_rule", "user_prompt", "");
        user_prompt.meta = json!({ "pattern": "Y", "replacement": "", "scope": "prompt", "targets": ["user_input"] });
        let rules = vec![ai_disp, user_prompt];

        assert_eq!(apply_regex_rules_for("XY", &rules, RegexTarget::AiOutput, RegexPhase::Display).as_deref(), Some("Y"));
        assert_eq!(apply_regex_rules_for("XY", &rules, RegexTarget::UserInput, RegexPhase::Prompt).as_deref(), Some("X"));
        assert_eq!(apply_regex_rules_for("XY", &rules, RegexTarget::UserInput, RegexPhase::Display), None);
    }

    #[test]
    fn apply_for_both_scope_covers_display_and_prompt() {
        let mut r = def("regex_rule", "r", "");
        r.meta = json!({ "pattern": "Z", "replacement": "", "scope": "both" }); // empty targets = broad
        assert_eq!(apply_regex_rules_for("Z", &[r.clone()], RegexTarget::AiOutput, RegexPhase::Display).as_deref(), Some(""));
        assert_eq!(apply_regex_rules_for("Z", &[r], RegexTarget::UserInput, RegexPhase::Prompt).as_deref(), Some(""));
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

    #[test]
    fn empty_active_child_does_not_emit_empty_tag() {
        let empty = def("char", "Anchor", ""); // identity anchor, empty content
        let body = def("char", "Bio", "real body");
        let f = folder_node("t", 0, "char");
        let r1 = child_ref("t", &f.id, 0, &empty.id);
        let r2 = child_ref("t", &f.id, 1, &body.id);
        let mut defs = std::collections::HashMap::new();
        defs.insert(empty.id.clone(), empty);
        defs.insert(body.id.clone(), body);
        let plan = assemble_from_nodes(&[f, r1, r2], &defs, &json!({}), &json!({}), &[], &mut || 0.0);
        assert_eq!(plan.segments.len(), 1);
        assert_eq!(plan.segments[0].content, "<char>\nreal body\n</char>");
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

    #[test]
    fn html_and_css_are_non_rendering() {
        assert!(is_non_rendering("html"));
        assert!(is_non_rendering("css"));
        assert!(is_non_rendering("regex_rule"));
        assert!(is_non_rendering("variables"));
        assert!(!is_non_rendering("prompt"));
    }
}
