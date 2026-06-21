# Pack Zip Bundle — Plan 4: Zip import (atomic, safe, deduped) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Teach `POST /api/import` to accept a `shirita.pack` **zip** (manifest + `assets/`) and a binary-less `shirita.pack` **JSON**. Imports restore referenced assets with **content-hash dedup**, **rewrite** the manifest's designated asset fields to the freshly stored names (blanking any ref absent from the bundle so no dead links survive), and create the pack + its definitions + its nodes **atomically in one DB transaction** — any failure rolls the whole import back. Zip extraction is **hardened** against path traversal and zip bombs.

**Architecture:** Two layers.
1. **Core (`shirita-core`)** gains one atomic storage method `Storage::import_pack(pack, defs, nodes, assets)` — a single `pool.begin()` transaction (mirrors the existing `reorder_sessions` tx) that inserts new asset rows, the pack, its definitions, then its (caller pre-ordered, parent-before-child) nodes, and commits. This is the *only* way to get true all-or-nothing semantics, because the per-entity `create_*` methods each commit independently.
2. **Web (`shirita-web`)** adds a safe `unzip_pack` extractor + a `persist_pack_bundle` restorer to `import_export.rs`, and wires both a PK-magic sniff and a `shirita.pack` JSON arm into the existing `import` handler. `persist_pack_bundle` reuses the Plan-2 codec (`collect_pack_assets` / `rewrite_pack_assets` / `parse_portable`) and the `import_template_bundle` topological-insert precedent, then hands the fully-built entities to `import_pack`.

**Tech Stack:** Rust, Axum 0.8, `zip` 2.x (`ZipArchive` read), sqlx sqlite transaction, `tokio::test` integration tests via `app().oneshot()`, core `#[tokio::test]` via `temp_storage()`.

## Global Constraints

- **Atomicity (spec §7):** packs + definitions + prompt_nodes (+ new asset rows) all write inside one transaction; any error → full rollback. No partial pack ever lands.
- **Archive safety (spec §7):** reject entries the `zip` crate flags as unsafe (`enclosed_name()` is `None` → `..`/absolute), reject nested `assets/<dir>/...` entries, and cap entry count + per-entry + total decompressed bytes (zip-bomb guard). Reject a bundle with no `manifest.json`.
- **Dedup (spec §5):** each restored asset is keyed by sha256; an existing row with that hash (or one queued earlier in the same import) is reused — no duplicate file, no duplicate row.
- **Dead-link guard (spec §7):** a designated asset ref present in the manifest but absent from the zip is blanked by `rewrite_pack_assets` (avatar → `null`, panel `/assets/…` stripped). Only refs that are **both** designated **and** present in the zip are restored.
- **Deterministic markers only (spec §4):** asset discovery goes through `collect_pack_assets` — never scan arbitrary strings.
- **Never delete existing entities:** like `import_template_bundle`, a pack is an atomic unit; on `Skip` an existing same-name pack short-circuits; otherwise a fresh pack (new UUID) is always created. Existing packs are never overwritten/deleted.
- Comments/commits in English. Tests: `cargo test -p shirita-core --lib storage::sqlite` (the two new core tests) and `cargo test -p shirita-web --test pack_import_test`.

---

## File Structure

- `shirita-core/src/storage/mod.rs` — declare `Storage::import_pack` in the packs section.
- `shirita-core/src/storage/sqlite.rs` — implement `import_pack` (single tx) + two atomicity/happy-path tests.
- `shirita-web/src/routes/import_export.rs` — add `unzip_pack` + `persist_pack_bundle`, wire PK sniff + `shirita.pack` JSON arm into `import`.
- `shirita-web/tests/pack_import_test.rs` — new integration tests (round-trip, dedup, safety, degraded JSON).

---

### Task 1: Atomic `Storage::import_pack`

**Files:**
- Modify: `shirita-core/src/storage/mod.rs`
- Modify: `shirita-core/src/storage/sqlite.rs` (impl + `mod tests`)

**Interfaces:**
- Consumes: `Pack`, `Definition`, `PromptNode`, `Asset` (already in scope in sqlite.rs).
- Produces: `Storage::import_pack(&self, pack: &Pack, defs: &[Definition], nodes: &[PromptNode], assets: &[Asset]) -> Result<()>`.

- [ ] **Step 1: Write the failing core tests**

In `shirita-core/src/storage/sqlite.rs`, inside `mod tests` (after an existing `#[tokio::test]`), add:

```rust
    #[tokio::test]
    async fn import_pack_persists_all_in_one_shot() {
        let s = temp_storage().await;
        let pack = Pack::new("Whole");
        let def = Definition::new("world", "D", "c");
        let node = PromptNode::new_ref(OwnerKind::Pack, &pack.id, None, 0, &def.id);
        let mut asset = Asset::new("a", "stored.png");
        asset.hash = Some("deadbeef".into());
        s.import_pack(&pack, &[def.clone()], std::slice::from_ref(&node), std::slice::from_ref(&asset))
            .await
            .unwrap();
        assert!(s.get_pack(&pack.id).await.unwrap().is_some());
        assert!(s.get_definition(&def.id).await.unwrap().is_some());
        assert_eq!(s.list_nodes(&OwnerKind::Pack, &pack.id).await.unwrap().len(), 1);
        assert!(s.find_asset_by_hash("deadbeef").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn import_pack_rolls_back_on_failure() {
        let s = temp_storage().await;
        let pack = Pack::new("Atomic");
        let def = Definition::new("world", "D", "c");
        let n1 = PromptNode::new_ref(OwnerKind::Pack, &pack.id, None, 0, &def.id);
        let mut n2 = PromptNode::new_ref(OwnerKind::Pack, &pack.id, None, 1, &def.id);
        n2.id = n1.id.clone(); // duplicate primary key → mid-transaction failure
        let res = s.import_pack(&pack, &[def.clone()], &[n1, n2], &[]).await;
        assert!(res.is_err());
        // Full rollback: pack, definition and nodes are all absent.
        assert!(s.get_pack(&pack.id).await.unwrap().is_none());
        assert!(s.get_definition(&def.id).await.unwrap().is_none());
        assert!(s.list_nodes(&OwnerKind::Pack, &pack.id).await.unwrap().is_empty());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-core --lib storage::sqlite::tests::import_pack 2>&1 | tail -20`
Expected: FAIL to compile — `import_pack` is not a method on `Storage` yet.

- [ ] **Step 3: Declare the trait method**

In `shirita-core/src/storage/mod.rs`, in the `// --- packs ---` block (after `delete_pack`), add:

```rust
    /// Atomically persist an imported pack bundle in a single transaction:
    /// new asset rows, the pack, its definitions, then its nodes (which MUST be
    /// pre-ordered parent-before-child). Any failure rolls the whole import back.
    async fn import_pack(
        &self,
        pack: &Pack,
        defs: &[Definition],
        nodes: &[PromptNode],
        assets: &[Asset],
    ) -> Result<()>;
```

- [ ] **Step 4: Implement it on `SqliteStorage`**

In `shirita-core/src/storage/sqlite.rs`, after `delete_pack` (the packs section), add:

```rust
    async fn import_pack(
        &self,
        pack: &Pack,
        defs: &[Definition],
        nodes: &[PromptNode],
        assets: &[Asset],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        // New asset rows (deduped/reused assets are not in this list).
        for a in assets {
            sqlx::query("INSERT INTO assets (id, name, path, kind, created_at, hash) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(&a.id).bind(&a.name).bind(&a.path).bind(&a.kind).bind(&a.created_at).bind(&a.hash)
                .execute(&mut *tx).await?;
        }
        // Pack.
        let identity = serde_json::to_string(&pack.identity)?;
        let meta = serde_json::to_string(&pack.meta)?;
        sqlx::query("INSERT INTO packs (id, name, identity_json, meta, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&pack.id).bind(&pack.name).bind(identity).bind(meta).bind(&pack.created_at).bind(&pack.updated_at)
            .execute(&mut *tx).await?;
        // Definitions.
        for d in defs {
            let dmeta = serde_json::to_string(&d.meta)?;
            sqlx::query("INSERT INTO definitions (id, type, name, content, meta) VALUES (?, ?, ?, ?, ?)")
                .bind(&d.id).bind(d.def_type.as_str()).bind(&d.name).bind(&d.content).bind(dmeta)
                .execute(&mut *tx).await?;
        }
        // Nodes — caller guarantees parent-before-child order (self-referential FK).
        for n in nodes {
            let nmeta = serde_json::to_string(&n.meta)?;
            sqlx::query("INSERT INTO prompt_nodes (id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at, meta) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(&n.id).bind(n.owner_kind.as_str()).bind(&n.owner_id).bind(&n.parent_id).bind(n.sort_order)
                .bind(n.kind.as_str()).bind(&n.tag).bind(&n.definition_id).bind(n.enabled as i64).bind(&n.created_at).bind(nmeta)
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p shirita-core --lib storage::sqlite::tests::import_pack 2>&1 | tail -20`
Expected: PASS — both the happy-path persist and the duplicate-id rollback assertions.

- [ ] **Step 6: Commit**

```bash
cargo build --workspace 2>&1 | tail -4
git add shirita-core/src/storage/mod.rs shirita-core/src/storage/sqlite.rs
git commit -m "feat(core): Storage::import_pack — atomic single-tx pack restore"
```

---

### Task 2: Safe zip import + restore, wired into `/api/import`

**Files:**
- Modify: `shirita-web/src/routes/import_export.rs`
- Test: `shirita-web/tests/pack_import_test.rs`

**Interfaces:**
- Consumes: `unzip_pack(&[u8]) -> Result<(Value, HashMap<String, Vec<u8>>), StatusCode>`; `shirita_core::{collect_pack_assets, rewrite_pack_assets, parse_portable, PortableDoc, sha256_hex, Asset, Pack}`; `Storage::{list_packs, find_asset_by_hash, import_pack}`; `config.assets_dir`.
- Produces: `POST /api/import` accepting a `shirita.pack` zip (PK magic) and a binary-less `shirita.pack` JSON, both summarized with a `"pack"` created item.

- [ ] **Step 1: Write the failing integration tests**

Create `shirita-web/tests/pack_import_test.rs`:

```rust
//! POST /api/import — shirita.pack zip bundles: round-trip, dedup, safety, degraded JSON.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, Pack, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state(dir: &std::path::Path) -> AppState {
    let storage = SqliteStorage::connect(dir.join("p.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", dir.join("assets").to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

async fn export_zip(state: &AppState, pack_id: &str) -> (HeaderMap, Vec<u8>) {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/packs/{pack_id}/export"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::empty())
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let headers = res.headers().clone();
    (headers, res.into_body().collect().await.unwrap().to_bytes().to_vec())
}

async fn import_bytes(state: &AppState, query: &str, data: &[u8]) -> (StatusCode, Value) {
    let boundary = "BND";
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"p.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    ).as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/import{query}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Build an in-memory zip: manifest.json + the given (entry-name, bytes) pairs.
fn make_zip(manifest: &Value, entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cur);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("manifest.json", opts).unwrap();
        zw.write_all(&serde_json::to_vec(manifest).unwrap()).unwrap();
        for (name, data) in entries {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    cur.into_inner()
}

fn created_pack_id(summary: &Value) -> String {
    summary["created"].as_array().unwrap().iter()
        .find(|c| c["kind"] == "pack").expect("a pack was created")["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn import_pack_zip_round_trips_and_restores_asset() {
    // Source instance: a pack with an avatar → export a zip.
    let src = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(src.path().join("assets")).unwrap();
    std::fs::write(src.path().join("assets/face.png"), b"img-bytes").unwrap();
    let s1 = test_state(src.path()).await;
    let mut pack = Pack::new("Alice");
    pack.identity.avatar = Some("face.png".into());
    s1.storage.create_pack(&pack).await.unwrap();
    let (_, zip_bytes) = export_zip(&s1, &pack.id).await;
    assert_eq!(&zip_bytes[..2], b"PK");

    // Destination instance: import the zip plug-and-play.
    let dst = tempfile::tempdir().unwrap();
    let s2 = test_state(dst.path()).await;
    let (st, summary) = import_bytes(&s2, "", &zip_bytes).await;
    assert_eq!(st, StatusCode::OK);
    let new_id = created_pack_id(&summary);

    // The imported pack's avatar resolves to a real, restored file with the same bytes.
    let got = s2.storage.get_pack(&new_id).await.unwrap().unwrap();
    let avatar = got.identity.avatar.expect("avatar present after import");
    let restored = dst.path().join("assets").join(&avatar);
    assert!(restored.exists(), "avatar file restored");
    assert_eq!(std::fs::read(&restored).unwrap(), b"img-bytes");
}

#[tokio::test]
async fn import_pack_zip_dedups_asset_by_hash() {
    let src = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(src.path().join("assets")).unwrap();
    std::fs::write(src.path().join("assets/face.png"), b"same-bytes").unwrap();
    let s1 = test_state(src.path()).await;
    let mut pack = Pack::new("Alice");
    pack.identity.avatar = Some("face.png".into());
    s1.storage.create_pack(&pack).await.unwrap();
    let (_, zip_bytes) = export_zip(&s1, &pack.id).await;

    // Import the same bundle twice (duplicate so the 2nd isn't skipped by name).
    let dst = tempfile::tempdir().unwrap();
    let s2 = test_state(dst.path()).await;
    let (_, a) = import_bytes(&s2, "?on_conflict=duplicate", &zip_bytes).await;
    let (_, b) = import_bytes(&s2, "?on_conflict=duplicate", &zip_bytes).await;
    let pa = s2.storage.get_pack(&created_pack_id(&a)).await.unwrap().unwrap();
    let pb = s2.storage.get_pack(&created_pack_id(&b)).await.unwrap().unwrap();
    // Same content hash → both packs point at the SAME stored file (deduped).
    assert_eq!(pa.identity.avatar, pb.identity.avatar);
    let assets = s2.storage.list_assets(None).await.unwrap();
    assert_eq!(assets.len(), 1, "asset registered once, reused on the 2nd import");
}

#[tokio::test]
async fn import_pack_zip_rejects_nested_asset_entry() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let manifest = serde_json::json!({
        "format": "shirita.pack",
        "pack": { "name": "Evil", "identity": {}, "meta": {} },
        "nodes": [], "definitions": []
    });
    let zip = make_zip(&manifest, &[("assets/sub/evil.png", b"x")]);
    let (st, _) = import_bytes(&state, "", &zip).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn import_pack_zip_rejects_missing_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    // A zip with only an asset, no manifest.json.
    use std::io::Write;
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cur);
        zw.start_file("assets/face.png", zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(b"img").unwrap();
        zw.finish().unwrap();
    }
    let (st, _) = import_bytes(&state, "", &cur.into_inner()).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn import_pack_binary_less_json() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let manifest = serde_json::json!({
        "format": "shirita.pack",
        "pack": { "name": "NoBin", "identity": {}, "meta": {} },
        "nodes": [], "definitions": []
    });
    let (st, summary) = import_bytes(&state, "", &serde_json::to_vec(&manifest).unwrap()).await;
    assert_eq!(st, StatusCode::OK);
    assert!(summary["created"].as_array().unwrap().iter().any(|c| c["kind"] == "pack"));
    assert!(state.storage.list_packs().await.unwrap().iter().any(|p| p.name == "NoBin"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-web --test pack_import_test 2>&1 | tail -25`
Expected: FAIL — the zip imports return `BAD_REQUEST` (no PK branch yet) so `created_pack_id` panics / the JSON arm 400s. (`make_zip`/`export_zip` resolve against the existing `zip` dep + Plan-3 endpoint.)

- [ ] **Step 3: Add the safe extractor + restorer**

In `shirita-web/src/routes/import_export.rs`, add the zip size/entry caps and the two helpers. Place them above the `import` handler.

```rust
const MAX_ZIP_ENTRIES: usize = 512;
const MAX_ENTRY_BYTES: u64 = 32 * 1024 * 1024; // 32 MiB per file
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB total decompressed

/// Safely unpack a `shirita.pack` zip into (manifest, `assets/<rel>` → bytes).
/// Rejects unsafe paths (`..`/absolute via `enclosed_name`), nested `assets/`
/// entries, and over-cap entry counts / per-entry / total decompressed sizes.
fn unzip_pack(bytes: &[u8]) -> Result<(Value, HashMap<String, Vec<u8>>), StatusCode> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| StatusCode::BAD_REQUEST)?;
    if zip.len() > MAX_ZIP_ENTRIES {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut manifest: Option<Value> = None;
    let mut assets: HashMap<String, Vec<u8>> = HashMap::new();
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|_| StatusCode::BAD_REQUEST)?;
        // `enclosed_name` returns None for traversal/absolute paths.
        let name = match entry.enclosed_name() {
            Some(p) => p.to_string_lossy().replace('\\', "/"),
            None => return Err(StatusCode::BAD_REQUEST),
        };
        let is_dir = entry.is_dir();
        let declared = entry.size();
        if is_dir {
            continue;
        }
        if declared > MAX_ENTRY_BYTES {
            return Err(StatusCode::BAD_REQUEST);
        }
        // Read with a hard cap — the declared size can lie.
        let mut buf = Vec::new();
        entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut buf).map_err(|_| StatusCode::BAD_REQUEST)?;
        if buf.len() as u64 > MAX_ENTRY_BYTES {
            return Err(StatusCode::BAD_REQUEST);
        }
        total += buf.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(StatusCode::BAD_REQUEST);
        }
        if name == "manifest.json" {
            manifest = Some(serde_json::from_slice(&buf).map_err(|_| StatusCode::BAD_REQUEST)?);
        } else if let Some(rel) = name.strip_prefix("assets/") {
            // Flat names only — no nested directories under assets/.
            if rel.is_empty() || rel.contains('/') {
                return Err(StatusCode::BAD_REQUEST);
            }
            assets.insert(rel.to_string(), buf);
        }
        // Any other top-level entry is ignored.
    }
    let manifest = manifest.ok_or(StatusCode::BAD_REQUEST)?;
    Ok((manifest, assets))
}

/// Restore a `shirita.pack` manifest + its bundled asset bytes: hash-dedup each
/// referenced asset (reuse an existing/just-queued row by content hash, else
/// write the file + register a new Asset), rewrite the manifest's designated
/// asset fields to the stored names (blanking refs absent from the bundle), then
/// atomically create the pack, its definitions and its nodes.
async fn persist_pack_bundle(
    state: &AppState,
    manifest: &Value,
    zip_assets: &HashMap<String, Vec<u8>>,
    oc: OnConflict,
    summary: &mut ImportSummary,
) -> Result<(), StatusCode> {
    use std::path::Path as FsPath;

    // Skip an existing same-name pack (peek before the full parse/restore).
    let name = manifest.get("pack").and_then(|p| p.get("name")).and_then(|v| v.as_str()).unwrap_or("Pack").to_string();
    if matches!(oc, OnConflict::Skip) {
        let packs = state.storage.list_packs().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(ex) = packs.iter().find(|p| p.name == name) {
            summary.skipped.push(item("pack", &ex.id, &ex.name));
            return Ok(());
        }
    }

    // 1) Hash-dedup restore. Only assets BOTH designated AND present in the zip
    //    are restored; the old→new map drives the rewrite (missing → blanked).
    tokio::fs::create_dir_all(&state.config.assets_dir).await.ok();
    let mut rename: HashMap<String, String> = HashMap::new();
    let mut new_assets: Vec<shirita_core::Asset> = Vec::new();
    let mut by_hash: HashMap<String, String> = HashMap::new(); // in-batch dedup
    for rel in shirita_core::collect_pack_assets(manifest) {
        let Some(bytes) = zip_assets.get(&rel) else { continue };
        let hash = shirita_core::sha256_hex(bytes);
        let stored = if let Some(p) = by_hash.get(&hash) {
            p.clone()
        } else if let Some(ex) =
            state.storage.find_asset_by_hash(&hash).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        {
            ex.path
        } else {
            let ext = FsPath::new(&rel).extension().and_then(|e| e.to_str()).unwrap_or("bin");
            let stored = format!("{}.{}", uuid::Uuid::new_v4(), ext);
            tokio::fs::write(FsPath::new(&state.config.assets_dir).join(&stored), bytes)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut a = shirita_core::Asset::new(&rel, stored.clone());
            a.kind = "avatar".into();
            a.hash = Some(hash.clone());
            new_assets.push(a);
            stored
        };
        by_hash.insert(hash, stored.clone());
        rename.insert(rel, stored);
    }

    // 2) Rewrite designated refs to stored names (unmapped → blanked).
    let rewritten = shirita_core::rewrite_pack_assets(manifest, &rename);

    // 3) Parse to a portable pack; build a fresh pack (new UUID) + entities.
    let (pname, identity, meta, pnodes, pdefs) =
        match shirita_core::parse_portable(&rewritten).map_err(|_| StatusCode::BAD_REQUEST)? {
            shirita_core::PortableDoc::Pack { name, identity, meta, nodes, defs } => (name, identity, meta, nodes, defs),
            _ => return Err(StatusCode::BAD_REQUEST),
        };
    let mut pack = shirita_core::Pack::new(&pname);
    pack.identity = identity;
    pack.meta = meta;

    // Definitions: local_id → new id (bundle defs created fresh, like template import).
    let mut def_map: HashMap<String, String> = HashMap::new();
    let mut out_defs: Vec<Definition> = Vec::new();
    for pd in &pdefs {
        let mut d = Definition::new(&pd.def_type, &pd.name, &pd.content);
        d.meta = pd.meta.clone();
        def_map.insert(pd.local_id.clone(), d.id.clone());
        out_defs.push(d);
    }

    // Nodes: pre-allocate new ids, then emit in parent-before-child order
    // (mirrors import_template_bundle's topological layering).
    let node_map: HashMap<String, String> =
        pnodes.iter().map(|n| (n.local_id.clone(), uuid::Uuid::new_v4().to_string())).collect();
    let mut out_nodes: Vec<PromptNode> = Vec::new();
    let mut inserted: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut remaining: Vec<&shirita_core::PortableNode> = pnodes.iter().collect();
    loop {
        let mut progressed = false;
        let mut still: Vec<&shirita_core::PortableNode> = Vec::new();
        for pn in remaining {
            let parent_pending = match &pn.parent_local_id {
                Some(p) => node_map.contains_key(p) && !inserted.contains(p),
                None => false,
            };
            if parent_pending {
                still.push(pn);
                continue;
            }
            let definition_id = match (&pn.kind, &pn.def_local_id) {
                (NodeKind::Ref, Some(dl)) => match def_map.get(dl) {
                    Some(real) => Some(real.clone()),
                    None => {
                        tracing::warn!(local_id = %pn.local_id, "pack import: ref def_local_id missing, skipping node");
                        inserted.insert(pn.local_id.clone());
                        progressed = true;
                        continue;
                    }
                },
                _ => None,
            };
            out_nodes.push(PromptNode {
                id: node_map[&pn.local_id].clone(),
                owner_kind: OwnerKind::Pack,
                owner_id: pack.id.clone(),
                parent_id: pn.parent_local_id.as_ref().and_then(|p| node_map.get(p)).cloned(),
                sort_order: pn.sort_order,
                kind: pn.kind.clone(),
                tag: pn.tag.clone(),
                definition_id,
                enabled: pn.enabled,
                created_at: chrono::Utc::now().to_rfc3339(),
                meta: pn.meta.clone(),
            });
            inserted.insert(pn.local_id.clone());
            progressed = true;
        }
        remaining = still;
        if remaining.is_empty() || !progressed {
            break;
        }
    }

    // 4) One transaction: assets + pack + defs + nodes (full rollback on any error).
    state
        .storage
        .import_pack(&pack, &out_defs, &out_nodes, &new_assets)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    summary.created.push(item("pack", &pack.id, &pack.name));
    Ok(())
}
```

- [ ] **Step 4: Wire both entry points into `import`**

In `shirita-web/src/routes/import_export.rs`, inside `import`, add the PK sniff right after the PNG branch (before the `let v: Value = serde_json::from_slice…` JSON parse):

```rust
    // 1b) Zip → shirita.pack bundle (manifest.json + assets/<file>).
    if bytes.len() >= 4 && bytes[..4] == [0x50, 0x4B, 0x03, 0x04] {
        let (manifest, zip_assets) = unzip_pack(&bytes)?;
        persist_pack_bundle(&state, &manifest, &zip_assets, oc, &mut summary).await?;
        return Ok(Json(summary));
    }
```

Then in the JSON `match v.get("format")…` block, add an arm next to the existing `shirita.template` arm:

```rust
        Some("shirita.pack") => {
            persist_pack_bundle(&state, &v, &HashMap::new(), oc, &mut summary).await?;
        }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p shirita-web --test pack_import_test 2>&1 | tail -25`
Expected: PASS — round-trip restores the avatar bytes; the second import dedups (one asset row, same stored name); nested-entry and missing-manifest zips 400; the binary-less JSON imports the pack.

- [ ] **Step 6: Full sweep + commit**

```bash
cargo test -p shirita-web 2>&1 | tail -8
cargo build --workspace 2>&1 | tail -4
git add shirita-web/src/routes/import_export.rs shirita-web/tests/pack_import_test.rs
git commit -m "feat(web): POST /import accepts shirita.pack zip + json (safe, deduped, atomic)"
```

---

## Final Verification

- [ ] **Core + web test + build sweep**

Run: `cargo test -p shirita-core --lib storage::sqlite::tests::import_pack 2>&1 | tail -6 && cargo test -p shirita-web --test pack_import_test 2>&1 | tail -8 && cargo build --workspace 2>&1 | tail -4`
Expected: 2 core import_pack tests pass; 5 web pack_import tests pass; workspace builds clean. No regression: `cargo test -p shirita-web --test import_test 2>&1 | tail -4` still green (the existing import paths are untouched).

---

## Self-Review

**Spec coverage:**
- Atomic DB write w/ full rollback (§7) — Task 1 `import_pack` single tx; asserted by `import_pack_rolls_back_on_failure`.
- PK sniff + JSON degradation (§3, §7) — Task 2 Step 4 (PK branch + `shirita.pack` JSON arm); asserted by round-trip + `import_pack_binary_less_json`.
- Archive safety: traversal/absolute via `enclosed_name`, nested-entry + missing-manifest rejection, entry-count + per-entry + total-size caps (§7) — `unzip_pack`; asserted by `rejects_nested_asset_entry` + `rejects_missing_manifest` (traversal/size caps are reject-paths exercised by the same guard).
- Hash dedup restore (§5) — `persist_pack_bundle` `by_hash` + `find_asset_by_hash`; asserted by `import_pack_zip_dedups_asset_by_hash` (one asset row across two imports).
- Dead-link blanking of unmapped refs (§7) — `rewrite_pack_assets` over the `rename` map built only from present-in-zip refs (Plan 2 behavior).
- Deterministic markers only (§4) — restore iterates `collect_pack_assets`, never arbitrary strings.

**Placeholder scan:** none — full method bodies, exact SQL (mirrors `create_asset`/`create_pack`/`create_definition`/`create_node`), complete handlers, complete test file, exact commands.

**Type consistency:** `import_pack(&Pack, &[Definition], &[PromptNode], &[Asset]) -> Result<()>` matches trait + impl + both call sites (core test passes slices; `persist_pack_bundle` passes `&pack, &out_defs, &out_nodes, &new_assets`). `unzip_pack` returns `(Value, HashMap<String, Vec<u8>>)` consumed by `persist_pack_bundle`. `collect_pack_assets(&Value) -> Vec<String>`, `rewrite_pack_assets(&Value, &HashMap<String,String>) -> Value`, `parse_portable(&Value) -> Result<PortableDoc>`, `PortableDoc::Pack{name,identity,meta,nodes,defs}`, `sha256_hex(&[u8]) -> String`, `Asset::new(name,path)` + `.kind`/`.hash`, `Pack::new(name)` + `.identity`/`.meta` are the existing Plan-1/Plan-2 + model signatures. `NodeKind`, `OwnerKind`, `PromptNode`, `Definition` are already imported in `import_export.rs`; `Pack`/`Asset`/codec fns are reached via `shirita_core::` (no new import line). Topological loop mirrors the proven `import_template_bundle`.

**Risk notes:**
- `zip` 2.x API surface: `ZipArchive::by_index`, `ZipFile::{enclosed_name, is_dir, size}`, `Read::take` on `ZipFile`, `ZipWriter`/`SimpleFileOptions` (test). If a minor name differs in the resolved 2.x, adjust at the call site — read (`unzip_pack`) and write (`make_zip`/Plan-3 export) must agree. `enclosed_name()` returns `Option<PathBuf>` (owned) in 2.x; converted to an owned `String` before `.take()` moves the entry, so no borrow conflict.
- A Ref node whose def is missing is skipped (warn), mirroring template import; a child parented to a skipped node is rare (Refs are leaves). Wrapped in the transaction, any resulting dangling parent FK rolls the whole import back rather than landing a partial pack — strictly safer than the pre-tx template path.
- Asset `kind` defaults to `"avatar"` for restored bundle assets (matches `save_png_asset`); imported pack assets are referenced by stored path, not picked from the library by `kind`, so this is cosmetic for the asset library filter only.
