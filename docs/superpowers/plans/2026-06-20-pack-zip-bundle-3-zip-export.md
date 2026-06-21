# Pack Zip Bundle — Plan 3: Zip export endpoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `GET /api/packs/{id}/export` — stream a `.zip` bundle (`manifest.json` + `assets/<file>`) when the pack references binary, or a plain `.json` manifest when it doesn't. The filename (`.zip` / `.json`) rides in `Content-Disposition` so the frontend (Plan 5) names the download correctly.

**Architecture:** Mirror the existing `export_template` handler (get entity → `list_nodes(Pack)` → `list_definitions` map → core codec), but feed the manifest to `collect_pack_assets` (Plan 2), read each existing asset file from `assets_dir`, and — if any exist — build an in-memory zip with the `zip` crate; otherwise return `Json(manifest)`. Pure packing logic already lives in core (Plan 2); this is the web glue + filesystem read + zip container.

**Tech Stack:** Rust, Axum 0.8, `zip` crate (in-memory `ZipWriter`/`ZipArchive`), `tokio::test` integration tests via `app().oneshot()`.

## Global Constraints

- Zip layout exactly `manifest.json` + `assets/<rel>` (rel = the pack's stored relative filenames from `collect_pack_assets`).
- A collected asset whose file is **missing** on disk is skipped + warned (its manifest ref will be blanked at import by `rewrite_pack_assets`). If **no** asset file ends up bundled → degrade to JSON.
- Response: zip → `Content-Type: application/zip` + `Content-Disposition: attachment; filename="<name>.zip"`; json → `Json` (its own `application/json`) + `filename="<name>.json"`. `<name>` via the existing `safe_filename`.
- Comments/commits in English. Test: `cargo test -p shirita-web --test pack_export_test`.

---

## File Structure

- `shirita-web/Cargo.toml` — add `zip`.
- `shirita-web/src/routes/export.rs` — add the `export_pack` handler.
- `shirita-web/src/lib.rs` — register the route.
- `shirita-web/tests/pack_export_test.rs` — new integration tests.

---

### Task 1: `GET /api/packs/{id}/export`

**Files:**
- Modify: `shirita-web/Cargo.toml`
- Modify: `shirita-web/src/routes/export.rs`
- Modify: `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/pack_export_test.rs`

**Interfaces:**
- Consumes: `shirita_core::{export_pack, collect_pack_assets}` (Plan 2), `Storage::{get_pack, list_nodes, list_definitions}`, `config.assets_dir`.
- Produces: route `GET /api/packs/{id}/export` returning a zip (binary, `PK…`) or a `shirita.pack` JSON.

- [ ] **Step 1: Add the `zip` dependency**

In `shirita-web/Cargo.toml`, under `[dependencies]`, add:

```toml
zip = "2"
```

- [ ] **Step 2: Write the failing integration tests**

Create `shirita-web/tests/pack_export_test.rs`:

```rust
//! GET /api/packs/{id}/export — zip when the pack has binary, JSON otherwise.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
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

async fn get(state: &AppState, uri: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::empty())
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, body)
}

#[tokio::test]
async fn export_pack_with_avatar_is_a_zip_bundle() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/face.png"), b"img-bytes").unwrap();
    let state = test_state(dir.path()).await;

    let mut pack = Pack::new("Alice");
    pack.identity.avatar = Some("face.png".into());
    state.storage.create_pack(&pack).await.unwrap();

    let (st, headers, body) = get(&state, &format!("/api/packs/{}/export", pack.id)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/zip");
    assert!(headers.get(header::CONTENT_DISPOSITION).unwrap().to_str().unwrap().ends_with(".zip\""));
    assert_eq!(&body[..2], b"PK"); // zip local-file magic

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(body)).unwrap();
    let names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().to_string()).collect();
    assert!(names.contains(&"manifest.json".to_string()));
    assert!(names.contains(&"assets/face.png".to_string()));
}

#[tokio::test]
async fn export_pack_without_binary_is_json() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    let state = test_state(dir.path()).await;

    let pack = Pack::new("Bob"); // no avatar, no panel
    state.storage.create_pack(&pack).await.unwrap();

    let (st, headers, body) = get(&state, &format!("/api/packs/{}/export", pack.id)).await;
    assert_eq!(st, StatusCode::OK);
    assert!(headers.get(header::CONTENT_TYPE).unwrap().to_str().unwrap().contains("application/json"));
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["format"], "shirita.pack");
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p shirita-web --test pack_export_test 2>&1 | tail -20`
Expected: FAIL — `GET /api/packs/{id}/export` returns 404 (route absent). (If `zip` isn't resolvable yet, the test file won't compile — that's also "fail".)

- [ ] **Step 4: Add the `export_pack` handler**

In `shirita-web/src/routes/export.rs`, add these imports at the top (next to the existing ones):

```rust
use std::io::{Cursor, Write};
use std::path::Path as FsPath;

use axum::response::Response;
```

Then add the handler (after `export_template`):

```rust
/// GET /api/packs/{id}/export — a zip bundle (manifest.json + assets/<file>)
/// when the pack references binary, else a plain `shirita.pack` JSON. The
/// download filename (.zip / .json) is set in Content-Disposition.
pub async fn export_pack(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    let pack = state
        .storage
        .get_pack(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let nodes = state
        .storage
        .list_nodes(&OwnerKind::Pack, &id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let defs: HashMap<String, Definition> = all.into_iter().map(|d| (d.id.clone(), d)).collect();
    let manifest = shirita_core::export_pack(&pack, &nodes, &defs);
    let base = safe_filename(&pack.name);

    // Read every referenced asset that exists on disk; missing ones are skipped
    // (their manifest ref is blanked at import by rewrite_pack_assets).
    let mut assets: Vec<(String, Vec<u8>)> = Vec::new();
    for rel in shirita_core::collect_pack_assets(&manifest) {
        let path = FsPath::new(&state.config.assets_dir).join(&rel);
        match tokio::fs::read(&path).await {
            Ok(bytes) => assets.push((rel, bytes)),
            Err(_) => tracing::warn!(rel = %rel, "export_pack: asset file missing, skipping"),
        }
    }

    // No binary → plain JSON.
    if assets.is_empty() {
        let cd = format!("attachment; filename=\"{base}.json\"");
        return Ok(([(header::CONTENT_DISPOSITION, cd)], Json(manifest)).into_response());
    }

    // Build the zip in memory: manifest.json + assets/<rel>.
    let mut cursor = Cursor::new(Vec::<u8>::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cursor);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let manifest_bytes =
            serde_json::to_vec(&manifest).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        zw.start_file("manifest.json", opts).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        zw.write_all(&manifest_bytes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for (rel, bytes) in &assets {
            zw.start_file(format!("assets/{rel}"), opts).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            zw.write_all(bytes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        zw.finish().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let bytes = cursor.into_inner();
    let cd = format!("attachment; filename=\"{base}.zip\"");
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, cd),
        ],
        bytes,
    )
        .into_response())
}
```

- [ ] **Step 5: Register the route**

In `shirita-web/src/lib.rs`, after the line

```rust
        .route("/packs/{id}/duplicate", post(routes::packs::duplicate))
```

add:

```rust
        .route("/packs/{id}/export", get(routes::export::export_pack))
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p shirita-web --test pack_export_test 2>&1 | tail -20`
Expected: PASS — the avatar pack exports a zip whose entries are `manifest.json` + `assets/face.png`; the binary-less pack exports `shirita.pack` JSON.

- [ ] **Step 7: Build the workspace + commit**

```bash
cargo build --workspace 2>&1 | tail -4
git add shirita-web/Cargo.toml Cargo.lock shirita-web/src/routes/export.rs shirita-web/src/lib.rs shirita-web/tests/pack_export_test.rs
git commit -m "feat(web): GET /packs/{id}/export — zip bundle (manifest + assets) or json"
```

---

## Final Verification

- [ ] **Web test + build sweep**

Run: `cargo test -p shirita-web --test pack_export_test 2>&1 | tail -6 && cargo build --workspace 2>&1 | tail -4`
Expected: both export tests pass; workspace builds clean.

---

## Self-Review

**Spec coverage (spec §3, §6):**
- Container `manifest.json` + `assets/<file>` — Task 1 (zip writer); asserted by the unzip test.
- Asset collection via the deterministic `collect_pack_assets` (Plan 2) — Task 1.
- Missing-file → skip + warn (ref blanked at import) — Task 1.
- JSON degradation when no binary — Task 1 (`assets.is_empty()` branch + the json test).
- `GET /api/packs/{id}/export` + Content-Disposition `.zip`/`.json` — Task 1 (route + handler; asserted).

**Placeholder scan:** none — full handler, route line, exact deps, complete tests, exact commands. The `zip` API (`SimpleFileOptions`, `ZipWriter`, `ZipArchive`) is the `zip` 2.x surface; if a minor name differs in the resolved version, adjust the import (the test's `ZipArchive` read and the handler's `ZipWriter` write must agree).

**Type consistency:** handler returns `Result<Response, StatusCode>` (both branches `.into_response()`). `export_pack`/`collect_pack_assets` are the Plan-2 signatures over `&Value`. `list_nodes(&OwnerKind::Pack, &id)` + `list_definitions()` mirror `export_template`. `Pack`/`create_pack` match the model + storage used in the test. Zip entry names `manifest.json` / `assets/<rel>` match what Plan 4's importer reads.
