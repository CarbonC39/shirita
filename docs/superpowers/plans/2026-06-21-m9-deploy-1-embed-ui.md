# M9 Deploy — Plan 1: Serve the UI from the binary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `shirita-web` serve the real Vue frontend from inside the binary — embedded via `rust-embed` behind a default-off `embed-ui` cargo feature — with the runtime token injected into `index.html`, frontend chunks at `/static/*`, and an SPA fallback for Vue Router history-mode deep links. Default (feature-off) builds are byte-for-byte unchanged so dev/test never need a frontend build.

**Architecture:** A new `shirita-web/src/embed.rs` holds two always-compiled pure helpers (`inject_runtime`, `is_reserved_prefix`) plus the `#[cfg(feature = "embed-ui")]` rust-embed struct + serving handlers (`serve_index`, `serve_static`, `spa_fallback`). `app()` adds the embed routes only under the feature; otherwise it keeps today's `/` verification page. Vite emits chunks to `dist/static/` (via `build.assetsDir`) so `/static/*` never collides with the existing `/assets/*` user-media route.

**Tech Stack:** Rust, Axum 0.8, `rust-embed` 8 (optional dep), `serde_json`, `tokio::test`; Vue 3 / Vite (one config line).

## Global Constraints

- The `embed-ui` feature is **default off**. `cargo test`/`cargo build`/`app()` with the feature off behave exactly as today (no rust-embed compiled, no `dist/` read). Verified by the default suite staying green with no frontend build.
- Frontend chunks live at `/static/*` (Vite `assetsDir: 'static'`); user media stays at `/assets/*` (unchanged). Frontend code is **not** modified — `client.ts` already reads `window.__SHIRITA_RUNTIME__`.
- Token injection must neutralize `</` inside the inlined JSON so a token containing `</script>` cannot break out of the inline `<script>`.
- Comments/commits in English. Tests: `cargo test -p shirita-web` (default, feature off) and `cargo test -p shirita-web --features embed-ui --test embed_ui_test` (needs `shirita-ui/dist` built first).

---

## File Structure

- `shirita-web/src/embed.rs` — **new.** Pure helpers (always compiled) + feature-gated rust-embed serving.
- `shirita-web/src/lib.rs` — declare `pub mod embed;`; wire the feature-gated routes into `app()`.
- `shirita-web/Cargo.toml` — optional `rust-embed` dep + `[features] embed-ui`.
- `shirita-web/tests/embed_ui_test.rs` — **new**, `#![cfg(feature = "embed-ui")]` integration test.
- `shirita-ui/vite.config.ts` — `build: { assetsDir: 'static' }`.

---

### Task 1: Pure helpers — `inject_runtime` + `is_reserved_prefix`

These compile and test with the feature **off**, so they run in the normal suite.

**Files:**
- Create: `shirita-web/src/embed.rs`
- Modify: `shirita-web/src/lib.rs` (add `pub mod embed;`)

**Interfaces:**
- Produces: `pub fn inject_runtime(html: &str, token: &str) -> String`; `pub(crate) fn is_reserved_prefix(path: &str) -> bool`.

- [ ] **Step 1: Create the module with the pure helpers + failing tests**

Create `shirita-web/src/embed.rs`:

```rust
//! Embedded frontend serving. The two helpers below are always compiled (so
//! they're unit-tested without the feature or a built `dist/`); the rust-embed
//! struct + handlers are gated behind `embed-ui`.

/// Splice `window.__SHIRITA_RUNTIME__ = { base, token }` into `<head>` so the
/// same-origin browser gets the API base + token without a build-time bake.
/// `</` is neutralized so a token containing `</script>` can't break out of the
/// inline `<script>`.
pub fn inject_runtime(html: &str, token: &str) -> String {
    let rt = serde_json::json!({ "base": "", "token": token })
        .to_string()
        .replace("</", "<\\/");
    let tag = format!("<script>window.__SHIRITA_RUNTIME__={rt};</script>");
    match html.rfind("</head>") {
        Some(i) => {
            let mut s = String::with_capacity(html.len() + tag.len());
            s.push_str(&html[..i]);
            s.push_str(&tag);
            s.push_str(&html[i..]);
            s
        }
        None => format!("{tag}{html}"),
    }
}

/// Paths owned by the API / media / static-chunk / health routes — the SPA
/// fallback returns 404 for these instead of serving `index.html`.
pub(crate) fn is_reserved_prefix(path: &str) -> bool {
    path == "/health"
        || path == "/api" || path.starts_with("/api/")
        || path == "/assets" || path.starts_with("/assets/")
        || path == "/static" || path.starts_with("/static/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_runtime_before_head_close() {
        let out = inject_runtime("<html><head><title>x</title></head><body></body></html>", "secret");
        let script_at = out.find("__SHIRITA_RUNTIME__").unwrap();
        let head_close = out.find("</head>").unwrap();
        assert!(script_at < head_close, "script spliced before </head>");
        assert!(out.contains(r#""token":"secret""#));
        assert!(out.contains(r#""base":"""#));
    }

    #[test]
    fn neutralizes_script_breakout_in_token() {
        let out = inject_runtime("<head></head>", "a</script>b");
        assert!(!out.contains("a</script>b"), "raw </script> must not survive");
        assert!(out.contains(r#""token":"a<\/script>b""#));
    }

    #[test]
    fn prepends_when_no_head() {
        let out = inject_runtime("<body>hi</body>", "t");
        assert!(out.starts_with("<script>window.__SHIRITA_RUNTIME__="));
        assert!(out.ends_with("<body>hi</body>"));
    }

    #[test]
    fn reserved_prefixes_are_not_spa_routes() {
        for p in ["/health", "/api", "/api/sessions", "/assets", "/assets/x.png", "/static", "/static/app.js"] {
            assert!(is_reserved_prefix(p), "{p} should be reserved");
        }
        for p in ["/", "/book", "/chat/abc", "/settings"] {
            assert!(!is_reserved_prefix(p), "{p} should fall through to SPA");
        }
    }
}
```

In `shirita-web/src/lib.rs`, add the module declaration next to the other `pub mod` lines (after `pub mod auth;`):

```rust
pub mod embed;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-web --lib embed:: 2>&1 | tail -20`
Expected: FAIL to compile until `embed.rs` exists / the module is declared — then once it compiles, the 4 tests should pass (these helpers are self-contained). If compilation succeeds and tests pass on first write, that's fine; the point is the module is wired and green.

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p shirita-web --lib embed:: 2>&1 | tail -20`
Expected: PASS — `injects_runtime_before_head_close`, `neutralizes_script_breakout_in_token`, `prepends_when_no_head`, `reserved_prefixes_are_not_spa_routes`.

- [ ] **Step 4: Commit**

```bash
git add shirita-web/src/embed.rs shirita-web/src/lib.rs
git commit -m "feat(web): embed module — inject_runtime + is_reserved_prefix (pure, tested)"
```

---

### Task 2: Feature-gated embedded serving + Vite assetsDir

**Files:**
- Modify: `shirita-web/Cargo.toml` (optional dep + feature)
- Modify: `shirita-web/src/embed.rs` (rust-embed struct + handlers)
- Modify: `shirita-web/src/lib.rs` (wire routes into `app()`)
- Modify: `shirita-ui/vite.config.ts` (`assetsDir: 'static'`)
- Test: `shirita-web/tests/embed_ui_test.rs` (new)

**Interfaces:**
- Consumes: `inject_runtime` / `is_reserved_prefix` (Task 1); `AppState` (`state.config.token_secret`); `app()`.
- Produces (only under `embed-ui`): handlers `serve_index`, `serve_static`, `spa_fallback`; routes `/`, `/static/{*path}`, fallback.

- [ ] **Step 1: Repoint Vite chunks to `/static` and rebuild**

In `shirita-ui/vite.config.ts`, add a `build` block to the `defineConfig({...})` object (next to `plugins`/`server`):

```ts
  build: { assetsDir: 'static' },
```

Then build so `dist/static/*` exists (rust-embed reads `dist/` at compile time):

```bash
cd shirita-ui && npm run build 2>&1 | tail -5
```

Expected: `dist/index.html` + `dist/static/*.js|css`. Verify: `ls shirita-ui/dist/static | head` lists hashed chunks, and `grep -o '/static/[^"]*' shirita-ui/dist/index.html | head` shows `/static/...` references.

- [ ] **Step 2: Add the optional dep + feature**

In `shirita-web/Cargo.toml`, under `[dependencies]` add:

```toml
rust-embed = { version = "8", optional = true }
```

And add a new `[features]` section (after `[dependencies]`, before `[dev-dependencies]`):

```toml
[features]
embed-ui = ["dep:rust-embed"]
```

- [ ] **Step 3: Add the rust-embed struct + handlers**

Append to `shirita-web/src/embed.rs` (after the pure helpers, before `#[cfg(test)] mod tests`):

```rust
#[cfg(feature = "embed-ui")]
mod serving {
    use super::{inject_runtime, is_reserved_prefix};
    use crate::AppState;
    use axum::extract::{Path, State};
    use axum::http::{header, StatusCode, Uri};
    use axum::response::{Html, IntoResponse, Response};

    /// The built Vue app, embedded at compile time (release) / read from disk
    /// (debug). Path is relative to `shirita-web/Cargo.toml`.
    #[derive(rust_embed::RustEmbed)]
    #[folder = "../shirita-ui/dist"]
    struct Ui;

    fn index_response(state: &AppState) -> Response {
        match Ui::get("index.html") {
            Some(f) => {
                let html = String::from_utf8_lossy(&f.data);
                Html(inject_runtime(&html, &state.config.token_secret)).into_response()
            }
            None => (StatusCode::INTERNAL_SERVER_ERROR, "embedded index.html missing").into_response(),
        }
    }

    /// `GET /` — the SPA shell with the runtime token injected.
    pub async fn serve_index(State(state): State<AppState>) -> Response {
        index_response(&state)
    }

    /// `GET /static/{*path}` — an embedded frontend chunk, content-typed by
    /// extension, immutably cached (filenames are content-hashed).
    pub async fn serve_static(Path(path): Path<String>) -> Response {
        match Ui::get(&format!("static/{path}")) {
            Some(f) => {
                let mime = f.metadata.mimetype().to_string();
                (
                    [
                        (header::CONTENT_TYPE, mime),
                        (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
                    ],
                    f.data.into_owned(),
                )
                    .into_response()
            }
            None => StatusCode::NOT_FOUND.into_response(),
        }
    }

    /// Router fallback — unknown app routes get the SPA shell (history-mode deep
    /// links); reserved prefixes 404 so an unknown API path never returns HTML.
    pub async fn spa_fallback(uri: Uri, State(state): State<AppState>) -> Response {
        if is_reserved_prefix(uri.path()) {
            return StatusCode::NOT_FOUND.into_response();
        }
        index_response(&state)
    }
}

#[cfg(feature = "embed-ui")]
pub use serving::{serve_index, serve_static, spa_fallback};
```

- [ ] **Step 4: Wire the routes into `app()`**

In `shirita-web/src/lib.rs`, replace the final router assembly (currently):

```rust
    Router::new()
        .route("/", get(routes::index::index))
        .route("/health", get(routes::health::health))
        .nest("/api", protected)
        .nest_service("/assets", ServeDir::new(assets_dir))
        .with_state(state)
}
```

with:

```rust
    let router = Router::new()
        .route("/health", get(routes::health::health))
        .nest("/api", protected)
        .nest_service("/assets", ServeDir::new(assets_dir));

    #[cfg(feature = "embed-ui")]
    let router = router
        .route("/", get(embed::serve_index))
        .route("/static/{*path}", get(embed::serve_static))
        .fallback(embed::spa_fallback);

    #[cfg(not(feature = "embed-ui"))]
    let router = router.route("/", get(routes::index::index));

    router.with_state(state)
}
```

- [ ] **Step 5: Verify the default (feature-off) build is unchanged + green**

Run: `cargo test -p shirita-web 2>&1 | grep -E "test result:|error" | tail -8`
Expected: all existing suites pass (no frontend build needed; the feature is off so `embed::serving`/`rust-embed` aren't compiled). The `embed::` pure-helper tests from Task 1 still pass.

- [ ] **Step 6: Write the feature-gated integration test**

Create `shirita-web/tests/embed_ui_test.rs`:

```rust
#![cfg(feature = "embed-ui")]
//! Embedded-UI serving. Compiled/run only with `--features embed-ui`, which
//! needs `shirita-ui/dist` built (Step 1). The Docker/CI build exercises this.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
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

async fn get(state: &AppState, uri: &str) -> (StatusCode, Vec<u8>) {
    let req = Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, body)
}

#[tokio::test]
async fn index_serves_spa_with_injected_token() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let (st, body) = get(&state, "/").await;
    assert_eq!(st, StatusCode::OK);
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("window.__SHIRITA_RUNTIME__="), "runtime injected");
    assert!(html.contains(r#""token":"secret-token""#), "token present");
    assert!(html.contains("/static/"), "built index references /static chunks");
}

#[tokio::test]
async fn deep_link_serves_index_unknown_api_404s() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    // Vue Router history deep link → SPA shell.
    let (st, body) = get(&state, "/book").await;
    assert_eq!(st, StatusCode::OK);
    assert!(String::from_utf8_lossy(&body).contains("__SHIRITA_RUNTIME__"));
    // Unknown API path → 404, never HTML.
    let (st2, _b2) = get(&state, "/api/nope").await;
    assert_eq!(st2, StatusCode::NOT_FOUND);
}
```

- [ ] **Step 7: Run the feature-gated test to verify it passes**

Run: `cargo test -p shirita-web --features embed-ui --test embed_ui_test 2>&1 | tail -15`
Expected: PASS — `/` returns the built `index.html` with `__SHIRITA_RUNTIME__` + `secret-token` + `/static/` references; `/book` returns the SPA shell; `/api/nope` → 404. (Requires Step 1's `npm run build`.)

- [ ] **Step 8: Commit**

```bash
git add shirita-web/Cargo.toml Cargo.lock shirita-web/src/embed.rs shirita-web/src/lib.rs shirita-web/tests/embed_ui_test.rs shirita-ui/vite.config.ts
git commit -m "feat(web): embed Vue UI behind embed-ui feature — /static + token-injected SPA"
```

---

## Final Verification

- [ ] **Default-off green + feature-on serving**

Run:
```bash
cargo test -p shirita-web 2>&1 | grep -E "test result:" | tail -3
cd shirita-ui && npm run build 2>&1 | tail -3 && cd ..
cargo test -p shirita-web --features embed-ui --test embed_ui_test 2>&1 | tail -6
cargo build -p shirita-web --release --features embed-ui 2>&1 | tail -3
```
Expected: default suite green (feature off, no dist needed); the feature-gated test passes against the built dist; the release binary builds with the UI embedded.

---

## Self-Review

**Spec coverage (spec §3–§5, §10):**
- `embed-ui` feature, default off, rust-embed — Task 2 (Cargo feature + `Ui` struct); default-off unchanged asserted in Task 2 Step 5.
- `/static/*` chunks + immutable cache — Task 2 `serve_static`; Vite `assetsDir` Step 1.
- Token injection into `index.html` — Task 1 `inject_runtime` (pure, tested) + Task 2 `index_response`; asserted in both the unit tests and the integration test.
- SPA fallback + reserved-prefix 404 — Task 1 `is_reserved_prefix` + Task 2 `spa_fallback`; asserted by `deep_link_serves_index_unknown_api_404s`.
- No frontend code change (`client.ts` already reads `__SHIRITA_RUNTIME__`) — confirmed; only `vite.config.ts` changes.
- Pure-fn tests always run; feature-gated integration test isolated — Task 1 (`#[cfg(test)]`, default suite) vs Task 2 (`#![cfg(feature = "embed-ui")]`).

**Placeholder scan:** none — full module, full handlers, exact Cargo lines, exact router replacement, complete tests, exact commands.

**Type consistency:** `inject_runtime(&str, &str) -> String` and `is_reserved_prefix(&str) -> bool` are defined in Task 1 and consumed by Task 2's `serving` module (`use super::{inject_runtime, is_reserved_prefix}`). Handlers return `axum::response::Response`; `serve_static` uses `Path<String>` for the `{*path}` wildcard; `spa_fallback` takes `Uri` + `State<AppState>`. `state.config.token_secret` matches `shirita_core::Config`. `app(state)` signature unchanged. `rust-embed` 8 API used: `Ui::get(path) -> Option<EmbeddedFile>`, `.data: Cow<[u8]>`, `.metadata.mimetype() -> &str`.

**Risk notes:**
- rust-embed reads `dist/` at compile time; the feature-gated test + release build require `npm run build` first (called out in Task 2 Step 1 / Final Verification). Default builds never touch `dist/`.
- `route_layer(require_bearer)` on the `/api` nest means an unmatched `/api/*` returns 404 (not 401) — the integration test asserts 404; if a future Axum changes this, adjust the reserved-prefix guard, which already 404s `/api/*` at the fallback regardless.
