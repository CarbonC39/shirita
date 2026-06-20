# Pack/Preset Separation — Plan 3: Pack Management & Mounting API

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose Packs over REST — CRUD, content-tree editing, mounting onto a session, and creating chats that mount packs (with greeting + variable seeding) — so a client can build a pack, fill it, mount it, and start a chat.

**Architecture:** Mirror the existing `templates` route module for pack CRUD; **reuse** the owner-agnostic prompt-node handlers for pack content (they resolve `owner_kind` from the query, which Plan 1 extended to `pack`). Session mounting mirrors the `mounted_definitions` endpoint. Session creation gains `pack_ids`, seeds variables via Plan 2's `resolve_schema_with_packs`, and seeds the opening greeting from mounted packs' `first_message`.

**Tech Stack:** Rust, `axum`, `shirita-web` (+ a one-line re-export in `shirita-core`). Integration tests use the existing `tower::ServiceExt::oneshot` harness with Bearer `secret-token`.

## Global Constraints

- Code comments and git commit messages in **English**.
- Consumes Plan 1 + Plan 2: `Pack`/`PackIdentity`, `Storage::{create_pack,get_pack,list_packs,update_pack,delete_pack,set_mounted_packs}`, `Session.mounted_packs`, `OwnerKind::Pack`, `NodeKind::Content`, `state::resolve_schema_with_packs`.
- **Identity pack-aware, display-schema pack vars (web `get_state`/`get_session_identity`), and startup backfill wiring are NOT in this plan** — they are Plan 4. This plan is management + mounting + chat creation.
- New REST routes go under `/api` (auth-gated). Tests must send `Authorization: Bearer secret-token`.
- After Task 4, the `resolve_schema` import in `sessions.rs` becomes unused — remove it in that task (zero-warning, like the Plan 2 cleanup).
- Every task ends green: `cargo test --workspace`, zero warnings.

---

### Task 1: Pack CRUD endpoints

**Files:**
- Modify: `shirita-core/src/lib.rs` (re-export `Pack`/`PackIdentity`)
- Create: `shirita-web/src/routes/packs.rs`
- Modify: `shirita-web/src/routes/mod.rs`, `shirita-web/src/lib.rs` (routes)
- Test: `shirita-web/tests/packs_test.rs` (new; carries the shared harness for this plan)

**Interfaces:**
- Produces: `GET/POST /api/packs`, `GET/PUT/DELETE /api/packs/{id}`, `POST /api/packs/{id}/duplicate`. `shirita_core::Pack` / `shirita_core::PackIdentity` re-exported.

- [ ] **Step 1: Re-export `Pack`** — in `shirita-core/src/lib.rs`, after `pub use models::template::Template;` (line ~46) add:

```rust
pub use models::pack::{Pack, PackIdentity};
```

- [ ] **Step 2: Write the failing test** — create `shirita-web/tests/packs_test.rs`:

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt; // oneshot

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("packs_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState {
        storage,
        config,
        provider,
        token_counter,
        model: "test-model".into(),
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client(),
    }
}

/// Send an authenticated request, returning (status, body-bytes).
async fn send(state: &AppState, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Vec<u8>) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token");
    let body = match body {
        Some(v) => {
            b = b.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    let res = app(state.clone()).oneshot(b.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, bytes)
}

fn body_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

#[tokio::test]
async fn pack_crud_roundtrip() {
    let state = test_state().await;
    let (st, b) = send(&state, "POST", "/api/packs", Some(json!({
        "name": "Alice", "identity": { "display_name": "Alice", "avatar": "a.png" }
    }))).await;
    assert_eq!(st, StatusCode::OK);
    let created = body_json(&b);
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["name"], "Alice");
    assert_eq!(created["identity"]["display_name"], "Alice");

    let (st, b) = send(&state, "GET", &format!("/api/packs/{id}"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b)["identity"]["avatar"], "a.png");

    let (st, b) = send(&state, "GET", "/api/packs", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b).as_array().unwrap().len(), 1);

    let (st, b) = send(&state, "PUT", &format!("/api/packs/{id}"), Some(json!({
        "name": "Alice 2", "identity": { "display_name": "Alice" }
    }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b)["name"], "Alice 2");

    let (st, _) = send(&state, "DELETE", &format!("/api/packs/{id}"), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (st, _) = send(&state, "GET", &format!("/api/packs/{id}"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p shirita-web --test packs_test`
Expected: FAIL — compile error (`routes::packs` missing) / 404.

- [ ] **Step 4: Create the route module** `shirita-web/src/routes/packs.rs` (mirrors `templates.rs`):

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use shirita_core::{OwnerKind, Pack, PackIdentity};

use crate::AppState;

#[derive(Deserialize)]
pub struct PackBody {
    pub name: String,
    #[serde(default)]
    pub identity: PackIdentity,
    #[serde(default)]
    pub meta: Value,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Pack>>, StatusCode> {
    state.storage.list_packs().await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn create(State(state): State<AppState>, Json(body): Json<PackBody>) -> Result<Json<Pack>, StatusCode> {
    let mut p = Pack::new(body.name);
    p.identity = body.identity;
    if !body.meta.is_null() {
        p.meta = body.meta;
    }
    state.storage.create_pack(&p).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(p))
}

pub async fn get(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Pack>, StatusCode> {
    state.storage.get_pack(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.map(Json).ok_or(StatusCode::NOT_FOUND)
}

pub async fn update(State(state): State<AppState>, Path(id): Path<String>, Json(body): Json<PackBody>) -> Result<Json<Pack>, StatusCode> {
    let mut p = state.storage.get_pack(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    p.name = body.name;
    p.identity = body.identity;
    if !body.meta.is_null() {
        p.meta = body.meta;
    }
    p.updated_at = chrono::Utc::now().to_rfc3339();
    state.storage.update_pack(&p).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(p))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, StatusCode> {
    state.storage.delete_pack(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn duplicate(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Pack>, StatusCode> {
    let original = state.storage.get_pack(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let mut copy = Pack::new(format!("{} (copy)", original.name));
    copy.identity = original.identity;
    copy.meta = original.meta;
    state.storage.create_pack(&copy).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state.storage.copy_nodes(&OwnerKind::Pack, &id, &OwnerKind::Pack, &copy.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(copy))
}
```

- [ ] **Step 5: Register the module + routes.** In `shirita-web/src/routes/mod.rs` add `pub mod packs;` (alphabetical). In `shirita-web/src/lib.rs`, after the templates routes (line ~96) add:

```rust
        .route("/packs", get(routes::packs::list).post(routes::packs::create))
        .route("/packs/{id}", get(routes::packs::get).put(routes::packs::update).delete(routes::packs::delete))
        .route("/packs/{id}/duplicate", post(routes::packs::duplicate))
```

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p shirita-web --test packs_test`
Expected: PASS.

- [ ] **Step 7: Full suite + commit**

Run: `cargo test --workspace` — PASS, zero warnings.

```bash
git add shirita-core/src/lib.rs shirita-web/src/routes/packs.rs shirita-web/src/routes/mod.rs shirita-web/src/lib.rs shirita-web/tests/packs_test.rs
git commit -m "feat(web): Pack CRUD REST endpoints"
```

---

### Task 2: Pack node routes + content node on template create

**Files:**
- Modify: `shirita-web/src/lib.rs` (pack node routes)
- Modify: `shirita-web/src/routes/templates.rs` (`create` seeds a content node)
- Test: `shirita-web/tests/packs_test.rs`

**Interfaces:**
- Produces: `GET/POST /api/packs/{id}/nodes` and `PUT /api/packs/{id}/nodes/reorder` (reusing `prompt_nodes` handlers with `?owner_kind=pack`). New templates own a `content` node before `history`.

- [ ] **Step 1: Write the failing test** — append to `shirita-web/tests/packs_test.rs`:

```rust
#[tokio::test]
async fn new_template_has_content_before_history() {
    let state = test_state().await;
    let (_, b) = send(&state, "POST", "/api/templates", Some(json!({ "name": "T" }))).await;
    let tid = body_json(&b)["id"].as_str().unwrap().to_string();
    let (st, b) = send(&state, "GET", &format!("/api/templates/{tid}/nodes?owner_kind=template"), None).await;
    assert_eq!(st, StatusCode::OK);
    let nodes = body_json(&b);
    let arr = nodes.as_array().unwrap();
    let content = arr.iter().find(|n| n["kind"] == "content").expect("content node");
    let history = arr.iter().find(|n| n["kind"] == "history").expect("history node");
    assert!(content["sort_order"].as_i64() < history["sort_order"].as_i64());
}

#[tokio::test]
async fn pack_nodes_crud_via_reused_endpoints() {
    let state = test_state().await;
    let (_, b) = send(&state, "POST", "/api/packs", Some(json!({ "name": "Alice" }))).await;
    let pid = body_json(&b)["id"].as_str().unwrap().to_string();
    let (_, b) = send(&state, "POST", "/api/definitions", Some(json!({ "type": "char", "name": "Alice", "content": "hi" }))).await;
    let did = body_json(&b)["id"].as_str().unwrap().to_string();

    let (st, b) = send(&state, "POST", &format!("/api/packs/{pid}/nodes?owner_kind=pack"),
        Some(json!({ "kind": "ref", "definition_id": did }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b)["owner_kind"], "pack");

    let (st, b) = send(&state, "GET", &format!("/api/packs/{pid}/nodes?owner_kind=pack"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b).as_array().unwrap().len(), 1);
}
```

> The definitions create endpoint shape (`type`/`name`/`content`) matches the existing `definitions_test.rs`; if it differs, match that file.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test packs_test new_template_has_content pack_nodes_crud`
Expected: FAIL — no content node on new template / 404 on `/packs/{id}/nodes`.

- [ ] **Step 3: Add the pack node routes.** In `shirita-web/src/lib.rs`, after the template `nodes` routes (line ~99) add:

```rust
        .route("/packs/{id}/nodes", get(routes::prompt_nodes::list_nodes).post(routes::prompt_nodes::create_node))
        .route("/packs/{id}/nodes/reorder", put(routes::prompt_nodes::reorder_nodes))
```

- [ ] **Step 4: Seed a content node on template create.** In `shirita-web/src/routes/templates.rs::create`, replace the history-seeding block (lines ~25–29) with content-then-history:

```rust
    // Auto-add the undeletable magic nodes: <<content>> (pack mount point) then
    // the chat-history node. Default enabled; content sorts before history.
    let mut content = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "content");
    content.kind = NodeKind::Content;
    content.tag = None;
    state.storage.create_node(&content).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 1, "history");
    hist.kind = NodeKind::History;
    hist.tag = None;
    state.storage.create_node(&hist).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p shirita-web --test packs_test new_template_has_content pack_nodes_crud`
Expected: PASS.

- [ ] **Step 6: Full suite + commit**

Run: `cargo test --workspace` — PASS, zero warnings.

```bash
git add shirita-web/src/lib.rs shirita-web/src/routes/templates.rs shirita-web/tests/packs_test.rs
git commit -m "feat(web): pack node endpoints + content node on template create"
```

---

### Task 3: Session→pack mounting endpoint

**Files:**
- Modify: `shirita-web/src/routes/sessions.rs` (add `SetPacks`, `set_packs`, `get_packs`)
- Modify: `shirita-web/src/lib.rs` (route)
- Test: `shirita-web/tests/packs_test.rs`

**Interfaces:**
- Produces: `PUT /api/sessions/{id}/packs` (`{ "pack_ids": [..] }`) and `GET /api/sessions/{id}/packs` (`["..", ..]`).

- [ ] **Step 1: Write the failing test** — append to `shirita-web/tests/packs_test.rs`:

```rust
#[tokio::test]
async fn session_pack_mounts_roundtrip() {
    let state = test_state().await;
    let (_, b) = send(&state, "POST", "/api/sessions", Some(json!({ "name": "Chat" }))).await;
    let sid = body_json(&b)["id"].as_str().unwrap().to_string();
    let (_, b) = send(&state, "POST", "/api/packs", Some(json!({ "name": "Alice" }))).await;
    let pid = body_json(&b)["id"].as_str().unwrap().to_string();

    let (st, _) = send(&state, "PUT", &format!("/api/sessions/{sid}/packs"), Some(json!({ "pack_ids": [pid] }))).await;
    assert_eq!(st, StatusCode::OK);
    let (st, b) = send(&state, "GET", &format!("/api/sessions/{sid}/packs"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b), json!([pid]));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test packs_test session_pack_mounts_roundtrip`
Expected: FAIL — 404 (route missing).

- [ ] **Step 3: Add the handlers** in `shirita-web/src/routes/sessions.rs` (near `set_mounts`, ~line 251):

```rust
#[derive(Deserialize)]
pub struct SetPacks {
    pub pack_ids: Vec<String>,
}

pub async fn set_packs(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<SetPacks>,
) -> Result<StatusCode, StatusCode> {
    if state.storage.get_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    state.storage.set_mounted_packs(&session_id, &body.pack_ids).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn get_packs(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let s = state.storage.get_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(s.mounted_packs))
}
```

- [ ] **Step 4: Register the route.** In `shirita-web/src/lib.rs`, near the `/sessions/{id}/mounts` route (line ~65) add:

```rust
        .route("/sessions/{id}/packs", get(routes::sessions::get_packs).put(routes::sessions::set_packs))
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p shirita-web --test packs_test session_pack_mounts_roundtrip`
Expected: PASS.

- [ ] **Step 6: Full suite + commit**

Run: `cargo test --workspace` — PASS, zero warnings.

```bash
git add shirita-web/src/routes/sessions.rs shirita-web/src/lib.rs shirita-web/tests/packs_test.rs
git commit -m "feat(web): session pack mounting endpoint"
```

---

### Task 4: Create chats with packs — mount + seed variables & greeting

**Files:**
- Modify: `shirita-web/src/routes/sessions.rs` (`CreateSession`, `create_session`, `seed_first_message`, imports)
- Test: `shirita-web/tests/packs_test.rs`

**Interfaces:**
- Consumes: `state::resolve_schema_with_packs` (Plan 2), `Storage::get_pack`, `Session.mounted_packs`.
- Produces: `POST /api/sessions` accepts `pack_ids`; sets `mounted_packs`, seeds `current_state` from template+pack variable schemas, and seeds the opening greeting from mounted packs' (then the template's) `first_message`.

- [ ] **Step 1: Write the failing test** — append to `shirita-web/tests/packs_test.rs`:

```rust
#[tokio::test]
async fn create_session_mounts_packs_and_seeds_pack_variables() {
    let state = test_state().await;
    // a pack declaring a variable + carrying a greeting
    let (_, b) = send(&state, "POST", "/api/packs", Some(json!({
        "name": "Alice",
        "meta": { "variables": [ { "name": "affection", "type": "number", "initial": "5" } ] }
    }))).await;
    let pid = body_json(&b)["id"].as_str().unwrap().to_string();
    let (_, b) = send(&state, "POST", "/api/definitions", Some(json!({
        "type": "first_message", "name": "hello", "content": "Hi, I'm Alice."
    }))).await;
    let gid = body_json(&b)["id"].as_str().unwrap().to_string();
    send(&state, "POST", &format!("/api/packs/{pid}/nodes?owner_kind=pack"),
        Some(json!({ "kind": "ref", "definition_id": gid }))).await;
    // a template (gets content+history)
    let (_, b) = send(&state, "POST", "/api/templates", Some(json!({ "name": "T" }))).await;
    let tid = body_json(&b)["id"].as_str().unwrap().to_string();

    let (st, b) = send(&state, "POST", "/api/sessions", Some(json!({
        "name": "Chat", "template_id": tid, "pack_ids": [pid]
    }))).await;
    assert_eq!(st, StatusCode::OK);
    let s = body_json(&b);
    assert_eq!(s["mounted_packs"], json!([pid]));
    assert_eq!(s["current_state"]["affection"], 5, "pack variable initial seeded");
    let sid = s["id"].as_str().unwrap().to_string();

    // greeting from the pack's first_message was seeded
    let (st, b) = send(&state, "GET", &format!("/api/sessions/{sid}/messages"), None).await;
    assert_eq!(st, StatusCode::OK);
    let msgs = body_json(&b);
    assert!(msgs.as_array().unwrap().iter().any(|m| m["raw_content"].as_str().unwrap_or("").contains("I'm Alice")),
        "pack greeting seeded as a message");
}
```

> The messages GET path / field name (`raw_content`) matches `sessions_test.rs`/`message_tree_test.rs`; if they differ, match those files.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test packs_test create_session_mounts_packs`
Expected: FAIL — `mounted_packs` empty / `affection` absent / no greeting.

- [ ] **Step 3: Update imports** in `shirita-web/src/routes/sessions.rs` (line ~13) — swap `resolve_schema` for `resolve_schema_with_packs`:

```rust
use shirita_core::state::{resolve_schema_with_packs, schema_initials};
```

- [ ] **Step 4: Accept `pack_ids` and seed with packs.** In `CreateSession` (line ~35) add a field:

```rust
    #[serde(default)]
    pub pack_ids: Vec<String>,
```

In `create_session` (lines ~46–57), set the mount + use the pack-aware schema:

```rust
    let mut session = Session::new(body.name);
    session.template_id = body.template_id.clone();
    session.avatar = body.avatar.clone();
    session.mounted_packs = body.pack_ids.clone();
    let template_meta = match &session.template_id {
        Some(tid) => state.storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    let mut pack_metas = Vec::new();
    for pid in &session.mounted_packs {
        if let Ok(Some(p)) = state.storage.get_pack(pid).await {
            pack_metas.push(p.meta);
        }
    }
    let schema = resolve_schema_with_packs(template_meta.as_ref(), &pack_metas, &session.override_config);
    session.current_state = Value::Object(schema_initials(&schema));
    state.storage.create_session(&session).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
```

- [ ] **Step 5: Seed the greeting from packs too.** Replace the node-gathering at the top of `seed_first_message` (lines ~71–93) so it scans mounted packs (in mount order) then the template, taking the first `first_message`:

```rust
async fn seed_first_message(state: &AppState, session: &Session) -> Result<(), StatusCode> {
    // Candidate trees: mounted packs (in mount order), then the template.
    let mut groups: Vec<Vec<PromptNode>> = Vec::new();
    for pid in &session.mounted_packs {
        groups.push(
            state.storage.list_nodes(&OwnerKind::Pack, pid).await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        );
    }
    if let Some(tid) = session.template_id.as_deref() {
        groups.push(
            state.storage.list_nodes(&OwnerKind::Template, tid).await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        );
    }
    let mut greeting: Option<(String, Vec<String>)> = None;
    'outer: for nodes in &groups {
        for n in nodes.iter().filter(|n| n.kind == NodeKind::Ref) {
            if let Some(did) = &n.definition_id {
                if let Ok(Some(def)) = state.storage.get_definition(did).await {
                    if def.def_type == "first_message" {
                        let alts = def
                            .meta
                            .get("alternate_greetings")
                            .and_then(|v| v.as_array())
                            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                            .unwrap_or_default();
                        greeting = Some((def.content.clone(), alts));
                        break 'outer;
                    }
                }
            }
        }
    }
    let Some((first, alts)) = greeting else { return Ok(()) };
```

(Everything from `let mut anchor = …` onward is unchanged.)

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p shirita-web --test packs_test create_session_mounts_packs`
Expected: PASS.

- [ ] **Step 7: Full suite + commit**

Run: `cargo test --workspace` — PASS, zero warnings (the `resolve_schema` import was replaced, not orphaned).

```bash
git add shirita-web/src/routes/sessions.rs shirita-web/tests/packs_test.rs
git commit -m "feat(web): create sessions with mounted packs, seed pack vars + greeting"
```

---

## Self-Review

**Spec coverage (§16.3 slice):** pack CRUD ✓ T1; pack-node endpoints (reused) + content node on template create ✓ T2; session-pack mounting ✓ T3; session creation with packs + variable/greeting seeding ✓ T4.

**Placeholder scan:** none — every step has exact code/commands. Two soft spots (definitions create body shape; messages GET path/field) are guarded with "match the existing test file" notes.

**Type consistency:** `Pack`/`PackIdentity` re-exported (T1) and used in `packs.rs` ✓; pack-node routes reuse `prompt_nodes::{list_nodes,create_node,reorder_nodes}` whose `NodesQuery.owner_kind` parses `pack` via `OwnerKind::from_db` (Plan 1) ✓; `SetPacks.pack_ids` → `set_mounted_packs` (Plan 1) ✓; `resolve_schema_with_packs(Option<&Value>, &[Value], &Value)` (Plan 2) matches the create_session call ✓; `resolve_schema` import removed in T4 → no unused-import warning ✓.

**Re-sliced to Plan 4 (intentional):** pack-aware `resolve_identity` + `GET /sessions/{id}/identity`; `get_state`/display schema including pack vars (`variables.rs`, `sessions.rs` still use `resolve_schema`); startup `ensure_templates_have_content_node` wiring (`shirita-web/src/main.rs` + `shirita-tauri/src/main.rs`) + re-export of `ensure_templates_have_content_node`. **Plan 5:** frontend (Book split, PromptTree select/tag/content, new-chat picks pack+template). **Plan 6:** ST import → Pack.
