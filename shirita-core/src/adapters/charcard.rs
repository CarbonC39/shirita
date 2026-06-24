//! SillyTavern Character Card v2/v3 -> Shirita loreset (Template + Definitions + Nodes).
//! One-way lossy translation: every non-empty ST field becomes its own
//! definition + ref node, placed in the loreset's 2-level template tree.

use crate::adapters::worldinfo::worldinfo_to_defs;
use crate::models::definition::Definition;
use crate::models::pack::Pack;
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::template::Template;
use crate::state::{VarDecl, VarType};

/// The full result of translating one ST card: a template, the definitions it
/// references, and the 2-level node tree wiring them together.
pub struct LoreSet {
    pub template: Template,
    pub definitions: Vec<Definition>,
    pub nodes: Vec<PromptNode>,
}

/// Return the field as a non-empty &str, or None.
fn nonempty<'a>(data: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    data.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

/// Map one ST `regex_scripts[i]` entry to a `regex_rule` definition. Stores the
/// richer ST switches in meta (scope/targets/depth); only display-side
/// application is wired in this slice (see assembly::apply_regex_rules).
fn regex_rule_def(s: &serde_json::Value) -> Definition {
    let name = s.get("scriptName").and_then(|v| v.as_str()).unwrap_or("regex").to_string();
    let mut d = Definition::new("regex_rule", name, "");
    let scope = match (
        s.get("markdownOnly").and_then(|v| v.as_bool()).unwrap_or(false),
        s.get("promptOnly").and_then(|v| v.as_bool()).unwrap_or(false),
    ) {
        (true, false) => "display",
        (false, true) => "prompt",
        _ => "both",
    };
    let targets: Vec<&str> = s
        .get("placement")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_u64())
                .map(|n| if n == 1 { "user_input" } else { "ai_output" })
                .collect()
        })
        .unwrap_or_default();
    d.meta = serde_json::json!({
        "pattern": s.get("findRegex").and_then(|v| v.as_str()).unwrap_or(""),
        "replacement": s.get("replaceString").and_then(|v| v.as_str()).unwrap_or(""),
        "disabled": s.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false),
        "scope": scope,
        "targets": targets,
        "min_depth": s.get("minDepth"),
        "max_depth": s.get("maxDepth"),
        "st_raw": s.clone(),
    });
    d
}

/// ST's TavernHelper extension stores a card's default chat variables as a
/// flat `{name: value}` object (`extensions.tavern_helper.variables`). Map
/// each entry to a `VarDecl`, inferring type from the JSON value; nested
/// objects have no equivalent in our schema (number/bool/string/list) and
/// are dropped (consistent with this being a one-way lossy translation).
fn tavern_helper_vardecls(data: &serde_json::Value) -> Vec<VarDecl> {
    let Some(vars) = data
        .get("extensions")
        .and_then(|e| e.get("tavern_helper"))
        .and_then(|t| t.get("variables"))
        .and_then(|v| v.as_object())
    else {
        return Vec::new();
    };
    vars.iter()
        .filter_map(|(name, value)| {
            let var_type = match value {
                serde_json::Value::Number(_) => VarType::Number,
                serde_json::Value::Bool(_) => VarType::Bool,
                serde_json::Value::String(_) => VarType::String,
                serde_json::Value::Array(_) => VarType::List,
                serde_json::Value::Null | serde_json::Value::Object(_) => return None,
            };
            Some(VarDecl { name: name.clone(), var_type, initial: value.clone(), scope: None })
        })
        .collect()
}

/// Capture-group numbers (1-based) referenced by valid `$N` tokens in
/// `replace_string`, deduped and sorted ascending. A `$N` only counts when
/// `1 <= N <= group_count` — an out-of-range `$N` (e.g. `$10` against a
/// 3-group pattern) is not a capture reference at all and must be ignored
/// here (see `substitute_dollar_refs`, which leaves it untouched in the
/// output).
fn dollar_refs_in(replace_string: &str, group_count: usize) -> Vec<usize> {
    static DOLLAR_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\$(\d+)").unwrap());
    let mut ns: Vec<usize> = DOLLAR_RE
        .captures_iter(replace_string)
        .filter_map(|c| c[1].parse::<usize>().ok())
        .filter(|&n| n >= 1 && n <= group_count)
        .collect();
    ns.sort_unstable();
    ns.dedup();
    ns
}

/// Replace every `$N` in `replace_string` where `N` is in `valid_ns` with
/// Panel's `{{fieldN}}` interpolation syntax; any other `$N` (out of range,
/// already excluded from `valid_ns` by `dollar_refs_in`) is left exactly
/// as-is.
fn substitute_dollar_refs(replace_string: &str, valid_ns: &[usize]) -> String {
    static DOLLAR_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\$(\d+)").unwrap());
    DOLLAR_RE
        .replace_all(replace_string, |caps: &regex::Captures| {
            // A digit run too large for usize can never be a valid group ref
            // (valid_ns only holds 1..=group_count); fall back to 0 to fail
            // the contains() check below rather than panicking.
            let n: usize = caps[1].parse().unwrap_or(0);
            if valid_ns.contains(&n) {
                let mut s = String::from("{{field");
                s.push_str(&n.to_string());
                s.push_str("}}");
                s
            } else {
                caps[0].to_string()
            }
        })
        .into_owned()
}

/// Remove every `<tag ...>...</tag>` block from `html` (case-insensitive,
/// content may span multiple lines), returning the html with those blocks
/// cut out plus each block's inner content in order of appearance. Used to
/// pull a card's `<style>` into `panel.css` and discard its `<script>`
/// (Panel forbids `<script>` at render time regardless — this just does it
/// explicitly, earlier).
fn extract_tag_blocks(html: &str, tag: &str) -> (String, Vec<String>) {
    let pattern = format!(r"(?is)<{tag}\b[^>]*>(.*?)</{tag}\s*>");
    let re = regex::Regex::new(&pattern).expect("tag is always a static literal (\"style\"/\"script\")");
    let mut blocks = Vec::new();
    let mut out = String::new();
    let mut last = 0;
    for caps in re.captures_iter(html) {
        let m = caps.get(0).unwrap();
        out.push_str(&html[last..m.start()]);
        blocks.push(caps.get(1).map(|g| g.as_str().to_string()).unwrap_or_default());
        last = m.end();
    }
    out.push_str(&html[last..]);
    (out, blocks)
}

/// Result of recognizing exactly one ST `regex_scripts` entry as the common
/// single-regex/`$N`-template status-bar pattern and converting it into
/// native Panel content. See the 2026-06-22 design spec, §3.
struct PanelConversion {
    source_index: usize,
    html: String,
    css: String,
    var_decls: Vec<VarDecl>,
    capture_vars: Vec<Option<String>>,
}

/// True if `html` cannot be represented as Panel content: either it's a
/// full standalone document (contains an `<html`, `<head`, or `<body`
/// opening tag — a `$N`-template status bar is, by definition, a fragment
/// meant to be spliced into an existing page, not a complete document), or
/// it relies on inline event-handler attributes (`onclick=...` etc. —
/// `sanitizePanelHtml` strips every `on*=` attribute, so a template that
/// needs one for its core interactivity cannot function as a Panel and
/// must stay on the compatibility layer where its handlers actually run).
fn is_unrepresentable_as_panel(html: &str) -> bool {
    static DOC_TAG_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)<(?:html|head|body)\b").unwrap());
    static INLINE_HANDLER_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)\bon[a-z]+\s*=").unwrap());
    DOC_TAG_RE.is_match(html) || INLINE_HANDLER_RE.is_match(html)
}

/// Detect and convert the card's status-bar `regex_scripts` entry, if there
/// is exactly one unambiguous candidate. Returns `None` when zero or
/// multiple scripts qualify — ambiguous detections are skipped rather than
/// guessed.
fn try_convert_status_panel(scripts: &[serde_json::Value]) -> Option<PanelConversion> {
    struct Candidate {
        index: usize,
        replace_string: String,
        valid_ns: Vec<usize>,
    }

    let mut candidates = Vec::new();
    for (index, s) in scripts.iter().enumerate() {
        if s.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        // Same scope derivation as `regex_rule_def`: (markdownOnly, promptOnly)
        // -> "display" | "prompt" | "both". Conversion only applies to scripts
        // that affect display ("display" or "both").
        let markdown_only = s.get("markdownOnly").and_then(|v| v.as_bool()).unwrap_or(false);
        let prompt_only = s.get("promptOnly").and_then(|v| v.as_bool()).unwrap_or(false);
        let display_scope = !(prompt_only && !markdown_only);
        if !display_scope {
            continue;
        }
        let Some(find_regex) = s.get("findRegex").and_then(|v| v.as_str()) else { continue };
        let Some(replace_string) = s.get("replaceString").and_then(|v| v.as_str()) else { continue };
        let normalized = crate::assembly::normalize_js_regex_literal(find_regex);
        let Ok(re) = fancy_regex::Regex::new(&normalized) else { continue };
        let group_count = re.captures_len().saturating_sub(1);
        if group_count == 0 {
            continue;
        }
        let valid_ns = dollar_refs_in(replace_string, group_count);
        if valid_ns.is_empty() {
            continue;
        }
        candidates.push(Candidate { index, replace_string: replace_string.to_string(), valid_ns });
    }

    if candidates.len() != 1 {
        return None;
    }
    let c = candidates.into_iter().next().unwrap();

    let substituted = substitute_dollar_refs(&c.replace_string, &c.valid_ns);
    if is_unrepresentable_as_panel(&substituted) {
        return None;
    }
    let (no_style, style_blocks) = extract_tag_blocks(&substituted, "style");
    let (html, _script_blocks) = extract_tag_blocks(&no_style, "script"); // script content is dropped, never preserved

    let max_n = *c.valid_ns.iter().max().unwrap();
    let mut capture_vars: Vec<Option<String>> = vec![None; max_n];
    let mut var_decls = Vec::new();
    for &n in &c.valid_ns {
        let name = format!("field{n}");
        capture_vars[n - 1] = Some(name.clone());
        var_decls.push(VarDecl {
            name,
            var_type: VarType::String,
            initial: serde_json::Value::String(String::new()),
            scope: None,
        });
    }

    Some(PanelConversion { source_index: c.index, html, css: style_blocks.join("\n"), var_decls, capture_vars })
}

/// Translate an ST character card (v1 top-level / v2/v3 under `data`) into a loreset.
pub fn charcard_to_loreset(card: &serde_json::Value) -> LoreSet {
    let data = card.get("data").unwrap_or(card);
    let name = data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Imported character")
        .to_string();

    let tmpl = Template::new(name.clone());
    // OwnerKind is not Copy and the node constructors take it by value, so we
    // pass OwnerKind::Template directly at each call (a zero-cost unit variant).
    let mut defs: Vec<Definition> = Vec::new();
    let mut nodes: Vec<PromptNode> = Vec::new();
    let mut sort: i64 = 0;
    let next = |s: &mut i64| -> i64 {
        let v = *s;
        *s += 1;
        v
    };

    // --- before-history: system_prompt first ---
    if let Some(sp) = nonempty(data, "system_prompt") {
        let d = Definition::new("prompt", format!("{name}·system"), sp);
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }

    // --- char folder: description + personality ---
    let charf = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, next(&mut sort), "char");
    let mut child_sort: i64 = 0;
    let desc = Definition::new(
        "char",
        name.clone(),
        data.get("description").and_then(|v| v.as_str()).unwrap_or(""),
    );
    nodes.push(PromptNode::new_ref(
        OwnerKind::Template,
        &tmpl.id,
        Some(charf.id.clone()),
        next(&mut child_sort),
        &desc.id,
    ));
    defs.push(desc);
    if let Some(p) = nonempty(data, "personality") {
        let d = Definition::new("char", format!("{name}·personality"), p);
        nodes.push(PromptNode::new_ref(
            OwnerKind::Template,
            &tmpl.id,
            Some(charf.id.clone()),
            next(&mut child_sort),
            &d.id,
        ));
        defs.push(d);
    }
    nodes.push(charf);

    // --- world folder: scenario (constant) + character_book ---
    let worldf =
        PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, next(&mut sort), "world");
    let mut wsort: i64 = 0;
    if let Some(sc) = nonempty(data, "scenario") {
        let mut d = Definition::new("world", format!("{name}·scenario"), sc);
        d.meta = serde_json::json!({
            "trigger": { "mode": "constant", "keys": [], "probability": 100, "order": 100 }
        });
        nodes.push(PromptNode::new_ref(
            OwnerKind::Template,
            &tmpl.id,
            Some(worldf.id.clone()),
            next(&mut wsort),
            &d.id,
        ));
        defs.push(d);
    }
    if let Some(book) = data.get("character_book") {
        for bd in worldinfo_to_defs(book) {
            nodes.push(PromptNode::new_ref(
                OwnerKind::Template,
                &tmpl.id,
                Some(worldf.id.clone()),
                next(&mut wsort),
                &bd.id,
            ));
            defs.push(bd);
        }
    }
    nodes.push(worldf);

    // --- before-history: mes_example ---
    if let Some(ex) = nonempty(data, "mes_example") {
        let d = Definition::new("prompt", format!("{name}·examples"), ex);
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }

    // --- non-rendering root refs: regex_scripts + first_message ---
    let empty_scripts: Vec<serde_json::Value> = Vec::new();
    let scripts: &[serde_json::Value] = data
        .get("extensions")
        .and_then(|e| e.get("regex_scripts"))
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_scripts);
    // Detect the common single-regex/$N-template status-bar pattern (see the
    // 2026-06-22 design spec). Ambiguous or absent -> None, and nothing about
    // the regex_rule Definitions below changes.
    let panel_conversion = try_convert_status_panel(scripts);
    let mut panel_regex_def_id: Option<String> = None;
    for (index, s) in scripts.iter().enumerate() {
        let mut d = regex_rule_def(s);
        let is_panel_rule = panel_conversion.as_ref().is_some_and(|c| c.source_index == index);
        if is_panel_rule {
            if let Some(conv) = &panel_conversion {
                if let Some(obj) = d.meta.as_object_mut() {
                    obj.insert(
                        "capture_vars".to_string(),
                        serde_json::to_value(&conv.capture_vars).unwrap(),
                    );
                }
            }
            panel_regex_def_id = Some(d.id.clone());
            defs.push(d); // def only; its ref is added under the panel folder below
        } else {
            nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
            defs.push(d);
        }
    }
    let first = nonempty(data, "first_mes");
    let alts: Vec<String> = data
        .get("alternate_greetings")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if first.is_some() || !alts.is_empty() {
        let mut d = Definition::new("first_message", format!("{name}·greeting"), first.unwrap_or(""));
        d.meta = serde_json::json!({ "alternate_greetings": alts });
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }

    // --- history node ---
    let mut hist =
        PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, next(&mut sort), "history");
    hist.kind = NodeKind::History;
    hist.tag = None;
    nodes.push(hist);

    // --- after-history: post_history_instructions ---
    if let Some(ph) = nonempty(data, "post_history_instructions") {
        let d = Definition::new("prompt", format!("{name}·post"), ph);
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }

    // --- preserve un-interpreted extensions on the main char def (lossless) ---
    if let Some(ext) = data.get("extensions") {
        if let Some(ch) = defs.iter_mut().find(|d| d.def_type == "char" && d.name == name) {
            ch.meta = serde_json::json!({ "st_raw": ext.clone() });
        }
    }

    // --- card-level (tavern_helper) variables → a root `variables` brick ---
    let card_vars = tavern_helper_vardecls(data);
    if !card_vars.is_empty() {
        let mut vdef = Definition::new("variables", format!("{name}·vars"), "");
        vdef.meta = serde_json::json!({ "decls": card_vars });
        nodes.push(PromptNode::new_ref(
            OwnerKind::Template, &tmpl.id, None, next(&mut sort), &vdef.id));
        defs.push(vdef);
    }

    // --- panel folder (html/css bricks + the panel-sync regex ref) ---
    if let Some(conv) = &panel_conversion {
        let mut folder =
            PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, next(&mut sort), "panel");
        folder.meta = serde_json::json!({ "name": format!("{name}·panel"), "caps": {} });
        let mut csort: i64 = 0;

        let html = Definition::new("html", format!("{name}·panel·html"), conv.html.clone());
        nodes.push(PromptNode::new_ref(
            OwnerKind::Template, &tmpl.id, Some(folder.id.clone()), next(&mut csort), &html.id));
        defs.push(html);

        let css = Definition::new("css", format!("{name}·panel·css"), conv.css.clone());
        nodes.push(PromptNode::new_ref(
            OwnerKind::Template, &tmpl.id, Some(folder.id.clone()), next(&mut csort), &css.id));
        defs.push(css);

        if !conv.var_decls.is_empty() {
            let mut vdef = Definition::new("variables", format!("{name}·panel·vars"), "");
            vdef.meta = serde_json::json!({ "decls": conv.var_decls.clone() });
            nodes.push(PromptNode::new_ref(
                OwnerKind::Template, &tmpl.id, Some(folder.id.clone()), next(&mut csort), &vdef.id));
            defs.push(vdef);
        }

        if let Some(rid) = &panel_regex_def_id {
            nodes.push(PromptNode::new_ref(
                OwnerKind::Template, &tmpl.id, Some(folder.id.clone()), next(&mut csort), rid));
        }
        nodes.push(folder);
    }

    LoreSet { template: tmpl, definitions: defs, nodes }
}

/// Re-home a [`LoreSet`] under a fresh [`Pack`] instead of a [`Template`].
///
/// A Pack is the format actually designed to hold one self-contained piece of
/// imported character content: a node tree owned directly by the pack (no
/// separate Template row) plus an optional bound identity. This takes the
/// `Template`-owned tree `charcard_to_loreset` builds and rewrites every
/// node's `owner_kind`/`owner_id` to point at a new pack, carries the
/// template's `meta` onto `pack.meta`, and sets `pack.identity` from the
/// card's name + (optional) saved avatar filename. Imported variables now
/// live in `variables` definition bricks (returned unchanged in
/// `definitions`), not in template/pack meta.
///
/// Definitions are returned unchanged — they have no owner field, so they are
/// reused as-is regardless of which tree they're attached to.
pub fn loreset_to_pack(ls: LoreSet, avatar: Option<&str>) -> (Pack, Vec<Definition>, Vec<PromptNode>) {
    let LoreSet { template, definitions, nodes } = ls;
    let mut pack = Pack::new(template.name.clone());
    pack.identity.display_name = Some(template.name);
    pack.identity.avatar = avatar.map(String::from);
    pack.meta = template.meta;
    let nodes = nodes
        .into_iter()
        // Packs hold no History/Content nodes (assembly::assemble_from_nodes_with_packs
        // expects exactly one of each, owned by the mounting Template — which is also
        // where the chat-history mount point itself lives). charcard_to_loreset always
        // appends a History root since its `LoreSet` shape was originally Template-only;
        // dropping it here keeps that single source of truth instead of every imported
        // pack carrying a second, inert chatHistory node alongside the template's.
        .filter(|n| !matches!(n.kind, NodeKind::History | NodeKind::Content))
        .map(|mut n| {
            n.owner_kind = OwnerKind::Pack;
            n.owner_id = pack.id.clone();
            n
        })
        .collect();
    (pack, definitions, nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty<'a>(s: &'a LoreSet, t: &str) -> Vec<&'a Definition> {
        s.definitions.iter().filter(|d| d.def_type == t).collect()
    }

    #[test]
    fn dollar_refs_in_keeps_only_in_range_groups() {
        // pattern has 3 capture groups; $10 is not a reference (out of range),
        // $1 and $3 are.
        assert_eq!(dollar_refs_in("$1 $3 $10", 3), vec![1, 3]);
    }

    #[test]
    fn dollar_refs_in_dedupes_and_sorts() {
        assert_eq!(dollar_refs_in("$3 $1 $3", 3), vec![1, 3]);
    }

    #[test]
    fn dollar_refs_in_ignores_dollar_dollar_and_dollar_amp() {
        // `$$` and `$&` never match `\$(\d+)` — no digits follow the `$`.
        assert_eq!(dollar_refs_in("$$ $& $1", 1), vec![1]);
    }

    #[test]
    fn substitute_dollar_refs_replaces_only_valid_refs() {
        let out = substitute_dollar_refs("a:$1 b:$10 c:$3", &[1, 3]);
        assert_eq!(out, "a:{{field1}} b:$10 c:{{field3}}");
    }

    #[test]
    fn substitute_dollar_refs_does_not_panic_on_overflowing_digit_run() {
        // A digit run too large for usize must be left exactly as-is, not panic.
        let out = substitute_dollar_refs("$1 $99999999999999999999999999", &[1]);
        assert_eq!(out, "{{field1}} $99999999999999999999999999");
    }

    #[test]
    fn extract_tag_blocks_removes_style_and_returns_contents() {
        let html = "<div>x</div><style>.a{color:red}</style><p>y</p>";
        let (remaining, blocks) = extract_tag_blocks(html, "style");
        assert_eq!(remaining, "<div>x</div><p>y</p>");
        assert_eq!(blocks, vec![".a{color:red}".to_string()]);
    }

    #[test]
    fn extract_tag_blocks_is_case_insensitive_and_dotall() {
        let html = "<Style>\n.a{color:red}\n</STYLE>";
        let (remaining, blocks) = extract_tag_blocks(html, "style");
        assert_eq!(remaining, "");
        assert_eq!(blocks, vec!["\n.a{color:red}\n".to_string()]);
    }

    #[test]
    fn extract_tag_blocks_handles_multiple_blocks() {
        let html = "<script>a()</script>mid<script>b()</script>";
        let (remaining, blocks) = extract_tag_blocks(html, "script");
        assert_eq!(remaining, "mid");
        assert_eq!(blocks, vec!["a()".to_string(), "b()".to_string()]);
    }

    #[test]
    fn extract_tag_blocks_no_match_returns_input_unchanged() {
        let html = "<div>no blocks here</div>";
        let (remaining, blocks) = extract_tag_blocks(html, "script");
        assert_eq!(remaining, html);
        assert!(blocks.is_empty());
    }

    fn script(find_regex: &str, replace_string: &str) -> serde_json::Value {
        serde_json::json!({ "findRegex": find_regex, "replaceString": replace_string, "disabled": false })
    }

    #[test]
    fn try_convert_status_panel_converts_the_only_candidate() {
        let scripts = vec![script(
            r"<update>(.*?)<hp>(\d+)</hp></update>",
            "<div>$1</div><div>HP: $2</div><style>.x{color:red}</style><script>alert(1)</script>",
        )];
        let conv = try_convert_status_panel(&scripts).expect("exactly one candidate must convert");
        assert_eq!(conv.source_index, 0);
        assert_eq!(conv.html, "<div>{{field1}}</div><div>HP: {{field2}}</div>");
        assert_eq!(conv.css, ".x{color:red}");
        assert_eq!(conv.capture_vars, vec![Some("field1".to_string()), Some("field2".to_string())]);
        let names: Vec<&str> = conv.var_decls.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["field1", "field2"]);
        assert!(conv.var_decls.iter().all(|v| v.var_type == VarType::String));
    }

    #[test]
    fn try_convert_status_panel_skips_when_no_candidates() {
        // no `$N` anywhere in replaceString -> not a status-bar template.
        let scripts = vec![script(r"<a>(.*)</a>", "plain text, no placeholders")];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_skips_when_ambiguous() {
        // two scripts both qualify -> skip rather than guess.
        let scripts = vec![script(r"<a>(.*)</a>", "$1"), script(r"<b>(.*)</b>", "$1")];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_skips_disabled_scripts() {
        let mut s = script(r"<a>(.*)</a>", "$1");
        s["disabled"] = serde_json::json!(true);
        assert!(try_convert_status_panel(&[s]).is_none());
    }

    #[test]
    fn try_convert_status_panel_skips_prompt_only_scripts() {
        // promptOnly (no markdownOnly) -> scope is "prompt", never applies to display.
        let mut s = script(r"<a>(.*)</a>", "$1");
        s["promptOnly"] = serde_json::json!(true);
        assert!(try_convert_status_panel(&[s]).is_none());
    }

    #[test]
    fn try_convert_status_panel_handles_js_literal_find_regex() {
        // `/pattern/flags` form must compile via normalize_js_regex_literal,
        // same as the display-time regex_rule pipeline.
        let scripts = vec![script(r"/<hp>(\d+)<\/hp>/gsi", "hp=$1")];
        let conv = try_convert_status_panel(&scripts).expect("js-literal pattern must still convert");
        assert_eq!(conv.html, "hp={{field1}}");
    }

    #[test]
    fn try_convert_status_panel_out_of_range_dollar_is_not_a_candidate_signal() {
        // 1 capture group, but replaceString only references $10 (out of range) ->
        // not a valid candidate (no in-range $N at all).
        let scripts = vec![script(r"<a>(.*)</a>", "$10 literally")];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_ignores_uncompilable_findregex() {
        // An unbalanced paren never compiles -> excluded from candidates
        // entirely (not an error, not a panic).
        let scripts = vec![script(r"<a>(.*", "$1")];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_repeated_dollar_n_yields_one_variable() {
        let scripts = vec![script(r"<hp>(\d+)</hp>", "now: $1, again: $1")];
        let conv = try_convert_status_panel(&scripts).unwrap();
        assert_eq!(conv.html, "now: {{field1}}, again: {{field1}}");
        assert_eq!(conv.var_decls.len(), 1);
        assert_eq!(conv.var_decls[0].name, "field1");
    }

    #[test]
    fn try_convert_status_panel_skips_full_html_document() {
        // A candidate whose replaceString is a complete standalone document
        // (not a fragment) cannot become Panel content — Panel forbids
        // <html>/<head>/<body> document-wrapper structure.
        let scripts = vec![script(
            r"<hp>(\d+)</hp>",
            "<!DOCTYPE html><html><body><div>HP: $1</div></body></html>",
        )];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_skips_inline_event_handlers() {
        // A fragment (no <html> wrapper) that still relies on inline
        // on*= handlers for its core interactivity cannot work as a Panel
        // (sanitizePanelHtml strips all on*= attributes).
        let scripts = vec![script(
            r"<hp>(\d+)</hp>",
            r#"<div onclick="doStuff()">HP: $1</div>"#,
        )];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_accepts_plain_fragment_without_handlers() {
        // Sanity check: the new rejection check must not affect a normal,
        // already-passing fragment candidate (no <html>, no on*=).
        let scripts = vec![script(r"<hp>(\d+)</hp>", "<div class=\"bar\">HP: $1</div>")];
        let conv = try_convert_status_panel(&scripts).expect("plain fragment must still convert");
        assert_eq!(conv.html, "<div class=\"bar\">HP: {{field1}}</div>");
    }

    #[test]
    fn decomposes_every_nonempty_field() {
        let card = serde_json::json!({
            "spec": "chara_card_v3", "spec_version": "3.0",
            "data": {
                "name": "Neo", "description": "desc",
                "personality": "calm", "scenario": "the matrix",
                "mes_example": "<START>ex", "system_prompt": "be terse",
                "post_history_instructions": "stay terse",
                "first_mes": "wake up", "alternate_greetings": ["again", "third"],
                "character_book": { "entries": [ { "keys": ["zion"], "comment": "Zion", "content": "Last city" } ] },
                "extensions": { "regex_scripts": [
                    { "scriptName": "r1", "findRegex": "a", "replaceString": "b", "disabled": false, "markdownOnly": true }
                ] }
            }
        });
        let s = charcard_to_loreset(&card);
        // every non-empty field becomes a definition
        assert_eq!(ty(&s, "char").len(), 2); // description + personality
        assert_eq!(ty(&s, "world").len(), 2); // scenario(constant) + 1 book entry
        assert_eq!(ty(&s, "prompt").len(), 3); // system_prompt + mes_example + post_history
        assert_eq!(ty(&s, "first_message").len(), 1);
        assert_eq!(ty(&s, "regex_rule").len(), 1);
        // first_message carries the alternates
        let fm = ty(&s, "first_message")[0];
        assert_eq!(fm.content, "wake up");
        assert_eq!(fm.meta["alternate_greetings"][1], "third");
        // scenario world is constant
        let worlds = ty(&s, "world");
        let scen = worlds.iter().find(|d| d.content == "the matrix").unwrap();
        assert_eq!(scen.meta["trigger"]["mode"], "constant");
        // 2-level: every ref's parent is None or points to a folder
        let folder_ids: std::collections::HashSet<_> = s
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Folder)
            .map(|n| n.id.clone())
            .collect();
        for n in s.nodes.iter().filter(|n| n.kind == NodeKind::Ref) {
            assert!(n.parent_id.is_none() || folder_ids.contains(n.parent_id.as_ref().unwrap()));
        }
        // system_prompt is before history, post_history is after (by sort_order)
        let hist = s.nodes.iter().find(|n| n.kind == NodeKind::History).unwrap();
        let sys_ref = s
            .nodes
            .iter()
            .find(|n| {
                n.parent_id.is_none()
                    && n.kind == NodeKind::Ref
                    && s.definitions.iter().any(|d| {
                        Some(&d.id) == n.definition_id.as_ref() && d.content == "be terse"
                    })
            })
            .unwrap();
        let post_ref = s
            .nodes
            .iter()
            .find(|n| {
                n.parent_id.is_none()
                    && n.kind == NodeKind::Ref
                    && s.definitions.iter().any(|d| {
                        Some(&d.id) == n.definition_id.as_ref() && d.content == "stay terse"
                    })
            })
            .unwrap();
        assert!(sys_ref.sort_order < hist.sort_order);
        assert!(post_ref.sort_order > hist.sort_order);
    }

    #[test]
    fn imports_tavern_helper_variables_into_a_root_variables_brick() {
        let card = serde_json::json!({
            "data": {
                "name": "Neo", "description": "desc",
                "extensions": { "tavern_helper": { "variables": { "hp": 100, "is_alive": true, "name": "Neo", "items": ["a", "b"] } } }
            }
        });
        let s = charcard_to_loreset(&card);
        let vbrick = s
            .definitions
            .iter()
            .find(|d| d.def_type == "variables" && d.name.ends_with("·vars"))
            .expect("a root variables brick");
        let vars = vbrick.meta["decls"].as_array().unwrap();
        let find = |n: &str| vars.iter().find(|v| v["name"] == n).unwrap();
        assert_eq!(find("hp")["type"], "number");
        assert_eq!(find("is_alive")["type"], "bool");
        assert_eq!(find("name")["type"], "string");
        assert_eq!(find("items")["type"], "list");
    }

    #[test]
    fn loreset_to_pack_rehomes_every_node_under_the_new_pack() {
        let card = serde_json::json!({
            "data": {
                "name": "Neo", "description": "desc", "personality": "calm",
                "scenario": "the matrix", "first_mes": "wake up",
            }
        });
        let ls = charcard_to_loreset(&card);
        let n_defs = ls.definitions.len();
        let n_history = ls.nodes.iter().filter(|n| n.kind == NodeKind::History).count();
        assert_eq!(n_history, 1, "charcard_to_loreset still appends its Template-shaped history node");
        let n_non_history = ls.nodes.len() - n_history;
        let (pack, defs, nodes) = loreset_to_pack(ls, Some("neo.png"));

        assert_eq!(pack.name, "Neo");
        assert_eq!(pack.identity.display_name.as_deref(), Some("Neo"));
        assert_eq!(pack.identity.avatar.as_deref(), Some("neo.png"));
        assert_eq!(defs.len(), n_defs);
        assert_eq!(nodes.len(), n_non_history, "the History node is dropped — packs hold no history/content nodes");
        assert!(
            !nodes.iter().any(|n| matches!(n.kind, NodeKind::History | NodeKind::Content)),
            "a pack must never carry its own history/content mount — that's the owning template's job"
        );
        // every node now belongs to the pack, not a template.
        for n in &nodes {
            assert_eq!(n.owner_kind, OwnerKind::Pack);
            assert_eq!(n.owner_id, pack.id);
        }
        // refs still resolve into the returned defs (nothing dangling).
        let def_ids: std::collections::HashSet<_> = defs.iter().map(|d| d.id.clone()).collect();
        for n in nodes.iter().filter(|n| n.kind == NodeKind::Ref) {
            assert!(def_ids.contains(n.definition_id.as_ref().unwrap()));
        }
    }

    #[test]
    fn loreset_to_pack_carries_variables_brick_and_no_avatar_when_absent() {
        let card = serde_json::json!({
            "data": {
                "name": "Neo", "description": "desc",
                "extensions": { "tavern_helper": { "variables": { "hp": 100 } } }
            }
        });
        let ls = charcard_to_loreset(&card);
        let vbrick = ls
            .definitions
            .iter()
            .find(|d| d.def_type == "variables" && d.name.ends_with("·vars"))
            .expect("a root variables brick");
        assert!(!vbrick.meta["decls"].as_array().unwrap().is_empty());
        let (pack, defs, _) = loreset_to_pack(ls, None);
        assert!(defs.iter().any(|d| d.def_type == "variables"));
        assert_eq!(pack.identity.avatar, None);
    }

    #[test]
    fn empty_fields_produce_no_defs() {
        let card = serde_json::json!({ "data": { "name": "Bare", "description": "only desc" } });
        let s = charcard_to_loreset(&card);
        assert_eq!(s.definitions.len(), 1); // only the char(description)
        assert_eq!(s.definitions[0].def_type, "char");
    }

    #[test]
    fn charcard_to_loreset_emits_panel_folder_for_unambiguous_status_bar() {
        let card = serde_json::json!({
            "data": {
                "name": "Neo", "description": "desc",
                "extensions": { "regex_scripts": [
                    { "scriptName": "status", "findRegex": "<hp>(\\d+)</hp>",
                      "replaceString": "HP: $1", "disabled": false, "markdownOnly": true }
                ] }
            }
        });
        let ls = charcard_to_loreset(&card);

        // No panel blob on template meta anymore.
        assert!(ls.template.meta.get("panel").is_none());

        // A panel folder exists.
        let folder = ls.nodes.iter()
            .find(|n| n.kind == NodeKind::Folder && n.tag.as_deref() == Some("panel"))
            .expect("a panel folder must be emitted");

        // It has an html child whose def content is the converted markup.
        let html_ref = ls.nodes.iter()
            .find(|n| n.kind == NodeKind::Ref && n.parent_id.as_deref() == Some(folder.id.as_str())
                && ls.definitions.iter().any(|d| Some(d.id.as_str()) == n.definition_id.as_deref() && d.def_type == "html"))
            .expect("panel folder has an html child");
        let html_def = ls.definitions.iter()
            .find(|d| Some(d.id.as_str()) == html_ref.definition_id.as_deref()).unwrap();
        assert_eq!(html_def.content, "HP: {{field1}}");

        // It has a css child (may be empty content when the card had no <style>).
        assert!(ls.nodes.iter().any(|n| n.kind == NodeKind::Ref
            && n.parent_id.as_deref() == Some(folder.id.as_str())
            && ls.definitions.iter().any(|d| Some(d.id.as_str()) == n.definition_id.as_deref() && d.def_type == "css")));

        // The panel-sync regex rule lives INSIDE the folder and keeps its capture_vars.
        let rule_ref = ls.nodes.iter()
            .find(|n| n.kind == NodeKind::Ref && n.parent_id.as_deref() == Some(folder.id.as_str())
                && ls.definitions.iter().any(|d| Some(d.id.as_str()) == n.definition_id.as_deref() && d.def_type == "regex_rule"))
            .expect("panel-sync regex is parented under the folder");
        let rule = ls.definitions.iter()
            .find(|d| Some(d.id.as_str()) == rule_ref.definition_id.as_deref()).unwrap();
        assert_eq!(rule.meta["capture_vars"], serde_json::json!(["field1"]));

        // The status-bar capture fields become a `variables` brick INSIDE
        // the panel folder (not on template meta).
        let panel_vars = ls
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Ref && n.parent_id.as_deref() == Some(folder.id.as_str()))
            .filter_map(|n| n.definition_id.as_deref())
            .filter_map(|id| ls.definitions.iter().find(|d| d.id == id))
            .find(|d| d.def_type == "variables")
            .expect("panel folder has a variables child brick");
        assert!(panel_vars.meta["decls"].as_array().unwrap().iter().any(|d| d["name"] == "field1"));
    }

    #[test]
    fn charcard_to_loreset_omits_panel_when_no_status_bar_detected() {
        let card = serde_json::json!({
            "data": {
                "name": "Neo", "description": "desc",
                "extensions": { "regex_scripts": [
                    { "scriptName": "r1", "findRegex": "a", "replaceString": "b", "disabled": false }
                ] }
            }
        });
        let ls = charcard_to_loreset(&card);
        assert!(ls.template.meta.get("panel").is_none());
        let rule = ls.definitions.iter().find(|d| d.def_type == "regex_rule").unwrap();
        assert!(rule.meta.get("capture_vars").is_none());
    }

    #[test]
    fn charcard_to_loreset_omits_panel_for_full_document_status_bar() {
        // Reproduces the 密教模拟器.json shape: a single $N-template candidate
        // whose replaceString is a complete HTML document with onclick
        // handlers and id-based CSS — must stay on the compatibility layer
        // (regex_rule untouched, no panel written) rather than become a
        // broken Panel.
        let card = serde_json::json!({
            "data": {
                "name": "Cultist", "description": "desc",
                "extensions": { "regex_scripts": [
                    { "scriptName": "status", "findRegex": "<hp>(\\d+)</hp>",
                      "replaceString": "<!DOCTYPE html><html><head><style>#bar{color:red}</style></head><body><div id=\"bar\" onclick=\"tick()\">HP: $1</div></body></html>",
                      "disabled": false, "markdownOnly": true }
                ] }
            }
        });
        let ls = charcard_to_loreset(&card);
        assert!(ls.template.meta.get("panel").is_none());
        let rule = ls.definitions.iter().find(|d| d.def_type == "regex_rule").unwrap();
        assert!(rule.meta.get("capture_vars").is_none());
        // the compatibility-layer fields are untouched — this script still
        // works exactly as before via apply_regex_rules_for + HtmlCardFrame.
        assert_eq!(rule.meta["pattern"], "<hp>(\\d+)</hp>");
        assert!(rule.meta["replacement"].as_str().unwrap().contains("onclick=\"tick()\""));
    }

    #[test]
    fn charcard_to_loreset_merges_converted_vars_with_tavern_helper_vars_without_duplicates() {
        let card = serde_json::json!({
            "data": {
                "name": "Neo", "description": "desc",
                "extensions": {
                    "regex_scripts": [
                        { "scriptName": "status", "findRegex": "<hp>(\\d+)</hp>",
                          "replaceString": "HP: $1", "disabled": false, "markdownOnly": true }
                    ],
                    "tavern_helper": { "variables": { "field1": "already declared", "mood": "calm" } }
                }
            }
        });
        let ls = charcard_to_loreset(&card);
        // Card-level (tavern_helper) variables become a root `variables` brick,
        // not template meta.
        let vbrick = ls
            .definitions
            .iter()
            .find(|d| d.def_type == "variables" && d.name.ends_with("·vars"))
            .expect("a root variables brick");
        let decls = vbrick.meta["decls"].as_array().unwrap();
        let names: Vec<&str> = decls.iter().map(|d| d["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"field1") && names.contains(&"mood"));
        assert!(ls.template.meta.get("variables").is_none(), "no meta.variables god-object");
    }
}
