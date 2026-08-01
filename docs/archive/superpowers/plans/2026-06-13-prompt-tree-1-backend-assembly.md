# Prompt Tree v2 — Plan 1: Backend tree-driven assembly + world-book triggers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the prompt node tree actually drive prompt assembly — walk the tree, apply world-book triggers (constant/keyword/random with recursive scan), split at the history node, and emit a structured message array — replacing the M2 group-by-type assembly.

**Architecture:** A new `keyword` module (Aho-Corasick multi-pattern matcher) + trigger activation in `assembly.rs`; `assemble_from_nodes` returns an ordered `Vec<PromptSegment>` (not a joined string); `build_chat_messages` serializes segments + real history into the provider message array with consecutive-same-role merging. `conversation.rs` resolves the session's *effective* tree (its own nodes if forked, else the referenced template's) and runs this pipeline. Sessions **reference** their template (no eager deep-copy).

**Tech Stack:** Rust, `aho-corasick`, `rand`, `serde_json`, sqlx/SQLite, existing `Storage` trait + `temp_storage()` test base.

**Spec:** `docs/superpowers/specs/2026-06-13-prompt-tree-worldbook-design.md` (§4 node model, §5 triggers, §6 scan, §7 reference+CoW, §8 assembly).

**Out of scope (later plans):** extensible `def_types`/`DefinitionType→string` (Plan 2), frontend tree v2 (Plan 3), trigger editor UI (Plan 4), quick UI fixes (Plan 5), ST import/export (Plan 6).

---

## File structure

- `shirita-core/Cargo.toml` — add `aho-corasick`, `rand`.
- `shirita-core/migrations/0007_prompt_nodes_history.sql` — **new**: rebuild `prompt_nodes` to allow `kind='history'`.
- `shirita-core/src/models/prompt_node.rs` — add `NodeKind::History`.
- `shirita-core/src/keyword.rs` — **new**: `KeywordIndex` (Aho-Corasick) → matched ids.
- `shirita-core/src/assembly.rs` — add `Trigger`, `parse_trigger`, `activate`, `Placement`, `PromptSegment`, `AssembledPlan`, `assemble_from_nodes`, `build_chat_messages`, `effective_trigger`. Keep `render_vars`/`effective_content`/`apply_regex_rules`. Remove old `assemble_system_prompt`/`wrap_tag` once callers move.
- `shirita-core/src/lib.rs` — re-export new module/types.
- `shirita-core/src/conversation.rs` — rewire `send_message` to the new pipeline + effective-tree resolution.
- `shirita-web/src/routes/sessions.rs` — `create_session` references template (no `copy_nodes`).
- `shirita-web/src/routes/templates.rs` — auto-create a history node on template create.

> Migration is numbered `0007` (next free; `0006_settings.sql` is the last existing one).

---

## Task 1: Add the `history` node kind

**Files:**
- Create: `shirita-core/migrations/0007_prompt_nodes_history.sql`
- Modify: `shirita-core/src/models/prompt_node.rs`

- [ ] **Step 1: Write the failing test** — append to the `tests` module in `shirita-core/src/models/prompt_node.rs`:

```rust
    #[test]
    fn node_kind_history_roundtrip() {
        assert_eq!(NodeKind::History.as_str(), "history");
        assert_eq!(NodeKind::from_db("history").unwrap(), NodeKind::History);
    }
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cargo test -p shirita-core prompt_node::tests::node_kind_history_roundtrip`
Expected: FAIL (no `NodeKind::History` variant).

- [ ] **Step 3: Add the variant.** In `shirita-core/src/models/prompt_node.rs`:

```rust
pub enum NodeKind {
    Folder,
    Ref,
    History,
}
```
and in `as_str`:
```rust
            NodeKind::History => "history",
```
and in `from_db`:
```rust
            "history" => NodeKind::History,
```

- [ ] **Step 4: Write the migration** `shirita-core/migrations/0007_prompt_nodes_history.sql` (SQLite can't ALTER a CHECK; rebuild the table preserving data, FKs, indexes):

```sql
PRAGMA foreign_keys=OFF;

CREATE TABLE prompt_nodes_new (
    id            TEXT PRIMARY KEY,
    owner_kind    TEXT NOT NULL CHECK(owner_kind IN ('template', 'session')),
    owner_id      TEXT NOT NULL,
    parent_id     TEXT REFERENCES prompt_nodes_new(id) ON DELETE CASCADE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    kind          TEXT NOT NULL CHECK(kind IN ('folder', 'ref', 'history')),
    tag           TEXT,
    definition_id TEXT REFERENCES definitions(id) ON DELETE SET NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO prompt_nodes_new
    SELECT id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at
    FROM prompt_nodes;

DROP TABLE prompt_nodes;
ALTER TABLE prompt_nodes_new RENAME TO prompt_nodes;

CREATE INDEX IF NOT EXISTS idx_prompt_nodes_owner ON prompt_nodes(owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_prompt_nodes_parent ON prompt_nodes(parent_id);

PRAGMA foreign_keys=ON;
```

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test -p shirita-core prompt_node::`
Expected: PASS (all node model tests, including the new one).

- [ ] **Step 6: Commit**

```bash
git add shirita-core/migrations/0007_prompt_nodes_history.sql shirita-core/src/models/prompt_node.rs
git commit -m "feat(core): add history node kind + migration"
```

---

## Task 2: Aho-Corasick keyword matcher

A reusable index: build once from `(id, keys)` pairs, scan a text buffer once, return the set of ids whose any key matched. Lowercases patterns + text for case-insensitive (incl. CJK no-op) matching.

**Files:**
- Create: `shirita-core/src/keyword.rs`
- Modify: `shirita-core/Cargo.toml`, `shirita-core/src/lib.rs`

- [ ] **Step 1: Add the dependency.** In `shirita-core/Cargo.toml` under `[dependencies]`:

```toml
aho-corasick = "1"
rand = "0.8"
```

- [ ] **Step 2: Write the failing test.** Create `shirita-core/src/keyword.rs`:

```rust
//! 关键词多模式匹配（Aho-Corasick）：一次扫描得全部命中 id。

use std::collections::HashSet;

use aho_corasick::AhoCorasick;

/// 把若干 (id, keys) 编成一个自动机，对文本一次扫描即得命中 id 集合。
pub struct KeywordIndex {
    ac: Option<AhoCorasick>,
    /// pattern 序号 → owner id。
    owners: Vec<String>,
}

impl KeywordIndex {
    /// `entries`: (id, 该 id 的关键词列表)。空关键词会被跳过。
    pub fn build(entries: &[(String, Vec<String>)]) -> Self {
        let mut patterns: Vec<String> = Vec::new();
        let mut owners: Vec<String> = Vec::new();
        for (id, keys) in entries {
            for k in keys {
                let k = k.trim().to_lowercase();
                if k.is_empty() {
                    continue;
                }
                patterns.push(k);
                owners.push(id.clone());
            }
        }
        let ac = if patterns.is_empty() {
            None
        } else {
            Some(AhoCorasick::new(&patterns).expect("aho-corasick build"))
        };
        Self { ac, owners }
    }

    /// 对 `text` 一次扫描，返回命中（任一关键词）的 id 集合。
    pub fn scan(&self, text: &str) -> HashSet<String> {
        let mut hit = HashSet::new();
        if let Some(ac) = &self.ac {
            let lower = text.to_lowercase();
            for m in ac.find_iter(&lower) {
                hit.insert(self.owners[m.pattern().as_usize()].clone());
            }
        }
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, keys: &[&str]) -> (String, Vec<String>) {
        (id.to_string(), keys.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn matches_case_insensitive_substring() {
        let idx = KeywordIndex::build(&[entry("zion", &["Zion"]), entry("neo", &["Neo", "the one"])]);
        let hit = idx.scan("Tell me about ZION please");
        assert!(hit.contains("zion"));
        assert!(!hit.contains("neo"));
    }

    #[test]
    fn multiple_keys_any_match() {
        let idx = KeywordIndex::build(&[entry("neo", &["neo", "the one"])]);
        assert!(idx.scan("he is the one").contains("neo"));
    }

    #[test]
    fn empty_index_matches_nothing() {
        let idx = KeywordIndex::build(&[entry("x", &[])]);
        assert!(idx.scan("anything").is_empty());
    }

    #[test]
    fn cjk_keys_match() {
        let idx = KeywordIndex::build(&[entry("zion", &["锡安"])]);
        assert!(idx.scan("说说锡安城").contains("zion"));
    }
}
```

- [ ] **Step 3: Register the module.** In `shirita-core/src/lib.rs` add `pub mod keyword;` (next to the other `pub mod` lines).

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -p shirita-core keyword::`
Expected: PASS (4 tests). (First run downloads `aho-corasick`/`rand`.)

- [ ] **Step 5: Commit**

```bash
git add shirita-core/Cargo.toml shirita-core/src/keyword.rs shirita-core/src/lib.rs
git commit -m "feat(core): Aho-Corasick keyword index"
```

---

## Task 3: Trigger model + activation

Parse `definition.meta.trigger`; compute the active set across a list of entries given the scan buffer, the recursive toggle, and a roll closure for random.

**Files:**
- Modify: `shirita-core/src/assembly.rs`

- [ ] **Step 1: Write the failing test.** Add to the `tests` module in `shirita-core/src/assembly.rs`:

```rust
    use crate::assembly::{activate, parse_trigger, Trigger, TriggerMode, Entry};

    fn ent(id: &str, mode: TriggerMode, keys: &[&str], content: &str) -> Entry {
        Entry {
            id: id.to_string(),
            trigger: Trigger { mode, keys: keys.iter().map(|s| s.to_string()).collect(), probability: 100 },
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
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cargo test -p shirita-core assembly::tests::activate_constant_always_keyword_on_match`
Expected: FAIL (types/functions undefined).

- [ ] **Step 3: Implement.** Add to `shirita-core/src/assembly.rs` (top-level, after the existing `use`s):

```rust
use std::collections::HashSet;
use crate::keyword::KeywordIndex;

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
            TriggerMode::Constant => { active.insert(e.id.clone()); }
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
    // 把已激活条目内容并入初始缓冲（constant 也参与递归来源）。
    for e in entries {
        if active.contains(&e.id) {
            scan_text.push('\n');
            scan_text.push_str(&e.content);
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
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -p shirita-core assembly::tests::`
Expected: PASS for the new trigger/activate tests (old assembly tests still pass too).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/assembly.rs
git commit -m "feat(core): world-book trigger model + activation (constant/keyword/random + recursion)"
```

---

## Task 4: `assemble_from_nodes` → structured segments

Walk the tree, build `Entry` list for activation, then emit ordered `PromptSegment`s split by the history node.

**Files:**
- Modify: `shirita-core/src/assembly.rs`, `shirita-core/src/lib.rs`

- [ ] **Step 1: Write the failing test.** Add to `assembly.rs` tests:

```rust
    use crate::assembly::{assemble_from_nodes, AssembledPlan, Placement};
    use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};

    fn folder(owner: &str, sort: i64, tag: &str) -> PromptNode {
        PromptNode::new_folder(OwnerKind::Template, owner, None, sort, tag)
    }
    fn child_ref(owner: &str, parent: &str, sort: i64, def: &str) -> PromptNode {
        PromptNode::new_ref(OwnerKind::Template, owner, Some(parent.to_string()), sort, def)
    }
    fn root_ref(owner: &str, sort: i64, def: &str) -> PromptNode {
        PromptNode::new_ref(OwnerKind::Template, owner, None, sort, def)
    }
    fn history(owner: &str, sort: i64) -> PromptNode {
        let mut n = PromptNode::new_folder(OwnerKind::Template, owner, None, sort, "history");
        n.kind = NodeKind::History;
        n.tag = None;
        n
    }

    #[test]
    fn assemble_wraps_containers_splits_history() {
        use crate::models::definition::{Definition, DefinitionType};
        let neo = Definition::new(DefinitionType::Char, "Neo", "Neo body");
        let jb = Definition::new(DefinitionType::Prompt, "JB", "Jailbreak body");
        let charf = folder("t", 0, "char");
        let cref = child_ref("t", &charf.id, 0, &neo.id);
        let hist = history("t", 1);
        let after = root_ref("t", 2, &jb.id);

        let mut defs = std::collections::HashMap::new();
        defs.insert(neo.id.clone(), neo.clone());
        defs.insert(jb.id.clone(), jb.clone());

        let nodes = vec![charf, cref, hist, after];
        let plan = assemble_from_nodes(
            &nodes, &defs, &json!({}), &json!({}), &["hi".to_string()], true, 4, &mut || 0.0,
        );
        // before: one <char> segment; after: jailbreak raw.
        let before: Vec<_> = plan.segments.iter().filter(|s| s.placement == Placement::BeforeHistory).collect();
        let after_s: Vec<_> = plan.segments.iter().filter(|s| s.placement == Placement::AfterHistory).collect();
        assert_eq!(before.len(), 1);
        assert!(before[0].content.contains("<char>\nNeo body\n</char>"));
        assert_eq!(after_s.len(), 1);
        assert_eq!(after_s[0].content, "Jailbreak body");
        assert!(plan.history_enabled);
    }

    #[test]
    fn assemble_omits_empty_container_and_inactive_refs() {
        use crate::models::definition::{Definition, DefinitionType};
        let mut lore = Definition::new(DefinitionType::World, "Zion", "Zion body");
        lore.meta = json!({ "trigger": { "mode": "keyword", "keys": ["zion"] } });
        let wf = folder("t", 0, "world");
        let wref = child_ref("t", &wf.id, 0, &lore.id);
        let mut defs = std::collections::HashMap::new();
        defs.insert(lore.id.clone(), lore.clone());
        let nodes = vec![wf, wref];
        // No "zion" in buffer → world container empty → omitted.
        let plan = assemble_from_nodes(&nodes, &defs, &json!({}), &json!({}), &["hi".into()], false, 4, &mut || 0.0);
        assert!(plan.segments.is_empty());
    }
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cargo test -p shirita-core assembly::tests::assemble_wraps_containers_splits_history`
Expected: FAIL (undefined `assemble_from_nodes`/`AssembledPlan`/`Placement`).

- [ ] **Step 3: Implement.** Add to `shirita-core/src/assembly.rs`:

```rust
use std::collections::HashMap;
use crate::models::prompt_node::{NodeKind, PromptNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    BeforeHistory,
    AfterHistory,
}

#[derive(Debug, Clone)]
pub struct PromptSegment {
    pub placement: Placement,
    pub content: String, // 已封包/裸文本，已渲染 {{var}}
    pub source: String,  // 溯源：folder tag 或 def id
}

#[derive(Debug, Clone)]
pub struct AssembledPlan {
    pub segments: Vec<PromptSegment>,
    pub history_enabled: bool,
}

/// 取定义的有效 trigger（局部覆盖优先）。
fn effective_trigger(def: &Definition, overrides: &serde_json::Value) -> Trigger {
    if let Some(t) = overrides.get(&def.id).and_then(|o| o.get("trigger")) {
        return parse_trigger(&serde_json::json!({ "trigger": t }));
    }
    parse_trigger(&def.meta)
}

/// 把树组装为有序段；history 节点切分 before/after。
#[allow(clippy::too_many_arguments)]
pub fn assemble_from_nodes(
    nodes: &[PromptNode],
    definitions: &HashMap<String, Definition>,
    overrides: &serde_json::Value,
    state: &serde_json::Value,
    recent_msgs: &[String],
    recursive: bool,
    _scan_depth: usize, // 调用方已按深度裁剪 recent_msgs；保留以备扩展
    roll: &mut impl FnMut() -> f64,
) -> AssembledPlan {
    // 1) 收集全部 ref 条目（含容器子节点）做激活。
    let mut entries: Vec<Entry> = Vec::new();
    for n in nodes {
        if n.kind == NodeKind::Ref {
            if let Some(def) = n.definition_id.as_ref().and_then(|id| definitions.get(id)) {
                entries.push(Entry {
                    id: def.id.clone(),
                    trigger: effective_trigger(def, overrides),
                    content: render_vars(&effective_content(def, overrides), state),
                });
            }
        }
    }
    let buffer = recent_msgs.join("\n");
    let active = activate(&entries, &buffer, recursive, roll);

    let render = |def: &Definition| render_vars(&effective_content(def, overrides), state);
    let is_included = |n: &PromptNode| -> Option<String> {
        if !n.enabled || n.kind != NodeKind::Ref {
            return None;
        }
        let def = n.definition_id.as_ref().and_then(|id| definitions.get(id))?;
        if !active.contains(&def.id) {
            return None;
        }
        Some(render(def))
    };

    // 2) 遍历根级，遇 history 切分。
    let mut segments = Vec::new();
    let mut placement = Placement::BeforeHistory;
    let mut history_enabled = false;
    let roots: Vec<&PromptNode> = {
        let mut r: Vec<&PromptNode> = nodes.iter().filter(|n| n.parent_id.is_none()).collect();
        r.sort_by_key(|n| n.sort_order);
        r
    };
    for root in roots {
        match root.kind {
            NodeKind::History => {
                if root.enabled {
                    history_enabled = true;
                    placement = Placement::AfterHistory;
                }
            }
            NodeKind::Folder => {
                if !root.enabled {
                    continue;
                }
                let tag = root.tag.clone().unwrap_or_default();
                let mut kids: Vec<&PromptNode> =
                    nodes.iter().filter(|n| n.parent_id.as_deref() == Some(root.id.as_str())).collect();
                kids.sort_by_key(|n| n.sort_order);
                let bodies: Vec<String> = kids.iter().filter_map(|k| is_included(k)).collect();
                if !bodies.is_empty() {
                    segments.push(PromptSegment {
                        placement,
                        content: format!("<{tag}>\n{}\n</{tag}>", bodies.join("\n")),
                        source: tag,
                    });
                }
            }
            NodeKind::Ref => {
                if let Some(body) = is_included(root) {
                    segments.push(PromptSegment {
                        placement,
                        content: body, // 根级 ref（prompt）裸文本
                        source: root.definition_id.clone().unwrap_or_default(),
                    });
                }
            }
        }
    }

    AssembledPlan { segments, history_enabled }
}
```

- [ ] **Step 4: Re-export.** In `shirita-core/src/lib.rs`, ensure `assembly` items are reachable (it already does `pub mod assembly;` / re-exports). Add to the `pub use assembly::{…}` line (or add one): `pub use assembly::{assemble_from_nodes, build_chat_messages, AssembledPlan, Placement, PromptSegment};` — `build_chat_messages` is added in Task 5, so for now export the available names and extend in Task 5.

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test -p shirita-core assembly::`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/assembly.rs shirita-core/src/lib.rs
git commit -m "feat(core): assemble_from_nodes — structured segments + history split"
```

---

## Task 5: `build_chat_messages` (segments → provider messages)

**Files:**
- Modify: `shirita-core/src/assembly.rs`, `shirita-core/src/lib.rs`

- [ ] **Step 1: Write the failing test.** Add to `assembly.rs` tests:

```rust
    use crate::assembly::build_chat_messages;
    use crate::model::ChatMessage;
    use crate::models::message::Role;

    #[test]
    fn build_messages_merges_same_role_and_inserts_history() {
        let plan = AssembledPlan {
            segments: vec![
                PromptSegment { placement: Placement::BeforeHistory, content: "A".into(), source: "a".into() },
                PromptSegment { placement: Placement::BeforeHistory, content: "B".into(), source: "b".into() },
                PromptSegment { placement: Placement::AfterHistory, content: "JB".into(), source: "jb".into() },
            ],
            history_enabled: true,
        };
        let history = vec![ChatMessage { role: Role::User, content: "hi".into() }];
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
            segments: vec![
                PromptSegment { placement: Placement::BeforeHistory, content: "A".into(), source: "a".into() },
                PromptSegment { placement: Placement::AfterHistory, content: "B".into(), source: "b".into() },
            ],
            history_enabled: false,
        };
        let history = vec![ChatMessage { role: Role::User, content: "hi".into() }];
        let msgs = build_chat_messages(&plan, &history, false);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::System);
        assert_eq!(msgs[0].content, "A\nB");
    }
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cargo test -p shirita-core assembly::tests::build_messages_merges_same_role_and_inserts_history`
Expected: FAIL (undefined `build_chat_messages`).

- [ ] **Step 3: Implement.** Add to `shirita-core/src/assembly.rs`:

```rust
use crate::model::ChatMessage;
use crate::models::message::Role;

/// 段 + 真实历史 → provider 消息数组；末了合并相邻同角色。
pub fn build_chat_messages(
    plan: &AssembledPlan,
    history: &[ChatMessage],
    history_enabled: bool,
) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::new();
    let push_sys = |out: &mut Vec<ChatMessage>, c: &str| {
        out.push(ChatMessage { role: Role::System, content: c.to_string() });
    };

    for s in plan.segments.iter().filter(|s| s.placement == Placement::BeforeHistory) {
        push_sys(&mut out, &s.content);
    }
    if history_enabled && plan.history_enabled {
        out.extend(history.iter().cloned());
    }
    for s in plan.segments.iter().filter(|s| s.placement == Placement::AfterHistory) {
        push_sys(&mut out, &s.content);
    }

    // 合并相邻同角色（OpenAI 多 system 合一；Claude 需要）。
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
```

- [ ] **Step 4: Extend the re-export** in `shirita-core/src/lib.rs` to include `build_chat_messages` (from Task 4's `pub use assembly::{…}`).

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test -p shirita-core assembly::`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/assembly.rs shirita-core/src/lib.rs
git commit -m "feat(core): build_chat_messages — serialize segments + history, merge same-role"
```

---

## Task 6: Effective-tree resolution + reference (no eager copy) + auto history node

The session uses its own nodes if it has any (a future fork), else the referenced template's. `create_session` (web) stops deep-copying. Template create auto-adds a history node.

**Files:**
- Modify: `shirita-core/src/conversation.rs` (helper), `shirita-web/src/routes/sessions.rs`, `shirita-web/src/routes/templates.rs`

- [ ] **Step 1: Write the failing test** (core helper). Add to `shirita-core/src/conversation.rs` tests module:

```rust
    #[tokio::test]
    async fn effective_nodes_prefers_session_else_template() {
        use crate::models::prompt_node::{OwnerKind, PromptNode};
        use crate::models::template::Template;
        let storage = temp_storage().await;
        // template with one folder node
        let t = Template::new("T");
        storage.create_template(&t).await.unwrap();
        let f = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "char");
        storage.create_node(&f).await.unwrap();

        // session references template, has no own nodes
        let mut sess = Session::new("s");
        sess.template_id = Some(t.id.clone());
        storage.create_session(&sess).await.unwrap();

        let nodes = super::effective_nodes(&storage, &sess).await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].tag.as_deref(), Some("char"));
    }
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cargo test -p shirita-core conversation::tests::effective_nodes_prefers_session_else_template`
Expected: FAIL (no `effective_nodes`).

- [ ] **Step 3: Implement the helper** in `shirita-core/src/conversation.rs` (module-level, above `send_message`):

```rust
use crate::models::prompt_node::{OwnerKind, PromptNode};
use crate::models::session::Session;

/// 会话有效节点树：自有节点优先（fork 后），否则引用模板。
pub async fn effective_nodes(
    storage: &dyn Storage,
    session: &Session,
) -> crate::Result<Vec<PromptNode>> {
    let own = storage.list_nodes(&OwnerKind::Session, &session.id).await?;
    if !own.is_empty() {
        return Ok(own);
    }
    if let Some(tid) = &session.template_id {
        return storage.list_nodes(&OwnerKind::Template, tid).await;
    }
    Ok(Vec::new())
}
```
(The test imports `Session` already via the existing `use crate::models::session::Session;` in tests; if not present there, add it.)

- [ ] **Step 4: Run the core test, verify pass**

Run: `cargo test -p shirita-core conversation::tests::effective_nodes_prefers_session_else_template`
Expected: PASS.

- [ ] **Step 5: Reference, not copy (web).** In `shirita-web/src/routes/sessions.rs`, find the `create_session` handler. **Remove** the block that calls `storage.copy_nodes(&OwnerKind::Template, &template_id, &OwnerKind::Session, &session.id)` after creating the session. Keep setting `session.template_id`. (Leave the rest — name/avatar — unchanged.) Verify by reading the handler before editing; the deep-copy is the lines invoking `copy_nodes`.

- [ ] **Step 6: Auto history node on template create (web).** In `shirita-web/src/routes/templates.rs`, in the `create` handler, after `storage.create_template(&template)`, add:

```rust
    let mut hist = shirita_core::PromptNode::new_folder(
        shirita_core::OwnerKind::Template, &template.id, None, 0, "history",
    );
    hist.kind = shirita_core::NodeKind::History;
    hist.tag = None;
    storage.create_node(&hist).await.map_err(internal)?;
```
(Use the crate's existing error-mapping helper used elsewhere in the file — match the surrounding `.map_err(...)` style; `internal` is illustrative.)

- [ ] **Step 7: Run the workspace build + web tests, verify pass**

Run: `cargo test -p shirita-web` and `cargo build`
Expected: PASS / builds. (If a web `create_session` test asserted copied nodes, update it to assert the session references the template and has no own nodes.)

- [ ] **Step 8: Commit**

```bash
git add shirita-core/src/conversation.rs shirita-web/src/routes/sessions.rs shirita-web/src/routes/templates.rs
git commit -m "feat: sessions reference template (no eager copy) + auto history node + effective_nodes"
```

---

## Task 7: Rewire `send_message` to the tree pipeline

**Files:**
- Modify: `shirita-core/src/conversation.rs`

- [ ] **Step 1: Update the existing assembly test.** In `shirita-core/src/conversation.rs`, the test `assembled_system_is_sent` currently mounts a definition and asserts `<characters>`. Replace its body to drive via a **template tree** instead of `mounted_definitions`:

```rust
    #[tokio::test]
    async fn assembled_system_is_sent() {
        use crate::models::prompt_node::{OwnerKind, PromptNode};
        use crate::models::template::Template;
        let storage = Arc::new(temp_storage().await);
        let ch = crate::models::definition::Definition::new(
            crate::models::definition::DefinitionType::Char, "C", "I am {{who}}",
        );
        storage.create_definition(&ch).await.unwrap();

        let t = Template::new("T");
        storage.create_template(&t).await.unwrap();
        let f = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "char");
        storage.create_node(&f).await.unwrap();
        let r = PromptNode::new_ref(OwnerKind::Template, &t.id, Some(f.id.clone()), 0, &ch.id);
        storage.create_node(&r).await.unwrap();

        let mut session = Session::new("t");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "who": "Neo" });
        storage.create_session(&session).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        assert_eq!(req.messages[0].role, Role::System);
        assert!(req.messages[0].content.contains("<char>"));
        assert!(req.messages[0].content.contains("I am Neo"));
    }
```

> Keep `regex_rule_sets_display_content` working: regex rules are no longer pulled from `mounted_definitions`. For now, source regex rules from **all `regex_rule` definitions** (Settings owns them per spec). Update that test to `create_definition` the rule (no mount) — it will be picked up by the new global-regex query below.

- [ ] **Step 2: Run it, verify it fails**

Run: `cargo test -p shirita-core conversation::tests::assembled_system_is_sent`
Expected: FAIL (still using old `assemble_system_prompt`/mounted).

- [ ] **Step 3: Rewire `send_message`.** In `shirita-core/src/conversation.rs`, replace the block from `// 2) 用已载入的 session ...` through the construction of `chat_messages` (lines that build `mounted`, `system`, and push system/history/user) with:

```rust
        // 2) 取会话有效树 + 定义 + 局部覆盖 + 最近消息，按树组装。
        let nodes = match effective_nodes(storage.as_ref(), &session).await {
            Ok(n) => n,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let mut defs = std::collections::HashMap::new();
        for n in &nodes {
            if let Some(did) = &n.definition_id {
                if !defs.contains_key(did) {
                    if let Ok(Some(d)) = storage.get_definition(did).await {
                        defs.insert(did.clone(), d);
                    }
                }
            }
        }
        let local = session.override_config.get("local_definitions").cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        // 扫描窗口：最近 N 条（含将发送的 user）。
        let scan_depth = 4usize;
        let mut recent: Vec<String> = history.iter().rev().take(scan_depth.saturating_sub(1))
            .map(|m| m.raw_content.clone()).collect();
        recent.reverse();
        recent.push(user_text.clone());

        let mut rng = rand::thread_rng();
        let plan = crate::assembly::assemble_from_nodes(
            &nodes, &defs, &local, &session.current_state, &recent, true, scan_depth,
            &mut || rand::Rng::gen::<f64>(&mut rng),
        );

        // 真实历史（过滤隐藏）+ 本次 user。
        let mut hist_msgs: Vec<ChatMessage> = history.iter().filter(|m| !m.is_hidden)
            .map(|m| ChatMessage { role: m.role, content: m.raw_content.clone() }).collect();
        hist_msgs.push(ChatMessage { role: Role::User, content: user_text.clone() });

        let history_enabled = true;
        let chat_messages = crate::assembly::build_chat_messages(&plan, &hist_msgs, history_enabled);

        // regex 规则：所有 regex_rule 定义（Settings 拥有）。
        let regex_rules: Vec<_> = match storage.list_definitions().await {
            Ok(all) => all.into_iter()
                .filter(|d| d.def_type == crate::models::definition::DefinitionType::RegexRule).collect(),
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
```
Leave the `prompt_text`/token-count line and everything after (`let req = ChatRequest { model, messages: chat_messages };` onward) unchanged.

- [ ] **Step 4: Run the core tests, verify pass**

Run: `cargo test -p shirita-core conversation::`
Expected: PASS (echo test, assembled-system test, regex test, unknown-session test).

- [ ] **Step 5: Remove the dead M2 assembler.** Delete `assemble_system_prompt` and `wrap_tag` from `assembly.rs` (now unused), and their `assemble_groups_in_order_with_tags` + `local_override_replaces_content` tests (covered by the new tests). Run `cargo test -p shirita-core` to confirm nothing else references them.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/conversation.rs shirita-core/src/assembly.rs
git commit -m "feat(core): send_message assembles from the session tree; retire M2 group-by-type"
```

---

## Task 8: Web integration test — templated session drives the prompt

**Files:**
- Modify: `shirita-web/tests/` (add a test, following the existing integration-test pattern in that dir)

- [ ] **Step 1: Write the failing test.** Find the existing web integration test file (e.g. `shirita-web/tests/api.rs`) and add a test that: creates a template, a `char` folder + a char definition ref, a history node; creates a session with that `template_id`; POSTs a message; and asserts the EchoProvider-backed response streamed (offline) — i.e. the request path works end-to-end. Mirror the existing helper that builds the `app(state)` with a temp DB and bearer token. Concretely assert: `GET /api/sessions/{id}/nodes?owner_kind=template`-style setup succeeds and `POST /api/sessions/{id}/messages` returns 200 with SSE `data:` lines.

```rust
// Pseudocode shape — adapt to the existing test harness in this file:
// 1. let app = test_app().await;  (existing helper)
// 2. POST /api/templates {name:"T"} -> tid
// 3. POST /api/definitions {type:"char",name:"Neo",content:"Neo body"} -> did
// 4. POST /api/templates/{tid}/nodes {kind:"folder",tag:"char"} -> fid
// 5. POST /api/templates/{tid}/nodes {kind:"ref",parent_id:fid,definition_id:did}
// 6. POST /api/sessions {name:"s",template_id:tid} -> sid
// 7. POST /api/sessions/{sid}/messages {text:"hi"} -> 200, body contains "data:"
```

- [ ] **Step 2: Run it, verify it fails** (before wiring, or asserts the new behavior)

Run: `cargo test -p shirita-web`
Expected: the new test FAILs first if asserting new behavior, else confirms the path.

- [ ] **Step 3: Make it pass** — fixes should already be in place from Tasks 6–7; resolve any remaining wiring (e.g. ensure `POST /api/sessions` accepts `template_id` without copying).

- [ ] **Step 4: Run the full workspace test suite**

Run: `cargo test` (workspace) and `cargo clippy --all-targets`
Expected: PASS, no new clippy errors.

- [ ] **Step 5: Commit**

```bash
git add shirita-web/tests
git commit -m "test(web): templated session tree drives assembly end-to-end"
```

---

## Self-review checklist (run before handing off to execution)

- **Spec coverage:** history kind (T1) ✓ · Aho-Corasick scan (T2, spec §6) ✓ · trigger model constant/keyword/random + recursion toggle (T3, §5/§6) ✓ · structured `assemble_from_nodes` + container wrap + root-raw prompt + history split (T4, §8) ✓ · `build_chat_messages` same-role merge (T5, §8) ✓ · reference-not-copy + auto history (T6, §7) ✓ · `send_message` rewire + retire M2 (T7, §8) ✓ · e2e (T8, §15) ✓. **Not here (later plans):** `def_types`/extensible types, `/api/types`, session-node HTTP endpoints, frontend.
- **Type consistency:** `Trigger{mode,keys,probability}`, `TriggerMode`, `Entry{id,trigger,content}`, `Placement`, `PromptSegment{placement,content,source}`, `AssembledPlan{segments,history_enabled}`, `assemble_from_nodes(nodes,defs,overrides,state,recent_msgs,recursive,scan_depth,roll)`, `build_chat_messages(plan,history,history_enabled)`, `effective_nodes(storage,session)` — names used identically across tasks.
- **Placeholders:** the web tasks (T6 step 5/6, T8) reference existing handlers/harness by pattern rather than verbatim code because those files weren't read in full while planning — the executor must open `routes/sessions.rs`, `routes/templates.rs`, and the `tests/` harness and match the local error-handling/test idioms. Flag for the executor.
- **scan_depth:** passed through but the actual depth-trim happens in `send_message` (recent N messages); `assemble_from_nodes` takes the already-trimmed slice. Consistent.
```
