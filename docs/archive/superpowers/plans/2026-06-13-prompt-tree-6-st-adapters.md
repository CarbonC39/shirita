# Prompt Tree v2 — Plan 6: SillyTavern import/export adapters

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Round-trip our data with SillyTavern: import/export World Info lorebooks, Character Cards (V2/V3), and template presets, reusing the trigger structure Plan 1 deliberately aligned to ST field names.

**Architecture:** Pure conversion functions in a new `shirita-core::adapters` module (no DB, no I/O) — each takes/produces `serde_json::Value` ↔ our `Definition`/template structures, so they're trivially unit-testable. Thin web endpoints in `shirita-web` adapt HTTP bodies and persist via `Storage`. Lenient on import (accept ST's two lorebook shapes), canonical on export.

**Tech Stack:** Rust, `serde_json`, existing `Storage`; Axum endpoints; reuses `assembly::parse_trigger` (Plan 1) for trigger semantics.

**Spec:** `docs/superpowers/specs/2026-06-13-prompt-tree-worldbook-design.md` §11 (export/import compatibility), §14 (security — sanitize imports). Depends on Plan 2 (`def_type` is a String; `world` is a registered container).

**Out of scope:** ST `@depth` positions (we only model before/after — preserved via `extensions` passthrough on export, ignored on import beyond before/after); group chats; assets/embedded images in cards.

---

## File structure

- `shirita-core/src/adapters/mod.rs` — **new**: module root.
- `shirita-core/src/adapters/worldinfo.rs` — **new**: `worldinfo_to_defs` / `defs_to_worldinfo`.
- `shirita-core/src/adapters/charcard.rs` — **new**: `charcard_to_defs` / `def_to_charcard`.
- `shirita-core/src/adapters/preset.rs` — **new**: `preset_to_tree` / `tree_to_preset`.
- `shirita-core/src/lib.rs` — **modify**: `pub mod adapters;` + re-exports.
- `shirita-web/src/routes/import_export.rs` — **new**: import/export endpoints.
- `shirita-web/src/lib.rs`, `shirita-web/src/routes/mod.rs` — **modify**: mount routes.

> All adapter fns return `Vec<Definition>` (or `serde_json::Value`) and **never touch the DB** — callers persist. The `id` on produced `Definition`s is a fresh uuid (via `Definition::new`) so imports don't collide.

---

## Task 1: World Info — import (`worldinfo_to_defs`)

ST lorebooks come in two shapes: standalone WI `{ "entries": { "0": { key, keysecondary, comment, content, constant, order, disable, probability, useProbability } } }` (map), and character-book `{ "entries": [ { keys, secondary_keys, comment, content, constant, insertion_order, enabled } ] }` (array). Accept both.

**Files:**
- Create: `shirita-core/src/adapters/mod.rs`, `shirita-core/src/adapters/worldinfo.rs`
- Modify: `shirita-core/src/lib.rs`, `shirita-core/src/models/mod.rs` is unaffected

- [ ] **Step 1: Write the failing test.** Create `shirita-core/src/adapters/worldinfo.rs`:

```rust
//! SillyTavern World Info / lorebook ↔ 我们的 world 类型定义（带 meta.trigger）。

use crate::models::definition::Definition;

/// 把 ST 世界书 JSON（map 形 或 array 形 entries）转成 world 定义列表。
pub fn worldinfo_to_defs(wi: &serde_json::Value) -> Vec<Definition> {
    let entries = match wi.get("entries") {
        Some(serde_json::Value::Object(map)) => map.values().cloned().collect::<Vec<_>>(),
        Some(serde_json::Value::Array(arr)) => arr.clone(),
        _ => Vec::new(),
    };
    entries.iter().map(entry_to_def).collect()
}

fn str_array(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn entry_to_def(e: &serde_json::Value) -> Definition {
    // keys：ST 标准用 "key"，character_book 用 "keys"。
    let keys = if e.get("key").is_some() { str_array(e.get("key")) } else { str_array(e.get("keys")) };
    let constant = e.get("constant").and_then(|v| v.as_bool()).unwrap_or(false);
    let use_prob = e.get("useProbability").and_then(|v| v.as_bool()).unwrap_or(false);
    let probability = e.get("probability").and_then(|v| v.as_u64()).unwrap_or(100).min(100);
    let mode = if constant { "constant" } else if !keys.is_empty() { "keyword" } else if use_prob { "random" } else { "constant" };
    let name = e.get("comment").and_then(|v| v.as_str()).unwrap_or("Imported entry").to_string();
    let content = e.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let order = e.get("order").or_else(|| e.get("insertion_order")).and_then(|v| v.as_u64()).unwrap_or(100);

    let mut def = Definition::new("world", name, content);
    def.meta = serde_json::json!({
        "trigger": { "mode": mode, "keys": keys, "probability": probability, "order": order }
    });
    def
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_standalone_map_entries() {
        let wi = serde_json::json!({
            "entries": {
                "0": { "key": ["zion"], "comment": "Zion", "content": "Last city", "constant": false, "order": 5 }
            }
        });
        let defs = worldinfo_to_defs(&wi);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].def_type, "world");
        assert_eq!(defs[0].name, "Zion");
        assert_eq!(defs[0].meta["trigger"]["mode"], "keyword");
        assert_eq!(defs[0].meta["trigger"]["keys"][0], "zion");
        assert_eq!(defs[0].meta["trigger"]["order"], 5);
    }

    #[test]
    fn imports_character_book_array_with_constant() {
        let wi = serde_json::json!({
            "entries": [ { "keys": [], "comment": "Lore", "content": "x", "constant": true } ]
        });
        let defs = worldinfo_to_defs(&wi);
        assert_eq!(defs[0].meta["trigger"]["mode"], "constant");
    }
}
```

- [ ] **Step 2: Register the module.** Create `shirita-core/src/adapters/mod.rs`:

```rust
//! 与外部工具（SillyTavern）往返的导入/导出适配器。纯函数，不触库。
pub mod charcard;
pub mod preset;
pub mod worldinfo;
```
In `shirita-core/src/lib.rs` add `pub mod adapters;`. (The `charcard`/`preset` modules are created in later tasks; to compile T1 alone, temporarily comment those two `pub mod` lines and uncomment them in T3/T5 — or create empty files now.)

- [ ] **Step 3: Run tests, verify pass**

Run: `cargo test -p shirita-core adapters::worldinfo`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add shirita-core/src/adapters/mod.rs shirita-core/src/adapters/worldinfo.rs shirita-core/src/lib.rs
git commit -m "feat(core): import ST World Info (map + array entries) → world defs"
```

---

## Task 2: World Info — export (`defs_to_worldinfo`)

**Files:**
- Modify: `shirita-core/src/adapters/worldinfo.rs`

- [ ] **Step 1: Write the failing test.** Add to `worldinfo.rs` tests:

```rust
    #[test]
    fn exports_defs_to_standalone_map() {
        let mut d = Definition::new("world", "Zion", "Last city");
        d.meta = serde_json::json!({ "trigger": { "mode": "keyword", "keys": ["zion"], "probability": 100, "order": 7 } });
        let wi = defs_to_worldinfo(&[d]);
        let e = &wi["entries"]["0"];
        assert_eq!(e["comment"], "Zion");
        assert_eq!(e["content"], "Last city");
        assert_eq!(e["key"][0], "zion");
        assert_eq!(e["constant"], false);
        assert_eq!(e["order"], 7);
        assert_eq!(e["disable"], false);
    }

    #[test]
    fn worldinfo_roundtrips() {
        let mut d = Definition::new("world", "Trinity", "She");
        d.meta = serde_json::json!({ "trigger": { "mode": "keyword", "keys": ["trinity", "she"], "probability": 100, "order": 100 } });
        let back = worldinfo_to_defs(&defs_to_worldinfo(std::slice::from_ref(&d)));
        assert_eq!(back[0].name, "Trinity");
        assert_eq!(back[0].meta["trigger"]["keys"], serde_json::json!(["trinity", "she"]));
        assert_eq!(back[0].meta["trigger"]["mode"], "keyword");
    }
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cargo test -p shirita-core adapters::worldinfo::tests::exports_defs_to_standalone_map`
Expected: FAIL (`defs_to_worldinfo` undefined).

- [ ] **Step 3: Implement.** Add to `worldinfo.rs`, reusing `parse_trigger` from assembly for canonical reads:

```rust
use crate::assembly::{parse_trigger, TriggerMode};

/// world 定义列表 → ST 标准世界书 JSON（map 形 entries，键为序号）。
pub fn defs_to_worldinfo(defs: &[Definition]) -> serde_json::Value {
    let mut entries = serde_json::Map::new();
    for (i, d) in defs.iter().enumerate() {
        let t = parse_trigger(&d.meta);
        let constant = matches!(t.mode, TriggerMode::Constant);
        let use_prob = matches!(t.mode, TriggerMode::Random);
        let order = d.meta.get("trigger").and_then(|x| x.get("order")).and_then(|v| v.as_u64()).unwrap_or(100);
        entries.insert(
            i.to_string(),
            serde_json::json!({
                "uid": i,
                "key": t.keys,
                "keysecondary": [],
                "comment": d.name,
                "content": d.content,
                "constant": constant,
                "selective": matches!(t.mode, TriggerMode::Keyword),
                "order": order,
                "position": 0,
                "disable": false,
                "probability": t.probability,
                "useProbability": use_prob,
            }),
        );
    }
    serde_json::json!({ "entries": entries })
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -p shirita-core adapters::worldinfo`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/adapters/worldinfo.rs
git commit -m "feat(core): export world defs → ST World Info (roundtrips)"
```

---

## Task 3: Character Card V2/V3 — import (`charcard_to_defs`)

A card produces one `char` definition (name + description→content) plus the embedded `character_book` entries as `world` definitions (via Task 1's importer).

**Files:**
- Create: `shirita-core/src/adapters/charcard.rs`

- [ ] **Step 1: Write the failing test.** Create `shirita-core/src/adapters/charcard.rs`:

```rust
//! SillyTavern Character Card V2/V3 ↔ char 定义（+ 内嵌 character_book → world 定义）。

use crate::adapters::worldinfo::worldinfo_to_defs;
use crate::models::definition::Definition;

/// 解析 chara_card_v2/v3：返回 (char 定义, 内嵌世界书定义列表)。
pub fn charcard_to_defs(card: &serde_json::Value) -> (Definition, Vec<Definition>) {
    let data = card.get("data").unwrap_or(card);
    let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("Imported character").to_string();
    let description = data.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let mut def = Definition::new("char", name, description);
    // 保留 ST 扩展字段以便回出口（不丢信息）。
    def.meta = serde_json::json!({
        "st": {
            "personality": data.get("personality"),
            "scenario": data.get("scenario"),
            "first_mes": data.get("first_mes"),
            "mes_example": data.get("mes_example"),
            "system_prompt": data.get("system_prompt"),
            "post_history_instructions": data.get("post_history_instructions"),
        }
    });

    let book_defs = data
        .get("character_book")
        .map(worldinfo_to_defs)
        .unwrap_or_default();
    (def, book_defs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_v2_card_with_book() {
        let card = serde_json::json!({
            "spec": "chara_card_v2", "spec_version": "2.0",
            "data": {
                "name": "Neo", "description": "The One",
                "character_book": { "entries": [ { "keys": ["zion"], "comment": "Zion", "content": "Last city" } ] }
            }
        });
        let (ch, book) = charcard_to_defs(&card);
        assert_eq!(ch.def_type, "char");
        assert_eq!(ch.name, "Neo");
        assert_eq!(ch.content, "The One");
        assert_eq!(book.len(), 1);
        assert_eq!(book[0].def_type, "world");
        assert_eq!(book[0].meta["trigger"]["keys"][0], "zion");
    }
}
```

- [ ] **Step 2: Uncomment the module.** Ensure `pub mod charcard;` is active in `shirita-core/src/adapters/mod.rs`.

- [ ] **Step 3: Run tests, verify pass**

Run: `cargo test -p shirita-core adapters::charcard`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add shirita-core/src/adapters/charcard.rs shirita-core/src/adapters/mod.rs
git commit -m "feat(core): import ST Character Card V2/V3 → char def + embedded book"
```

---

## Task 4: Character Card — export (`def_to_charcard`)

**Files:**
- Modify: `shirita-core/src/adapters/charcard.rs`

- [ ] **Step 1: Write the failing test.** Add to `charcard.rs` tests:

```rust
    #[test]
    fn exports_char_with_book_to_v2() {
        let ch = Definition::new("char", "Neo", "The One");
        let mut lore = Definition::new("world", "Zion", "Last city");
        lore.meta = serde_json::json!({ "trigger": { "mode": "keyword", "keys": ["zion"], "probability": 100 } });
        let card = def_to_charcard(&ch, &[lore]);
        assert_eq!(card["spec"], "chara_card_v2");
        assert_eq!(card["data"]["name"], "Neo");
        assert_eq!(card["data"]["description"], "The One");
        // embedded book present (standalone WI map shape under character_book)
        assert_eq!(card["data"]["character_book"]["entries"]["0"]["comment"], "Zion");
    }
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cargo test -p shirita-core adapters::charcard::tests::exports_char_with_book_to_v2`
Expected: FAIL (`def_to_charcard` undefined).

- [ ] **Step 3: Implement.** Add to `charcard.rs`:

```rust
use crate::adapters::worldinfo::defs_to_worldinfo;

/// char 定义 (+ 关联世界书定义) → chara_card_v2 JSON。
pub fn def_to_charcard(ch: &Definition, book: &[Definition]) -> serde_json::Value {
    let st = ch.meta.get("st");
    let pick = |k: &str| st.and_then(|s| s.get(k)).cloned().unwrap_or(serde_json::Value::String(String::new()));
    serde_json::json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": {
            "name": ch.name,
            "description": ch.content,
            "personality": pick("personality"),
            "scenario": pick("scenario"),
            "first_mes": pick("first_mes"),
            "mes_example": pick("mes_example"),
            "system_prompt": pick("system_prompt"),
            "post_history_instructions": pick("post_history_instructions"),
            "alternate_greetings": [],
            "tags": [],
            "creator": "",
            "character_version": "",
            "character_book": defs_to_worldinfo(book),
            "extensions": {}
        }
    })
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -p shirita-core adapters::charcard`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/adapters/charcard.rs
git commit -m "feat(core): export char def (+book) → ST Character Card V2"
```

---

## Task 5: Preset — template tree ↔ ST-like preset

A minimal, lossless-enough preset: an ordered list of nodes with their definition content. Export walks the tree; import creates definitions + a template tree. Keep it simple (before/after history captured by node order; history node emitted as a marker).

**Files:**
- Create: `shirita-core/src/adapters/preset.rs`

- [ ] **Step 1: Write the failing test.** Create `shirita-core/src/adapters/preset.rs`:

```rust
//! 模板树 ↔ 类 ST preset JSON（prompt 顺序 + 容器/历史标记）。

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, PromptNode};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct PresetItem {
    pub kind: String,       // "container" | "ref" | "history"
    pub tag: Option<String>,
    pub name: Option<String>,
    pub content: Option<String>,
    pub def_type: Option<String>,
}

/// 把模板根级树序列化为有序 preset 项（容器内子项跟随其后，深度由 parent 决定）。
pub fn tree_to_preset(nodes: &[PromptNode], defs: &HashMap<String, Definition>) -> serde_json::Value {
    let mut roots: Vec<&PromptNode> = nodes.iter().filter(|n| n.parent_id.is_none()).collect();
    roots.sort_by_key(|n| n.sort_order);
    let mut items: Vec<serde_json::Value> = Vec::new();
    for r in roots {
        match r.kind {
            NodeKind::History => items.push(serde_json::json!({ "kind": "history" })),
            NodeKind::Folder => {
                let tag = r.tag.clone().unwrap_or_default();
                let mut kids: Vec<&PromptNode> =
                    nodes.iter().filter(|n| n.parent_id.as_deref() == Some(r.id.as_str())).collect();
                kids.sort_by_key(|n| n.sort_order);
                let children: Vec<serde_json::Value> = kids.iter().filter_map(|k| ref_item(k, defs)).collect();
                items.push(serde_json::json!({ "kind": "container", "tag": tag, "children": children }));
            }
            NodeKind::Ref => {
                if let Some(it) = ref_item(r, defs) { items.push(it); }
            }
        }
    }
    serde_json::json!({ "version": 1, "items": items })
}

fn ref_item(n: &PromptNode, defs: &HashMap<String, Definition>) -> Option<serde_json::Value> {
    let def = n.definition_id.as_ref().and_then(|id| defs.get(id))?;
    Some(serde_json::json!({
        "kind": "ref", "name": def.name, "content": def.content,
        "def_type": def.def_type, "meta": def.meta,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::prompt_node::OwnerKind;

    #[test]
    fn serializes_container_then_history() {
        let neo = Definition::new("char", "Neo", "body");
        let cf = PromptNode::new_folder(OwnerKind::Template, "t", None, 0, "char");
        let cref = PromptNode::new_ref(OwnerKind::Template, "t", Some(cf.id.clone()), 0, &neo.id);
        let mut hist = PromptNode::new_folder(OwnerKind::Template, "t", None, 1, "history");
        hist.kind = NodeKind::History; hist.tag = None;

        let mut defs = HashMap::new();
        defs.insert(neo.id.clone(), neo.clone());
        let out = tree_to_preset(&[cf.clone(), cref, hist], &defs);
        let items = out["items"].as_array().unwrap();
        assert_eq!(items[0]["kind"], "container");
        assert_eq!(items[0]["tag"], "char");
        assert_eq!(items[0]["children"][0]["name"], "Neo");
        assert_eq!(items[1]["kind"], "history");
    }
}
```

- [ ] **Step 2: Uncomment the module** (`pub mod preset;` in `adapters/mod.rs`) and run tests.

Run: `cargo test -p shirita-core adapters::preset`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add shirita-core/src/adapters/preset.rs shirita-core/src/adapters/mod.rs
git commit -m "feat(core): export template tree → preset JSON"
```

> **Preset import** (`preset_to_tree`) is symmetric but writes to storage (create defs + nodes), so it belongs with the web endpoint (Task 6) where a `Storage` handle exists. Add it there as a helper that, given the preset JSON + a target `template_id`, creates definitions and nodes.

---

## Task 6: Web import/export endpoints

**Files:**
- Create: `shirita-web/src/routes/import_export.rs`
- Modify: `shirita-web/src/routes/mod.rs`, `shirita-web/src/lib.rs`, `shirita-core/src/lib.rs` (re-exports)

- [ ] **Step 1: Re-export adapters.** In `shirita-core/src/lib.rs` add:

```rust
pub use adapters::charcard::{charcard_to_defs, def_to_charcard};
pub use adapters::preset::tree_to_preset;
pub use adapters::worldinfo::{defs_to_worldinfo, worldinfo_to_defs};
```

- [ ] **Step 2: Write the failing test.** Add `shirita-web/tests/import_export_test.rs` (harness like `template_assembly_test.rs`):

```rust
#[tokio::test]
async fn import_worldinfo_creates_world_defs() {
    let state = test_state().await;
    let body = r#"{"entries":{"0":{"key":["zion"],"comment":"Zion","content":"Last city","constant":false}}}"#;
    let (st, out) = send(&state, "POST", "/api/import/worldinfo", Some(body)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["created"], 1);
    // it is now listed as a world definition
    let (_, defs) = send(&state, "GET", "/api/definitions?type=world", None).await;
    assert!(json(&defs).as_array().unwrap().iter().any(|d| d["name"] == "Zion"));
}

#[tokio::test]
async fn import_charcard_creates_char_and_book() {
    let state = test_state().await;
    let card = r#"{"spec":"chara_card_v2","data":{"name":"Neo","description":"The One","character_book":{"entries":[{"keys":["zion"],"comment":"Zion","content":"x"}]}}}"#;
    let (st, out) = send(&state, "POST", "/api/import/charcard", Some(card)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["created"], 2); // char + 1 world
}
```

- [ ] **Step 3: Implement the routes.** Create `shirita-web/src/routes/import_export.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::AppState;

async fn persist(state: &AppState, defs: Vec<shirita_core::Definition>) -> Result<usize, StatusCode> {
    let n = defs.len();
    for d in defs {
        state.storage.create_definition(&d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(n)
}

/// POST /api/import/worldinfo — body 为 ST 世界书 JSON。
pub async fn import_worldinfo(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let defs = shirita_core::worldinfo_to_defs(&body);
    let created = persist(&state, defs).await?;
    Ok(Json(serde_json::json!({ "created": created })))
}

/// POST /api/import/charcard — body 为 chara_card_v2/v3 JSON。
pub async fn import_charcard(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let (ch, book) = shirita_core::charcard_to_defs(&body);
    let mut all = vec![ch];
    all.extend(book);
    let created = persist(&state, all).await?;
    Ok(Json(serde_json::json!({ "created": created })))
}
```

- [ ] **Step 4: Mount routes.** In `shirita-web/src/routes/mod.rs` add `pub mod import_export;`. In `shirita-web/src/lib.rs` (protected router):

```rust
        .route("/import/worldinfo", post(routes::import_export::import_worldinfo))
        .route("/import/charcard", post(routes::import_export::import_charcard))
```

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test -p shirita-web import_export`
Expected: PASS (both import tests).

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/import_export.rs shirita-web/src/routes/mod.rs shirita-web/src/lib.rs shirita-core/src/lib.rs shirita-web/tests/import_export_test.rs
git commit -m "feat(web): /api/import/{worldinfo,charcard} endpoints"
```

> **Export endpoints** (`GET /api/export/charcard/{def_id}`, `/export/worldinfo`, `/export/preset/{template_id}`) follow the same shape: load from storage, call `def_to_charcard`/`defs_to_worldinfo`/`tree_to_preset`, return JSON. Add them in a follow-up step once import is proven; they're pure reads with no new adapter logic. The frontend Import/Export buttons in `DefinitionEditor`/`BookView` (currently disabled stubs) wire to these — a thin UI task that can ride along or be its own small plan.

---

## Task 7: Full verification

- [ ] **Step 1:** `cargo test` → green (core adapters + web import).
- [ ] **Step 2:** `grep -rn "v-html" shirita-ui/src` → none (imports are data only; any future preview must render as text).
- [ ] **Step 3: Manual smoke.** POST a real exported ST lorebook JSON to `/api/import/worldinfo` and confirm the world definitions appear in `/book`.

---

## Self-review checklist

- **Spec coverage (§11):** World Info import both shapes + export with ST field names `key`/`keysecondary`/`order`/`disable`/`probability`/`useProbability` (T1–T2) ✓ · Character Card V2/V3 import (name/description + character_book) + export with embedded book (T3–T4) ✓ · preset export of tree (T5) ✓ · web import endpoints (T6) ✓ · pure-JSON, no DB-only fields leaked; types referenced by stable english id (`world`/`char`) (all) ✓. **Deferred (noted):** export endpoints + UI wiring (Task 6 note), `@depth` positions, group/asset fields, preset import-to-storage (Task 6 note).
- **Placeholder scan:** real JSON fixtures + conversion code in every step.
- **Type consistency:** `worldinfo_to_defs`/`defs_to_worldinfo`/`charcard_to_defs`/`def_to_charcard`/`tree_to_preset`, all producing `Definition` (type strings `world`/`char` from Plan 2) with `meta.trigger` matching Plan 1's `parse_trigger` shape — names + shapes identical across tasks and aligned with `assembly::parse_trigger`.
- **Security (§14):** adapters are pure data transforms; imported content stored as text, never rendered via `v-html`; web layer validates JSON via `serde_json`.
- **Caveat for executor:** ST formats drift across versions — if a real-world card/lorebook fails a field mapping, prefer leniency (default + passthrough via `extensions`/`st`) over rejecting the import.
```
