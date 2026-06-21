# Pack Zip Bundle — Plan 1: Asset content-hash infrastructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every asset a content hash so the zip importer (Plan 4) can deduplicate images — one image, one copy in the library, referenced everywhere. Add the `hash` column + `sha256_hex`, compute it on every save, expose `find_asset_by_hash`, and backfill existing assets at startup.

**Architecture:** A nullable `hash` column on `assets` (sha256 hex of the file bytes). `shirita-core` gains a pure `sha256_hex` helper; the two web save sites set `asset.hash` before `create_asset`; a `ensure_asset_hashes` startup step (next to the existing `ensure_*` calls) fills any missing hash by reading the file — so dedup covers pre-existing assets too. No dedup logic yet (that's Plan 4); this plan only makes hashes exist and be queryable.

**Tech Stack:** Rust, sqlx runtime API (SqliteStorage), `sha2` crate, `tokio::test` integration tests.

## Global Constraints

- `hash` is **nullable** (`Option<String>`) — old rows start NULL and are backfilled; new saves always set it.
- Hash = lowercase hex sha256 of the raw file bytes (`sha256_hex`), computed in `shirita-core` so both the web save sites and the core backfill share one implementation.
- Dedup matching is **by hash only** (kind-agnostic — same bytes == same file); the actual reuse logic lands in Plan 4.
- Comments/commits in English. Backend tests: `cargo test -p shirita-core` and `cargo test -p shirita-web`.

---

## File Structure

- `shirita-core/Cargo.toml` — add `sha2`. (Task 1)
- `shirita-core/migrations/0020_assets_hash.sql` — add the column. (Task 1)
- `shirita-core/src/models/asset.rs` — `Asset.hash`. (Task 1)
- `shirita-core/src/hashing.rs` — `sha256_hex`. (Task 1)
- `shirita-core/src/lib.rs` — module + re-exports. (Tasks 1 & 3)
- `shirita-core/src/storage/mod.rs` — trait methods. (Task 1)
- `shirita-core/src/storage/sqlite.rs` — SQL + methods + test. (Task 1)
- `shirita-web/src/routes/assets.rs` — hash on upload. (Task 2)
- `shirita-web/src/routes/import_export.rs` — hash on PNG save. (Task 2)
- `shirita-web/tests/assets_hash_test.rs` — upload-sets-hash integration test. (Task 2)
- `shirita-core/src/seed.rs` — `ensure_asset_hashes` + test. (Task 3)
- `shirita-web/src/main.rs`, `shirita-tauri/src/main.rs` — call it. (Task 3)

---

### Task 1: Schema + model + `sha256_hex` + storage

**Files:**
- Modify: `shirita-core/Cargo.toml`
- Create: `shirita-core/migrations/0020_assets_hash.sql`
- Modify: `shirita-core/src/models/asset.rs`
- Create: `shirita-core/src/hashing.rs`
- Modify: `shirita-core/src/lib.rs`
- Modify: `shirita-core/src/storage/mod.rs`
- Modify: `shirita-core/src/storage/sqlite.rs`

**Interfaces:**
- Produces: `Asset.hash: Option<String>`; `shirita_core::sha256_hex(&[u8]) -> String`; `Storage::find_asset_by_hash(&str) -> Result<Option<Asset>>`; `Storage::set_asset_hash(&str, &str) -> Result<()>`. (Plan 2 uses `sha256_hex`; Plan 4 uses `find_asset_by_hash`; Task 3 uses `set_asset_hash`.)

- [ ] **Step 1: Add the `sha2` dependency**

In `shirita-core/Cargo.toml`, under `[dependencies]`, add:

```toml
sha2 = "0.10"
```

- [ ] **Step 2: Add the migration**

Create `shirita-core/migrations/0020_assets_hash.sql`:

```sql
-- Content hash (sha256 hex) for asset dedup. Nullable; backfilled at startup.
ALTER TABLE assets ADD COLUMN hash TEXT;
```

- [ ] **Step 3: Add `hash` to the `Asset` model**

In `shirita-core/src/models/asset.rs`, add the field to the struct (after `kind`) and default it in `new`:

```rust
pub struct Asset {
    pub id: String,
    pub name: String,
    pub path: String,
    /// Library this asset belongs to: `"avatar"` or `"background"`.
    pub kind: String,
    /// sha256 hex of the file bytes; None until set on save / backfill.
    #[serde(default)]
    pub hash: Option<String>,
    pub created_at: String,
}
```

And in `Asset::new`, add `hash: None,` (place it after the `kind` line):

```rust
            kind: "background".into(),
            hash: None,
            created_at: chrono::Utc::now().to_rfc3339(),
```

- [ ] **Step 4: Add the `sha256_hex` helper + the failing unit test**

Create `shirita-core/src/hashing.rs`:

```rust
//! Content hashing for asset dedup.

use sha2::{Digest, Sha256};

/// Lowercase hex sha256 of the given bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
```

In `shirita-core/src/lib.rs`, register the module and re-export (next to the other `mod`/`pub use` lines):

```rust
mod hashing;
pub use hashing::sha256_hex;
```

- [ ] **Step 5: Add the storage trait methods**

In `shirita-core/src/storage/mod.rs`, in the assets section of the `Storage` trait (after `delete_asset`), add:

```rust
    /// First asset whose content hash matches, if any (dedup lookup).
    async fn find_asset_by_hash(&self, hash: &str) -> Result<Option<Asset>>;
    /// Set/replace an asset's content hash (used by the startup backfill).
    async fn set_asset_hash(&self, id: &str, hash: &str) -> Result<()>;
```

- [ ] **Step 6: Write the failing storage test**

In `shirita-core/src/storage/sqlite.rs`, inside the existing `#[cfg(test)] mod tests { … }`, add:

```rust
    #[tokio::test]
    async fn asset_hash_create_lookup_and_set() {
        use crate::models::asset::Asset;
        let dir = tempfile::tempdir().unwrap();
        let storage = SqliteStorage::connect(dir.path().join("h.db").to_str().unwrap()).await.unwrap();
        storage.run_migrations().await.unwrap();

        // created with a hash → findable by it
        let mut a = Asset::new("face", "u1.png");
        a.hash = Some("abc123".into());
        storage.create_asset(&a).await.unwrap();
        assert_eq!(storage.find_asset_by_hash("abc123").await.unwrap().unwrap().id, a.id);
        assert!(storage.find_asset_by_hash("missing").await.unwrap().is_none());

        // created without a hash → backfillable via set_asset_hash
        let b = Asset::new("bg", "u2.png");
        storage.create_asset(&b).await.unwrap();
        assert!(storage.get_asset(&b.id).await.unwrap().unwrap().hash.is_none());
        storage.set_asset_hash(&b.id, "def456").await.unwrap();
        assert_eq!(storage.get_asset(&b.id).await.unwrap().unwrap().hash.as_deref(), Some("def456"));
    }
```

- [ ] **Step 7: Run the test to verify it fails**

Run: `cargo test -p shirita-core asset_hash_create_lookup_and_set 2>&1 | tail -20`
Expected: FAIL — `find_asset_by_hash` / `set_asset_hash` not implemented, `Asset` has no `hash`, the SELECTs don't read `hash`.

- [ ] **Step 8: Implement the SQL in `sqlite.rs`**

In `shirita-core/src/storage/sqlite.rs`:

(a) `list_assets` — change both SELECT strings to include `hash` and widen the row tuple. The two query strings become:

```rust
                "SELECT id, name, path, kind, created_at, hash FROM assets WHERE kind = ? ORDER BY created_at DESC, id DESC",
```
```rust
                "SELECT id, name, path, kind, created_at, hash FROM assets ORDER BY created_at DESC, id DESC",
```

and the row mapping (widen the tuple type to 6 and build the `Asset`):

```rust
            .map(|(id, name, path, kind, created_at, hash): (String, String, String, String, String, Option<String>)| Asset { id, name, path, kind, hash, created_at })
```

(b) `get_asset` — SELECT + mapping:

```rust
            "SELECT id, name, path, kind, created_at, hash FROM assets WHERE id = ?",
```
```rust
        Ok(row.map(|(id, name, path, kind, created_at, hash): (String, String, String, String, String, Option<String>)| Asset { id, name, path, kind, hash, created_at }))
```

(c) `create_asset` — add the column + bind:

```rust
        sqlx::query("INSERT INTO assets (id, name, path, kind, created_at, hash) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&asset.id)
            .bind(&asset.name)
            .bind(&asset.path)
            .bind(&asset.kind)
            .bind(&asset.created_at)
            .bind(&asset.hash)
```

(d) Add the two new methods (next to the other asset methods):

```rust
    async fn find_asset_by_hash(&self, hash: &str) -> Result<Option<Asset>> {
        let row = sqlx::query_as::<_, (String, String, String, String, String, Option<String>)>(
            "SELECT id, name, path, kind, created_at, hash FROM assets WHERE hash = ? LIMIT 1",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, name, path, kind, created_at, hash)| Asset { id, name, path, kind, hash, created_at }))
    }

    async fn set_asset_hash(&self, id: &str, hash: &str) -> Result<()> {
        sqlx::query("UPDATE assets SET hash = ? WHERE id = ?")
            .bind(hash)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

(Match the exact `query_as` / binding style and the `self.pool` field name already used by the surrounding asset methods; adjust if the local names differ.)

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cargo test -p shirita-core asset_hash 2>&1 | tail -20`
Expected: PASS — `asset_hash_create_lookup_and_set` and `sha256_hex_matches_known_vector`.

- [ ] **Step 10: Build + commit**

```bash
cargo build -p shirita-core 2>&1 | tail -4
git add shirita-core/Cargo.toml Cargo.lock shirita-core/migrations/0020_assets_hash.sql shirita-core/src/models/asset.rs shirita-core/src/hashing.rs shirita-core/src/lib.rs shirita-core/src/storage/mod.rs shirita-core/src/storage/sqlite.rs
git commit -m "feat(core): asset content hash (column + sha256_hex + find_by_hash/set_hash)"
```

---

### Task 2: Compute the hash on every save

**Files:**
- Modify: `shirita-web/src/routes/assets.rs`
- Modify: `shirita-web/src/routes/import_export.rs`
- Test: `shirita-web/tests/assets_hash_test.rs`

**Interfaces:**
- Consumes: `shirita_core::sha256_hex` (Task 1).
- Produces: uploaded / PNG-imported assets carry a `hash`. (Plan 4 relies on existing assets having hashes to dedup against.)

- [ ] **Step 1: Write the failing integration test**

Create `shirita-web/tests/assets_hash_test.rs` (mirrors the `variables_test.rs` `test_state` harness):

```rust
//! Uploading an asset records its content hash.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use shirita_core::{
    sha256_hex, Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter,
    TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state(dir: &std::path::Path) -> AppState {
    let storage = SqliteStorage::connect(dir.join("a.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", dir.join("assets").to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

#[tokio::test]
async fn upload_records_content_hash() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    let state = test_state(dir.path()).await;

    let bytes = b"fake-image-bytes";
    let boundary = "BOUNDARY";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.png\"\r\n\
         Content-Type: image/png\r\n\r\n{data}\r\n--{b}--\r\n",
        b = boundary,
        data = std::str::from_utf8(bytes).unwrap(),
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/assets?kind=avatar")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let assets = state.storage.list_assets(None).await.unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].hash.as_deref(), Some(sha256_hex(bytes).as_str()));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shirita-web --test assets_hash_test 2>&1 | tail -20`
Expected: FAIL — `assets[0].hash` is `None` (the upload handler doesn't set it yet).

- [ ] **Step 3: Set the hash in the upload handler**

In `shirita-web/src/routes/assets.rs`, in `upload`, after `asset.kind = norm_kind(q.kind.as_deref());` and before `create_asset`, add:

```rust
        asset.hash = Some(shirita_core::sha256_hex(data.as_ref()));
```

- [ ] **Step 4: Set the hash in the PNG import save**

In `shirita-web/src/routes/import_export.rs`, in `save_png_asset`, after `asset.kind = "avatar".into();` and before `create_asset`, add:

```rust
    asset.hash = Some(shirita_core::sha256_hex(bytes));
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p shirita-web --test assets_hash_test 2>&1 | tail -20`
Expected: PASS — the uploaded asset's `hash` equals `sha256_hex(bytes)`. (The PNG path uses the identical one-liner on the same `create_asset`, so it's covered by the same `sha256_hex` mechanism without a separate PNG fixture.)

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/assets.rs shirita-web/src/routes/import_export.rs shirita-web/tests/assets_hash_test.rs
git commit -m "feat(web): record asset content hash on upload + PNG import"
```

---

### Task 3: `ensure_asset_hashes` startup backfill

**Files:**
- Modify: `shirita-core/src/seed.rs`
- Modify: `shirita-core/src/lib.rs`
- Modify: `shirita-web/src/main.rs`
- Modify: `shirita-tauri/src/main.rs`

**Interfaces:**
- Consumes: `Storage::{list_assets, set_asset_hash}`, `sha256_hex` (Task 1).
- Produces: `shirita_core::ensure_asset_hashes(&storage, assets_dir) -> Result<()>` — idempotent; fills `hash` for any asset whose file exists and whose hash is NULL.

- [ ] **Step 1: Write the failing test**

In `shirita-core/src/seed.rs`, inside its `#[cfg(test)] mod tests { … }`, add:

```rust
    #[tokio::test]
    async fn ensure_asset_hashes_backfills_missing() {
        use crate::models::asset::Asset;
        let dir = tempfile::tempdir().unwrap();
        let assets_dir = dir.path().join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::write(assets_dir.join("img.png"), b"hello-bytes").unwrap();

        let storage = SqliteStorage::connect(dir.path().join("s.db").to_str().unwrap()).await.unwrap();
        storage.run_migrations().await.unwrap();
        let a = Asset::new("img", "img.png"); // hash None
        storage.create_asset(&a).await.unwrap();

        crate::ensure_asset_hashes(&storage, assets_dir.to_str().unwrap()).await.unwrap();
        crate::ensure_asset_hashes(&storage, assets_dir.to_str().unwrap()).await.unwrap(); // idempotent

        let got = storage.get_asset(&a.id).await.unwrap().unwrap();
        assert_eq!(got.hash.as_deref(), Some(crate::sha256_hex(b"hello-bytes").as_str()));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shirita-core ensure_asset_hashes_backfills_missing 2>&1 | tail -20`
Expected: FAIL — `ensure_asset_hashes` doesn't exist.

- [ ] **Step 3: Implement `ensure_asset_hashes`**

In `shirita-core/src/seed.rs`, add (mirroring the `ensure_*` signature style; the file already uses `Storage` + `Result`):

```rust
/// Backfill the content hash of any asset whose file exists but whose hash is
/// NULL (pre-`0020` rows). Idempotent; run at startup after migrations.
pub async fn ensure_asset_hashes<S: Storage + ?Sized>(storage: &S, assets_dir: &str) -> Result<()> {
    for asset in storage.list_assets(None).await? {
        if asset.hash.is_some() {
            continue;
        }
        let path = std::path::Path::new(assets_dir).join(&asset.path);
        match tokio::fs::read(&path).await {
            Ok(bytes) => storage.set_asset_hash(&asset.id, &crate::sha256_hex(&bytes)).await?,
            Err(_) => tracing::warn!(asset = %asset.id, path = %asset.path, "ensure_asset_hashes: file missing, skipping"),
        }
    }
    Ok(())
}
```

In `shirita-core/src/lib.rs`, add `ensure_asset_hashes` to the existing `pub use seed::{…}` re-export line (the one already exporting `ensure_default_template`, `ensure_templates_have_content_node`).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shirita-core ensure_asset_hashes 2>&1 | tail -20`
Expected: PASS — the asset's hash is backfilled from the file bytes; second call is a no-op.

- [ ] **Step 5: Wire it into both startup paths**

In `shirita-web/src/main.rs`, after `ensure_templates_have_content_node(&storage).await?;` add:

```rust
    shirita_core::ensure_asset_hashes(&storage, &config.assets_dir).await?;
```

In `shirita-tauri/src/main.rs`, after the `ensure_templates_have_content_node(&storage)` call, add the equivalent (match the file's existing `.await?`/error-handling style around the other `ensure_*` calls):

```rust
    shirita_core::ensure_asset_hashes(&storage, &config.assets_dir).await?;
```

- [ ] **Step 6: Build the workspace + commit**

```bash
cargo build --workspace 2>&1 | tail -4
git add shirita-core/src/seed.rs shirita-core/src/lib.rs shirita-web/src/main.rs shirita-tauri/src/main.rs
git commit -m "feat(core): ensure_asset_hashes startup backfill, wired into web + tauri"
```

---

## Final Verification

- [ ] **Backend test + build sweep**

Run: `cargo test -p shirita-core 2>&1 | grep -E "test result:" | tail -3 && cargo test -p shirita-web --test assets_hash_test 2>&1 | tail -4 && cargo build --workspace 2>&1 | tail -4`
Expected: shirita-core suites pass (incl. the two new tests), the web asset-hash test passes, workspace builds clean.

---

## Self-Review

**Spec coverage (spec §5):**
- `hash` column on `assets` — Task 1 (migration 0020 + model).
- Compute + store hash on every save (upload, PNG import) — Task 2; zip-import save reuses `sha256_hex` in Plan 4.
- Startup backfill so dedup covers pre-existing assets — Task 3 (`ensure_asset_hashes`, wired into both mains).
- `find_asset_by_hash` for the dedup lookup — Task 1 (consumed by Plan 4).

**Placeholder scan:** none — exact SQL, full helper/method/test code, exact commands. (Two notes flag matching local style: `self.pool`/`query_as` form in sqlite.rs and the tauri `ensure_*` error-handling shape — both are "match the adjacent existing code," not missing content.)

**Type consistency:** `Asset.hash: Option<String>` flows through `create_asset` bind, the widened 6-tuple SELECT mappings, `find_asset_by_hash`/`set_asset_hash`, and `ensure_asset_hashes`. `sha256_hex(&[u8]) -> String` is called with `data.as_ref()` (axum `Bytes`) and `bytes` (`&[u8]`) and `&bytes` (Vec) consistently. `ensure_asset_hashes(&storage, &str)` matches the main call sites passing `&config.assets_dir`.
