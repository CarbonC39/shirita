# UX & Settings Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the 11 UX/settings fixes from the 2026-06-19 design spec — a polish round across appearance, settings, import, prompt assembly, and the media library.

**Architecture:** Vertical slices, one issue per task (backend-before-frontend where a UI task consumes new backend behavior). Rust core/web changes are TDD'd with `cargo test`; Vue changes with Vitest. Settings remain a flat KV store; provider config moves to per-source namespaced keys. The media library gains a `kind` column. No new heavy dependencies.

**Tech Stack:** Rust (Axum, sqlx runtime API, fancy-regex), Vue 3 + Pinia + vue-i18n + Tailwind v4, Vitest, `cargo test`.

## Global Constraints

- Code comments and git commit messages in **English**.
- i18n parity: every new user-facing string added to `en` (source schema) **and** `zh-Hans`, `zh-Hant`, `ja`. A parity test enforces this.
- No new heavy dependencies; the image cropper and CSS injection are in-house.
- Migrations are numbered `.sql` files in `shirita-core/migrations/`, run by `sqlx::migrate!`. Next free number is **0016**.
- Backend reads response-token limit from setting key `provider_max_tokens` (3 call sites). The UI must write that same key.
- Settings storage API: `get_setting(key) -> Option<Value>`, `set_setting(key, &Value)`, `list_settings() -> Vec<(String, Value)>`.
- Identity single-source-of-truth: never drop a `char`/`persona` definition that carries an avatar or a non-empty name.

---

## File Structure

**Backend (Rust):**
- `shirita-core/src/assembly.rs` — add `strip_comments`; wire into render; drop empty bodies (#7, #6).
- `shirita-core/src/adapters/charcard.rs` — keep char anchor only (already nonempty-guarded elsewhere) (#6 note).
- `shirita-web/src/routes/import_export.rs` — skip empty non-anchor defs in `persist_defs` (#6).
- `shirita-core/src/summarize.rs` — honor `summarize.enabled` (#11).
- `shirita-core/src/conversation.rs`, `shirita-web/src/routes/provider.rs` — read per-source provider keys (#10).
- `shirita-core/migrations/0016_assets_kind.sql`, `shirita-core/src/models/asset.rs`, `shirita-core/src/storage/{mod.rs,sqlite.rs}`, `shirita-web/src/routes/assets.rs` — asset `kind` (#1).

**Frontend (Vue):**
- `shirita-ui/src/styles.css` — `@layer base` anchor reset; CSS hooks (#8, #4).
- `shirita-ui/src/main.ts`, `shirita-ui/src/composables/useCustomCss.ts` — pre-mount CSS injection (#4).
- `shirita-ui/src/views/SettingsView.vue` — per-source provider form, context section, notify toggle, width control, max-tokens key fix (#10, #11, #9, #5).
- `shirita-ui/src/stores/{settings.ts,ui.ts,media.ts,chat.ts}` — per-source helpers, width state, kind-aware media, notification fire (#10, #5, #1, #9).
- `shirita-ui/src/components/{AssetPicker.vue,AvatarPicker.vue,ImageCropper.vue}` — kind filter + cropper (#1).
- `shirita-ui/src/components/DefinitionEditor.vue`, `shirita-ui/src/views/BookView.vue` — rename buttons (#2).
- `shirita-ui/src/views/ChatView.vue` — center panel + width (#3, #5).
- `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` — new strings.

---

## Task 1: Fix nav-icon cascade (#8)

**Files:**
- Modify: `shirita-ui/src/styles.css:47-50`
- Test: `shirita-ui/src/components/AppShell.test.ts`

**Interfaces:**
- Produces: nothing consumed by other tasks. Establishes the `@layer base` reset that makes `text-*` utilities work on `<a>`.

- [ ] **Step 1: Write the failing test**

Add to `AppShell.test.ts` (mount needs a router; follow the file's existing setup — reuse its router/pinia helper). Assert the inactive nav links carry `text-muted` and the active one carries `text-ink`:

```ts
it('marks only the active section icon as ink, others muted', async () => {
  const w = await mountShell('/book') // existing helper that mounts at a route
  const links = w.findAll('header nav a')
  const cls = links.map((l) => l.attributes('class') || '')
  expect(cls.filter((c) => c.includes('text-ink'))).toHaveLength(1)
  expect(cls.filter((c) => c.includes('text-muted'))).toHaveLength(2)
})
```

If `AppShell.test.ts` has no mount helper, add one mirroring the file's existing pattern (createRouter + memory history + createPinia). This test asserts class presence (the cascade bug is visual, but the regression we lock is "exactly one active class").

- [ ] **Step 2: Run test to verify current behavior**

Run: `cd shirita-ui && npx vitest run src/components/AppShell.test.ts`
Expected: PASS for class assertion (classes are already correct in markup) — the real defect is CSS cascade, not classes. Proceed to the cascade fix and verify visually.

- [ ] **Step 3: Wrap the anchor reset in `@layer base`**

In `styles.css`, replace lines 47-50:

```css
a {
  color: inherit;
  text-decoration: none;
}
```

with:

```css
@layer base {
  a {
    color: inherit;
    text-decoration: none;
  }
}
```

- [ ] **Step 4: Verify the build + colors apply**

Run: `cd shirita-ui && npx vitest run src/components/AppShell.test.ts && npm run build`
Expected: tests PASS, build succeeds. Manually confirm in the running app that inactive nav icons are gray and only the active one is ink, and that markdown links render in the primary color again.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/styles.css shirita-ui/src/components/AppShell.test.ts
git commit -m "fix(ui): move anchor color reset into @layer base so text-* utils win

Unlayered a{color:inherit} outranked Tailwind utility layer, making all
nav icons (and md-links) inherit body ink. (#8)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: Comment stripping in assembly (#7)

**Files:**
- Modify: `shirita-core/src/assembly.rs` (add `strip_comments`; wrap the 3 render sites)
- Test: `shirita-core/src/assembly.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Produces: `pub fn strip_comments(input: &str) -> String`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `assembly.rs`:

```rust
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core assembly::tests::strip_comments`
Expected: FAIL — `strip_comments` not found.

- [ ] **Step 3: Implement `strip_comments`**

Add near `render_vars` in `assembly.rs`:

```rust
/// Strip `{{// ... }}` authoring comments. Linear scan (no regex → no
/// catastrophic backtracking): find each `{{//`, drop through the next `}}`.
/// A comment alone on its line takes the line's leading whitespace and one
/// trailing newline with it. An unterminated `{{//` strips to end of input.
pub fn strip_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{//") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let end = after.find("}}").map(|e| e + 2).unwrap_or(after.len());
        rest = &after[end..];

        // Whole-line comment: emitted text ends at a line start (only ws since
        // the last newline) and what's left begins with optional spaces + '\n'.
        let line_start = out.rsplit_once('\n').map(|(_, t)| t).unwrap_or(&out[..]);
        if line_start.trim().is_empty() {
            let cut = out.len() - line_start.len();
            out.truncate(cut);
            let trimmed = rest.trim_start_matches([' ', '\t']);
            rest = trimmed.strip_prefix('\n').unwrap_or(trimmed);
        }
    }
    out.push_str(rest);
    out
}
```

- [ ] **Step 4: Wire it into assembly before `render_vars`**

In `assemble_from_nodes`, wrap each of the three render sites so comments are stripped first. Change:
- Entry content (`assembly.rs:389`): `content: render_vars(&strip_comments(&effective_def_content(def, overrides)), state),`
- `resolve` body (`assembly.rs:410`): `let body = render_vars(&strip_comments(&effective_def_content(def, overrides)), state);`
- depth insert (`assembly.rs:487`): `let content = render_vars(&strip_comments(&effective_def_content(d, overrides)), state);`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p shirita-core assembly`
Expected: PASS (new tests + existing assembly tests).

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/assembly.rs
git commit -m "feat(core): strip {{// }} comments during prompt assembly (#7)

Linear scan, no backtracking; runs before {{var}} render; definition/
template content only.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: Empty-content handling on import + assembly (#6)

**Files:**
- Modify: `shirita-core/src/assembly.rs` (folder/ref empty-body filter)
- Modify: `shirita-web/src/routes/import_export.rs` (`persist_defs` empty skip)
- Test: `shirita-core/src/assembly.rs`, `shirita-web/tests/import_empty_test.rs` (new)

**Interfaces:**
- Consumes: `strip_comments` (Task 2) already wired.
- Produces: assembly no longer emits empty `<tag>` segments; `persist_defs` drops empty non-anchor, non-meta-only defs.

- [ ] **Step 1: Write the failing assembly test**

Add to `assembly.rs` tests:

```rust
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core assembly::tests::empty_active_child`
Expected: FAIL — current output includes the empty body, producing `<char>\n\nreal body\n</char>`.

- [ ] **Step 3: Drop empty bodies in the folder join + skip empty root refs**

In `assemble_from_nodes`, the `Folder` branch (`assembly.rs:440`):

```rust
let bodies: Vec<String> = children
    .iter()
    .filter_map(|c| resolve(c))
    .filter(|b| !b.trim().is_empty())
    .collect();
```

And the root `Ref` branch (`assembly.rs:450-458`): skip when the resolved content is blank:

```rust
NodeKind::Ref => {
    if let Some(content) = resolve(root) {
        if content.trim().is_empty() { continue; }
        segments.push(PromptSegment {
            placement,
            content,
            source: root.definition_id.clone().unwrap_or_default(),
        });
    }
}
```

- [ ] **Step 4: Run assembly tests**

Run: `cargo test -p shirita-core assembly`
Expected: PASS.

- [ ] **Step 5: Write the failing import test**

Create `shirita-web/tests/import_empty_test.rs` (harness mirrors `tests/regex_scopes_test.rs`):

```rust
use std::sync::Arc;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;
use shirita_core::{Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("import_empty.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into(),
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client() }
}

#[tokio::test]
async fn worldinfo_import_skips_empty_entries() {
    let state = test_state().await;
    let storage = state.storage.clone();
    // a world book with one real entry and one empty-content entry
    let body = serde_json::json!({
        "entries": [
            { "keys": ["zion"], "comment": "Zion", "content": "Last city" },
            { "keys": ["void"], "comment": "Void", "content": "" }
        ]
    });
    let req = Request::builder()
        .method("POST").uri("/api/import/worldinfo")
        .header("authorization", "Bearer secret-token")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap();
    let res = app(state).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let defs = storage.list_definitions().await.unwrap();
    assert!(defs.iter().any(|d| d.content == "Last city"));
    assert!(!defs.iter().any(|d| d.content.trim().is_empty()));
}
```

Verify the route path/method against `shirita-web/src/routes/mod.rs` (`import_worldinfo`); adjust the URI if it differs.

- [ ] **Step 6: Run to verify it fails**

Run: `cargo test -p shirita-web --test import_empty_test`
Expected: FAIL — the empty entry is persisted.

- [ ] **Step 7: Skip empty non-anchor defs in `persist_defs`**

In `import_export.rs::persist_defs`, at the top of the `for mut d in defs` loop:

```rust
for mut d in defs {
    // Skip empty content-bearing defs (cleanliness), but never drop identity
    // anchors (char/persona with a name or avatar) or meta-only types whose
    // payload lives in meta (regex_rule/first_message).
    let meta_only = matches!(d.def_type.as_str(), "regex_rule" | "first_message");
    let is_anchor = matches!(d.def_type.as_str(), "char" | "persona")
        && (!d.name.trim().is_empty()
            || d.meta.get("avatar").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false));
    if d.content.trim().is_empty() && !meta_only && !is_anchor {
        continue;
    }
    // ... existing dedup/persist logic unchanged ...
```

Leave `persist_loreset` unchanged: charcard fields are already `nonempty`-guarded and the char anchor is intentionally retained (its empty content is now harmless via the assembly filter).

- [ ] **Step 8: Run import + assembly tests**

Run: `cargo test -p shirita-web --test import_empty_test && cargo test -p shirita-core assembly`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add shirita-core/src/assembly.rs shirita-web/src/routes/import_export.rs shirita-web/tests/import_empty_test.rs
git commit -m "fix(core,web): drop empty bodies in assembly; skip empty non-anchor imports (#6)

Identity anchors (char/persona w/ name or avatar) are always kept; empty
content no longer emits an empty <tag>.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: Auto-summarize enable toggle + backend (#11 backend)

**Files:**
- Modify: `shirita-core/src/summarize.rs` (honor `summarize.enabled`)
- Test: `shirita-core/src/summarize.rs`

**Interfaces:**
- Produces: `summarize::run` early-returns when `summarize.enabled == false`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `summarize.rs` (reuse `long_session`, `temp_storage`, `FixedProvider`):

```rust
#[tokio::test]
async fn run_skipped_when_disabled() {
    let storage = Arc::new(temp_storage().await);
    let (session, _leaf) = long_session(&storage, 14).await;
    storage.set_setting("context.window", &json!(50)).await.unwrap(); // would normally fold
    storage.set_setting("summarize.enabled", &json!(false)).await.unwrap();
    let provider: Arc<dyn ModelProvider> = Arc::new(FixedProvider("X".into()));
    let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    run(storage.clone(), provider, counter, "m".into(), session.id.clone()).await;
    assert!(storage.list_summaries(&session.id).await.unwrap().is_empty());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core summarize::tests::run_skipped_when_disabled`
Expected: FAIL — a summary is still produced.

- [ ] **Step 3: Honor the flag**

In `summarize.rs::run`, after loading the session/path (before computing the window), add an early opt-out. Add a helper next to `setting_usize`:

```rust
async fn setting_bool(s: &dyn Storage, key: &str, default: bool) -> bool {
    s.get_setting(key).await.ok().flatten().and_then(|v| v.as_bool()).unwrap_or(default)
}
```

and near the top of `run`, right after the `if path.is_empty() { return; }` guard:

```rust
if !setting_bool(storage.as_ref(), "summarize.enabled", true).await {
    return;
}
```

(Default `true` preserves today's always-on behavior.)

- [ ] **Step 4: Run summarize tests**

Run: `cargo test -p shirita-core summarize`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/summarize.rs
git commit -m "feat(core): honor summarize.enabled setting (default on) (#11)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 5: Per-provider settings storage (#10 backend)

**Files:**
- Modify: `shirita-web/src/routes/provider.rs` (read namespaced keys, migrate legacy)
- Modify: `shirita-core/src/conversation.rs` (active source's base_url/api_key/model for the send path)
- Test: `shirita-web/tests/provider_isolation_test.rs` (new)

**Interfaces:**
- Produces: provider config read from `provider.<source>.base_url|api_key|model`, with one-time migration of legacy flat `provider_base_url|provider_api_key|provider_model` into the active source's namespace.
- Consumes (by Task 7 UI): the same key scheme.

- [ ] **Step 1: Add a shared resolver helper**

Create a small resolver used by both `provider.rs` and the send path. Add to `shirita-web/src/routes/provider.rs`:

```rust
use shirita_core::Storage;

/// Read a setting as a String, or None.
async fn setting_str(storage: &dyn Storage, key: &str) -> Option<String> {
    storage.get_setting(key).await.ok().flatten()
        .and_then(|v| v.as_str().map(str::to_string))
}

/// Resolve the active provider's config from per-source namespaced settings,
/// migrating legacy flat keys into the active namespace on first use.
/// Returns (source, base_url, api_key, model).
pub async fn resolve_provider_config(storage: &dyn Storage) -> (String, String, String, String) {
    let source = setting_str(storage, "provider_source").await.unwrap_or_else(|| "openai".into());

    // Migrate legacy flat keys once: if the namespaced key is unset but a flat
    // one exists, copy it over (the flat key is left as a harmless remnant).
    for (flat, field) in [
        ("provider_base_url", "base_url"),
        ("provider_api_key", "api_key"),
        ("provider_model", "model"),
    ] {
        let ns = format!("provider.{source}.{field}");
        if setting_str(storage, &ns).await.is_none() {
            if let Some(v) = setting_str(storage, flat).await {
                let _ = storage.set_setting(&ns, &serde_json::json!(v)).await;
            }
        }
    }

    let base_url = setting_str(storage, &format!("provider.{source}.base_url")).await
        .unwrap_or_else(|| default_base_url(&source).into());
    let api_key = setting_str(storage, &format!("provider.{source}.api_key")).await.unwrap_or_default();
    let model = setting_str(storage, &format!("provider.{source}.model")).await
        .unwrap_or_else(|| "gpt-4o".into());
    (source, base_url, api_key, model)
}
```

- [ ] **Step 2: Write the failing isolation test**

Create `shirita-web/tests/provider_isolation_test.rs` (harness as in Task 3):

```rust
// ... same test_state() helper ...

#[tokio::test]
async fn provider_config_is_per_source() {
    let state = test_state().await;
    let s = state.storage.clone();
    s.set_setting("provider_source", &serde_json::json!("openai")).await.unwrap();
    s.set_setting("provider.openai.api_key", &serde_json::json!("KEY_A")).await.unwrap();
    s.set_setting("provider.anthropic.api_key", &serde_json::json!("KEY_B")).await.unwrap();

    let (_src, _url, key, _model) =
        shirita_web::routes::provider::resolve_provider_config(s.as_ref()).await;
    assert_eq!(key, "KEY_A");

    s.set_setting("provider_source", &serde_json::json!("anthropic")).await.unwrap();
    let (_src, _url, key2, _model) =
        shirita_web::routes::provider::resolve_provider_config(s.as_ref()).await;
    assert_eq!(key2, "KEY_B"); // switching source does not lose the other's key
}
```

Ensure `pub mod provider;` is reachable as `shirita_web::routes::provider` (check `shirita-web/src/routes/mod.rs` and `lib.rs`; make the module/function `pub` if needed).

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p shirita-web --test provider_isolation_test`
Expected: FAIL — `resolve_provider_config` not found / not public.

- [ ] **Step 4: Use the resolver in the provider routes + send path**

In `provider.rs::test_connection` and `list_models`, replace the four per-key `get_setting` reads with:

```rust
let (source, base_url, api_key, model) = resolve_provider_config(state.storage.as_ref()).await;
```

In `conversation.rs`, where the send path currently reads `provider_base_url`/`provider_api_key`/`provider_source`/`provider_model` to build the provider, switch to the same resolver (import it or replicate the namespaced reads). Keep `provider_max_tokens` as-is (global; Task 7 fixes the UI key).

- [ ] **Step 5: Run the test + existing provider/web tests**

Run: `cargo test -p shirita-web`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/provider.rs shirita-core/src/conversation.rs shirita-web/tests/provider_isolation_test.rs
git commit -m "feat(web,core): per-source provider config with legacy migration (#10)

Each provider source keeps its own base_url/api_key/model; switching source
no longer clobbers the others.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 6: Asset `kind` column + API (#1 backend)

**Files:**
- Create: `shirita-core/migrations/0016_assets_kind.sql`
- Modify: `shirita-core/src/models/asset.rs`, `shirita-core/src/storage/{mod.rs,sqlite.rs}`, `shirita-web/src/routes/assets.rs`, `shirita-web/src/routes/import_export.rs` (`save_png_asset` → kind=avatar)
- Test: `shirita-core/src/storage/sqlite.rs` (extend `assets_crud_roundtrip`)

**Interfaces:**
- Produces: `Asset { id, name, path, kind, created_at }`; `list_assets(kind: Option<&str>)`; upload accepts a `kind`; `asset_json` includes `kind`. `kind ∈ {"avatar","background"}`, default `"background"`.

- [ ] **Step 1: Add the migration**

Create `shirita-core/migrations/0016_assets_kind.sql`:

```sql
-- Split the media library by kind: avatars vs backgrounds. Existing rows
-- default to 'background' (they were used for the app background).
ALTER TABLE assets ADD COLUMN kind TEXT NOT NULL DEFAULT 'background';
```

- [ ] **Step 2: Write the failing storage test**

In `sqlite.rs` tests, add:

```rust
#[tokio::test]
async fn assets_filtered_by_kind() {
    let storage = temp_storage().await; // existing helper in this module
    let mut av = Asset::new("face", "a.png"); av.kind = "avatar".into();
    let bg = Asset::new("scene", "b.png"); // defaults to background
    storage.create_asset(&av).await.unwrap();
    storage.create_asset(&bg).await.unwrap();
    assert_eq!(storage.list_assets(Some("avatar")).await.unwrap().len(), 1);
    assert_eq!(storage.list_assets(Some("background")).await.unwrap().len(), 1);
    assert_eq!(storage.list_assets(None).await.unwrap().len(), 2);
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p shirita-core storage::sqlite::tests::assets_filtered_by_kind`
Expected: FAIL — `Asset` has no `kind`; `list_assets` takes no arg.

- [ ] **Step 4: Add `kind` to the model + storage**

`models/asset.rs`: add `pub kind: String,` to the struct; in `Asset::new` set `kind: "background".into()`.

`storage/mod.rs`: change the trait method to `async fn list_assets(&self, kind: Option<&str>) -> Result<Vec<Asset>>;`.

`storage/sqlite.rs`:
- `create_asset`: include `kind` — `INSERT INTO assets (id, name, path, kind, created_at) VALUES (?, ?, ?, ?, ?)` binding `asset.kind`.
- `list_assets`: select `kind` and filter:

```rust
async fn list_assets(&self, kind: Option<&str>) -> Result<Vec<Asset>> {
    let rows = match kind {
        Some(k) => sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT id, name, path, kind, created_at FROM assets WHERE kind = ? ORDER BY created_at DESC, id DESC")
            .bind(k).fetch_all(&self.pool).await?,
        None => sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT id, name, path, kind, created_at FROM assets ORDER BY created_at DESC, id DESC")
            .fetch_all(&self.pool).await?,
    };
    Ok(rows.into_iter().map(|(id, name, path, kind, created_at)| Asset { id, name, path, kind, created_at }).collect())
}
```
- `get_asset`: select `kind` too and populate it.

Update all other `list_assets()` call sites to pass `None` (or the right kind).

- [ ] **Step 5: Run storage tests**

Run: `cargo test -p shirita-core storage`
Expected: PASS.

- [ ] **Step 6: Thread `kind` through the HTTP route**

`assets.rs`:
- `asset_json`: add `"kind": a.kind`.
- `list`: accept `Query<ListAssetsQuery { kind: Option<String> }>` and pass `kind.as_deref()` to `list_assets`.
- `upload`: read a `kind` form field (or `?kind=` query); default `"background"`; set `asset.kind` before `create_asset`. Validate it's one of `avatar`/`background`, else default.

`import_export.rs::save_png_asset`: after building the `Asset`, set `asset.kind = "avatar".into()` (character-card PNGs are avatars).

- [ ] **Step 7: Build + test web**

Run: `cargo test -p shirita-web && cargo build -p shirita-web`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add shirita-core/migrations/0016_assets_kind.sql shirita-core/src/models/asset.rs shirita-core/src/storage shirita-web/src/routes/assets.rs shirita-web/src/routes/import_export.rs
git commit -m "feat(core,web): asset kind column (avatar|background) + filtered listing (#1)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 7: Per-provider settings UI + max-tokens key fix (#10 UI, #11 fix)

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue` (provider computeds → active source namespace; `genMaxTokens` → `provider_max_tokens`)
- Test: `shirita-ui/src/views/SettingsView.i18n.test.ts` is i18n-only; add `shirita-ui/src/views/SettingsView.provider.test.ts` (new) if feasible, else cover via store-level test.

**Interfaces:**
- Consumes: backend key scheme `provider.<source>.{base_url,api_key,model}` (Task 5).

- [ ] **Step 1: Write the failing logic test**

The provider computeds are internal to the SFC; test the key mapping via a tiny pure helper extracted from the view. Create `shirita-ui/src/utils/providerKeys.ts`:

```ts
export const providerKey = (source: string, field: 'base_url' | 'api_key' | 'model') =>
  `provider.${source}.${field}`
```

Test `shirita-ui/src/utils/providerKeys.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { providerKey } from './providerKeys'
describe('providerKey', () => {
  it('namespaces per source', () => {
    expect(providerKey('anthropic', 'api_key')).toBe('provider.anthropic.api_key')
    expect(providerKey('openai', 'model')).toBe('provider.openai.model')
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/utils/providerKeys.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the helper + rewrite the SettingsView computeds**

Add `providerKeys.ts` (above). In `SettingsView.vue`:
- Import `providerKey`.
- `providerBaseUrl`/`providerApiKey`/`providerModel` get/set read/write `settings.data[providerKey(providerSource.value, field)]` instead of the flat `provider_base_url` etc.
- `providerSource` setter: on change, **do not** reset base_url to default unconditionally; instead, if the new source has no saved `base_url`, seed it from `defaultBaseUrls[v]`. Keep `provider_source` as the active selection key.
- The autosave watch (lines 246-289) and the models-fetch watch (163-183) save the namespaced keys for the active source (plus `provider_source`).
- Fix max-tokens: `genMaxTokens` get/set use key `provider_max_tokens` (not `gen_max_response_tokens`); update the save patch object key accordingly.

- [ ] **Step 4: Run frontend tests + typecheck**

Run: `cd shirita-ui && npx vitest run src/utils/providerKeys.test.ts && npx vue-tsc --noEmit`
Expected: PASS / no type errors.

- [ ] **Step 5: Manual verification**

Start the app: set OpenAI key, switch source to Anthropic, set its key, switch back to OpenAI → the OpenAI key is still present. Set Max response tokens, send a message, confirm the provider request carries it (network tab / server log).

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/utils/providerKeys.ts shirita-ui/src/utils/providerKeys.test.ts shirita-ui/src/views/SettingsView.vue
git commit -m "feat(ui): per-source provider settings; write provider_max_tokens (#10, #11)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 8: Context / auto-summarize settings section (#11 UI)

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue` (new Context section)
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts`
- Test: existing `SettingsView.i18n.test.ts` parity test covers the new keys.

**Interfaces:**
- Consumes: backend setting keys `summarize.enabled`, `context.window`, `context.threshold`, `context.keep_recent`, `summarize.instruction` (Task 4 + existing summarize.rs).

- [ ] **Step 1: Add i18n keys (en source first)**

In `locales/en.ts` under `settings`, add:

```ts
context: 'Context',
autoSummarize: 'Auto-summarize long chats',
contextWindow: 'Context window (tokens)',
contextThreshold: 'Summarize at (% of window)',
keepRecent: 'Keep recent messages',
summarizeInstruction: 'Summary instruction',
```

Mirror the same keys (translated) into `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts`.

- [ ] **Step 2: Run the parity test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/views/SettingsView.i18n.test.ts`
Expected: depends on the test — if it checks key parity across locales, it FAILS until all four have the keys; make them match, then it PASSES. (If it only checks en, it already passes — proceed.)

- [ ] **Step 3: Add the Context section to SettingsView**

After the Generation section (`SettingsView.vue:571`), add a new `<section>` with:
- A `ToggleSwitch` bound to a `summarizeEnabled` computed (key `summarize.enabled`, default true).
- Number `field` for `context.window` (computed `contextWindow`, default 200000).
- A `SliderControl` (0–1, step 0.01) or percentage number for `context.threshold` (default 0.8).
- Number `field` for `context.keep_recent` (default 10).
- A `textarea` (+ existing `FullscreenEditor` pattern) for `summarize.instruction`.

Add computeds mirroring the existing `get`/`set` pattern, e.g.:

```ts
const summarizeEnabled = computed({
  get: () => (get('summarize.enabled') as boolean) ?? true,
  set: (v: boolean) => set('summarize.enabled', v),
})
const contextWindow = computed({
  get: () => (get('context.window') as number) ?? 200000,
  set: (v: number) => set('context.window', v),
})
const contextThreshold = computed({
  get: () => (get('context.threshold') as number) ?? 0.8,
  set: (v: number) => set('context.threshold', v),
})
const keepRecent = computed({
  get: () => (get('context.keep_recent') as number) ?? 10,
  set: (v: number) => set('context.keep_recent', v),
})
const summarizeInstruction = computed({
  get: () => (get('summarize.instruction') as string) || '',
  set: (v: string) => set('summarize.instruction', v),
})
```

Add these five to the autosave `watch` dependency array and the `settings.save({...})` patch object so they persist.

- [ ] **Step 4: Run frontend tests + typecheck**

Run: `cd shirita-ui && npx vitest run src/views/SettingsView.i18n.test.ts && npx vue-tsc --noEmit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/SettingsView.vue shirita-ui/src/locales
git commit -m "feat(ui): expose context/auto-summarize settings (#11)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 9: System/browser notifications (#9)

**Files:**
- Modify: `shirita-ui/src/stores/chat.ts` (fire on `done` when hidden)
- Create: `shirita-ui/src/utils/notify.ts`
- Modify: `shirita-ui/src/views/SettingsView.vue` + locales (toggle)
- Test: `shirita-ui/src/utils/notify.test.ts` (new)

**Interfaces:**
- Produces: `notifyReplyDone(title: string, body: string)` — fires a Notification only when permission granted and `document.visibilityState === 'hidden'`. Setting key `notify_enabled` (bool, default false).

- [ ] **Step 1: Write the failing test**

`shirita-ui/src/utils/notify.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { notifyReplyDone } from './notify'

describe('notifyReplyDone', () => {
  beforeEach(() => { vi.restoreAllMocks() })

  it('does not notify when the tab is visible', () => {
    const ctor = vi.fn()
    vi.stubGlobal('Notification', Object.assign(ctor, { permission: 'granted' }))
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true })
    notifyReplyDone('t', 'b')
    expect(ctor).not.toHaveBeenCalled()
  })

  it('notifies when hidden and permitted', () => {
    const ctor = vi.fn()
    vi.stubGlobal('Notification', Object.assign(ctor, { permission: 'granted' }))
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true })
    notifyReplyDone('Neo', 'hello')
    expect(ctor).toHaveBeenCalledWith('Neo', expect.objectContaining({ body: 'hello' }))
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/utils/notify.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `notify.ts`**

```ts
// Fire a desktop notification only when the tab is backgrounded and the user
// has granted permission. Guarded so SSR/no-Notification environments no-op.
export function notifyReplyDone(title: string, body: string): void {
  if (typeof Notification === 'undefined') return
  if (Notification.permission !== 'granted') return
  if (document.visibilityState !== 'hidden') return
  try { new Notification(title, { body }) } catch { /* ignore */ }
}

export async function ensureNotifyPermission(): Promise<boolean> {
  if (typeof Notification === 'undefined') return false
  if (Notification.permission === 'granted') return true
  if (Notification.permission === 'denied') return false
  return (await Notification.requestPermission()) === 'granted'
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd shirita-ui && npx vitest run src/utils/notify.test.ts`
Expected: PASS.

- [ ] **Step 5: Fire from the chat store + add the toggle**

In `stores/chat.ts::consume`, in the `done` branch (`chat.ts:52`), after `await loadMessages(sessionId)`, fire when enabled:

```ts
else if (event.type === 'done') {
  streamingText.value = ''
  await loadMessages(sessionId)
  const settings = useSettingsStore()
  if (settings.data.notify_enabled) {
    const last = messages.value[messages.value.length - 1]
    notifyReplyDone(document.title || 'Shirita', (last?.content ?? '').slice(0, 120))
  }
}
```

Import `notifyReplyDone` and `useSettingsStore` at the top of `chat.ts`. (If importing the settings store into the chat store risks a cycle, read `notify_enabled` from `localStorage`/a lightweight source instead — verify no circular import at build.)

In `SettingsView.vue`, add a toggle bound to a `notifyEnabled` computed (key `notify_enabled`, default false); on enabling, call `ensureNotifyPermission()` and revert the toggle if denied. Add it to the autosave watch + patch. Add locale keys `settings.notifications` / `settings.notifyReplies` to all four locales.

- [ ] **Step 6: Run tests + typecheck**

Run: `cd shirita-ui && npx vitest run && npx vue-tsc --noEmit`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/utils/notify.ts shirita-ui/src/utils/notify.test.ts shirita-ui/src/stores/chat.ts shirita-ui/src/views/SettingsView.vue shirita-ui/src/locales
git commit -m "feat(ui): opt-in desktop notifications on reply done while tab hidden (#9)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 10: Configurable center width (#5)

**Files:**
- Modify: `shirita-ui/src/stores/ui.ts` (cached `contentWidth`)
- Modify: `shirita-ui/src/views/ChatView.vue:118`, `shirita-ui/src/views/BookView.vue` (container max-width)
- Modify: `shirita-ui/src/views/SettingsView.vue` + locales (width control)
- Test: `shirita-ui/src/stores/ui.test.ts`

**Interfaces:**
- Produces: `ui.contentWidth` (number px, default 760) + `ui.setContentWidth(n)`; mirrored to setting `appearance_content_width`.

- [ ] **Step 1: Write the failing store test**

Add to `stores/ui.test.ts`:

```ts
it('defaults content width to 760 and persists changes', () => {
  setActivePinia(createPinia())
  const ui = useUiStore()
  expect(ui.contentWidth).toBe(760)
  ui.setContentWidth(900)
  expect(ui.contentWidth).toBe(900)
  expect(localStorage.getItem('ui.contentWidth')).toBe('900')
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/stores/ui.test.ts`
Expected: FAIL — `contentWidth` undefined.

- [ ] **Step 3: Add width state to the ui store**

In `stores/ui.ts` state: `contentWidth: Number(localStorage.getItem('ui.contentWidth')) || 760,`. Action:

```ts
setContentWidth(px: number) {
  this.contentWidth = px
  localStorage.setItem('ui.contentWidth', String(px))
},
```

- [ ] **Step 4: Apply the width + add the setting control**

- `ChatView.vue:118`: replace `max-w-[600px]` with an inline bound style `:style="{ maxWidth: ui.contentWidth + 'px' }"` (import `useUiStore`).
- `BookView.vue`: the top-level content container's `max-w-[...]` → same bound style.
- `SettingsView.vue`: in Appearance, add a `SliderControl` (min 560, max 1100, step 20) bound to a computed that reads/writes `ui.contentWidth` via `ui.setContentWidth` and mirrors to setting `appearance_content_width` (save on change, like `onBackgroundChange`). On `onMounted`, if `settings.data.appearance_content_width` is a number, `ui.setContentWidth(it)` (server is source of truth, mirrors the background pattern at `SettingsView.vue:219-221`).
- Add locale key `settings.contentWidth` to all four locales.

- [ ] **Step 5: Run tests + typecheck**

Run: `cd shirita-ui && npx vitest run src/stores/ui.test.ts && npx vue-tsc --noEmit`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/stores/ui.ts shirita-ui/src/views/ChatView.vue shirita-ui/src/views/BookView.vue shirita-ui/src/views/SettingsView.vue shirita-ui/src/locales
git commit -m "feat(ui): configurable center column width (default 760px) (#5)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 11: Custom CSS injection + hooks, no FOUC (#4)

**Files:**
- Create: `shirita-ui/src/composables/useCustomCss.ts`
- Modify: `shirita-ui/src/main.ts` (pre-mount inject), `shirita-ui/src/App.vue` (call composable)
- Modify: structural elements for stable hooks: `AppShell.vue`, `ChatView.vue`, `Composer.vue`, `MessageItem.vue`
- Test: `shirita-ui/src/composables/useCustomCss.test.ts` (new)

**Interfaces:**
- Produces: a `<style id="user-custom-css">` element kept in sync with the `custom_css` setting; localStorage cache key `ui.customCss`. Documented hooks: `.app-chat-column`, `.app-message[data-role]`, `.app-composer`, `data-app="shell"`.

- [ ] **Step 1: Write the failing test**

`shirita-ui/src/composables/useCustomCss.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { applyCustomCss } from './useCustomCss'

describe('applyCustomCss', () => {
  beforeEach(() => { document.head.innerHTML = ''; localStorage.clear() })

  it('creates a single style element and updates its text', () => {
    applyCustomCss('.app-chat-column { color: red }')
    const el = document.getElementById('user-custom-css') as HTMLStyleElement
    expect(el).toBeTruthy()
    expect(el.textContent).toContain('color: red')
    applyCustomCss('.app-composer { color: blue }')
    expect(document.querySelectorAll('#user-custom-css')).toHaveLength(1) // reused, not duplicated
    expect(document.getElementById('user-custom-css')!.textContent).toContain('blue')
  })

  it('caches to localStorage', () => {
    applyCustomCss('.x{}')
    expect(localStorage.getItem('ui.customCss')).toBe('.x{}')
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/composables/useCustomCss.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the composable**

```ts
import { watch } from 'vue'
import { useSettingsStore } from '../stores/settings'

const STYLE_ID = 'user-custom-css'
const CACHE_KEY = 'ui.customCss'

// Idempotently set the custom-CSS <style> text and refresh the cache. Exported
// for direct use at boot (before mount) and for tests.
export function applyCustomCss(css: string): void {
  let el = document.getElementById(STYLE_ID) as HTMLStyleElement | null
  if (!el) {
    el = document.createElement('style')
    el.id = STYLE_ID
    document.head.appendChild(el)
  }
  el.textContent = css
  try { localStorage.setItem(CACHE_KEY, css) } catch { /* ignore */ }
}

// Paint immediately from the localStorage cache (call before app.mount to avoid
// FOUC), then reconcile with the server value once settings load.
export function bootCustomCss(): void {
  applyCustomCss(localStorage.getItem(CACHE_KEY) || '')
}

export function useCustomCss(): void {
  const settings = useSettingsStore()
  watch(
    () => settings.data.custom_css,
    (css) => { if (typeof css === 'string') applyCustomCss(css) },
    { immediate: true },
  )
}
```

- [ ] **Step 4: Wire boot + composable**

In `main.ts`, before `app.mount(...)`, call `bootCustomCss()` (import it). In `App.vue` `<script setup>`, call `useCustomCss()` next to `useTheme()`.

- [ ] **Step 5: Add stable hooks**

Add stable classes/attributes (additive — keep existing utility classes):
- `AppShell.vue` root `<div class="h-full flex flex-col">` → add `data-app="shell"`.
- `ChatView.vue` container (`:118`) → add `class="app-chat-column ..."` (keep existing classes).
- `Composer.vue` root → add `class="app-composer ..."`.
- `MessageItem.vue` root → add `class="app-message ..."` and `:data-role="role"` (user/assistant).

Document hooks in the CSS editor placeholder: change `placeholder="/* custom CSS */"` (`SettingsView.vue:643`) to list the hooks, e.g. `/* hooks: .app-chat-column .app-message[data-role] .app-composer [data-app=shell] */`.

- [ ] **Step 6: Run tests + typecheck + build**

Run: `cd shirita-ui && npx vitest run src/composables/useCustomCss.test.ts && npx vue-tsc --noEmit && npm run build`
Expected: PASS. Manually: set custom CSS targeting `.app-chat-column`, refresh — it applies with no flash of default styling.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/composables/useCustomCss.ts shirita-ui/src/composables/useCustomCss.test.ts shirita-ui/src/main.ts shirita-ui/src/App.vue shirita-ui/src/components/AppShell.vue shirita-ui/src/views/ChatView.vue shirita-ui/src/components/Composer.vue shirita-ui/src/components/MessageItem.vue shirita-ui/src/views/SettingsView.vue
git commit -m "feat(ui): inject custom CSS pre-mount with stable hooks, no FOUC (#4)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 12: Center panel over background (#3)

**Files:**
- Modify: `shirita-ui/src/views/ChatView.vue:118` (panel background)
- Modify: `shirita-ui/src/components/AppShell.vue:36-39` (lighten global scrim)
- Test: visual; add a render assertion to an existing ChatView test if present, else manual.

**Interfaces:**
- Consumes: `.app-chat-column` hook (Task 11) — reuse it for the panel.

- [ ] **Step 1: Add the panel background to the chat column**

In `ChatView.vue:118`, add a semi-opaque surface panel behind the column (keep `app-chat-column` from Task 11):

```html
<div class="app-chat-column flex flex-col h-full mx-auto bg-surface/85" :style="{ maxWidth: ui.contentWidth + 'px' }">
```

(Replaces the prior `max-w-[600px] bg-cover bg-center` — the per-column background image is dropped in favor of the readability panel; the app-wide image still shows in the gutters via AppShell.)

- [ ] **Step 2: Lighten the global scrim**

In `AppShell.vue:38`, since the chat column now carries its own panel, reduce the full-screen scrim so the background reads in the gutters. Change `bg-surface/75` to a lighter tint, e.g. `bg-surface/30`:

```html
<div class="fixed inset-0 -z-10 bg-surface/30" />
```

- [ ] **Step 3: Verify in both themes**

Run: `cd shirita-ui && npm run build`
Expected: build OK. Manually set a background image, open a chat: text sits on the ~85% panel (readable), the image shows in the side gutters and faintly through; check light and dark.

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/views/ChatView.vue shirita-ui/src/components/AppShell.vue
git commit -m "feat(ui): semi-opaque chat panel over background for readability (#3)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 13: Split avatar/background libraries + avatar cropper (#1 frontend)

**Files:**
- Create: `shirita-ui/src/components/ImageCropper.vue`
- Modify: `shirita-ui/src/stores/media.ts` (kind-aware), `shirita-ui/src/api/client.ts` (`kind` in type + calls)
- Modify: `shirita-ui/src/components/AssetPicker.vue` (kind prop + crop on avatar), `shirita-ui/src/components/AvatarPicker.vue` (`kind="avatar"`), `shirita-ui/src/views/SettingsView.vue` (background picker `kind="background"`)
- Test: `shirita-ui/src/stores/media.test.ts` (new), `shirita-ui/src/components/AssetPicker.test.ts` (new)

**Interfaces:**
- Consumes: backend `kind` (Task 6). `Asset` type gains `kind`. `listAssets(kind?)`, `uploadAsset(file, kind)`.

- [ ] **Step 1: Update the API client**

`client.ts`: extend `Asset` interface with `kind: string`. `listAssets(kind?: 'avatar' | 'background')` → `apiGet('/assets' + (kind ? '?kind=' + kind : ''))`. `uploadAsset(file, kind)` → append `kind` to the FormData (`form.append('kind', kind)`).

- [ ] **Step 2: Write the failing media-store test**

`shirita-ui/src/stores/media.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useMediaStore } from './media'
import * as client from '../api/client'

describe('media store by kind', () => {
  beforeEach(() => { setActivePinia(createPinia()); vi.restoreAllMocks() })

  it('caches assets per kind', async () => {
    vi.spyOn(client, 'listAssets').mockImplementation(async (kind?: string) =>
      kind === 'avatar' ? [{ id: 'a', name: 'f', path: 'a.png', url: '/assets/a.png', kind: 'avatar' }]
                        : [{ id: 'b', name: 's', path: 'b.png', url: '/assets/b.png', kind: 'background' }])
    const m = useMediaStore()
    await m.load('avatar')
    await m.load('background')
    expect(m.byKind('avatar').map((a) => a.id)).toEqual(['a'])
    expect(m.byKind('background').map((a) => a.id)).toEqual(['b'])
  })
})
```

- [ ] **Step 3: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/stores/media.test.ts`
Expected: FAIL — `byKind`/kinded `load` not present.

- [ ] **Step 4: Make the media store kind-aware**

Rewrite `media.ts` to key assets by kind:

```ts
export const useMediaStore = defineStore('media', () => {
  const assets = ref<Record<string, Asset[]>>({ avatar: [], background: [] })
  const loaded = ref<Record<string, boolean>>({ avatar: false, background: false })
  const error = ref<string | null>(null)

  function byKind(kind: 'avatar' | 'background') { return assets.value[kind] ?? [] }

  async function load(kind: 'avatar' | 'background', force = false) {
    if (loaded.value[kind] && !force) return
    try { assets.value[kind] = await listAssets(kind); loaded.value[kind] = true }
    catch (e) { error.value = (e as Error).message }
  }
  async function upload(file: File, kind: 'avatar' | 'background'): Promise<Asset | null> {
    try { const a = await uploadAsset(file, kind); assets.value[kind] = [a, ...byKind(kind)]; return a }
    catch (e) { error.value = (e as Error).message; return null }
  }
  async function rename(id: string, kind: 'avatar' | 'background', name: string) {
    const a = byKind(kind).find((x) => x.id === id)
    const prev = a?.name
    if (a) a.name = name
    try { await renameAsset(id, name) }
    catch (e) { if (a && prev !== undefined) a.name = prev; error.value = (e as Error).message }
  }
  async function remove(id: string, kind: 'avatar' | 'background') {
    try { await deleteAsset(id); assets.value[kind] = byKind(kind).filter((x) => x.id !== id) }
    catch (e) { error.value = (e as Error).message }
  }

  return { assets, loaded, error, byKind, load, upload, rename, remove }
})
```

- [ ] **Step 5: Run to verify it passes**

Run: `cd shirita-ui && npx vitest run src/stores/media.test.ts`
Expected: PASS.

- [ ] **Step 6: Build the cropper**

Create `shirita-ui/src/components/ImageCropper.vue` — a dependency-free square cropper (drag to pan, wheel to zoom, exports 512×512 PNG):

```vue
<script setup lang="ts">
import { ref, onMounted } from 'vue'

const props = defineProps<{ file: File }>()
const emit = defineEmits<{ cropped: [Blob]; cancel: [] }>()

const SIZE = 512
const canvas = ref<HTMLCanvasElement | null>(null)
const img = new Image()
let scale = 1, minScale = 1, offsetX = 0, offsetY = 0
let dragging = false, lastX = 0, lastY = 0

function draw() {
  const c = canvas.value; if (!c) return
  const ctx = c.getContext('2d')!; ctx.clearRect(0, 0, SIZE, SIZE)
  ctx.drawImage(img, offsetX, offsetY, img.width * scale, img.height * scale)
}
function clamp() {
  const w = img.width * scale, h = img.height * scale
  offsetX = Math.min(0, Math.max(SIZE - w, offsetX))
  offsetY = Math.min(0, Math.max(SIZE - h, offsetY))
}
onMounted(() => {
  const url = URL.createObjectURL(props.file)
  img.onload = () => {
    minScale = Math.max(SIZE / img.width, SIZE / img.height)
    scale = minScale
    offsetX = (SIZE - img.width * scale) / 2
    offsetY = (SIZE - img.height * scale) / 2
    draw(); URL.revokeObjectURL(url)
  }
  img.src = url
})
function onWheel(e: WheelEvent) {
  e.preventDefault()
  scale = Math.max(minScale, scale * (e.deltaY < 0 ? 1.05 : 0.95))
  clamp(); draw()
}
function onDown(e: PointerEvent) { dragging = true; lastX = e.clientX; lastY = e.clientY }
function onMove(e: PointerEvent) {
  if (!dragging) return
  offsetX += e.clientX - lastX; offsetY += e.clientY - lastY
  lastX = e.clientX; lastY = e.clientY; clamp(); draw()
}
function onUp() { dragging = false }
function confirmCrop() {
  canvas.value!.toBlob((b) => { if (b) emit('cropped', b) }, 'image/png')
}
</script>

<template>
  <div class="flex flex-col items-center gap-3">
    <canvas
      ref="canvas" :width="512" :height="512"
      class="w-[256px] h-[256px] rounded-full border border-line touch-none cursor-grab"
      @wheel="onWheel" @pointerdown="onDown" @pointermove="onMove" @pointerup="onUp" @pointerleave="onUp"
    />
    <div class="flex gap-2">
      <button class="btn btn-ghost" @click="emit('cancel')">{{ $t('common.cancel') }}</button>
      <button class="btn btn-primary" data-test="cropper-confirm" @click="confirmCrop">{{ $t('common.save') }}</button>
    </div>
  </div>
</template>
```

In `AssetPicker.onFile`, when `kind === 'avatar'`, wrap the cropped Blob back into a File before upload: `new File([blob], 'avatar.png', { type: 'image/png' })`.

- [ ] **Step 7: Wire kind + crop into AssetPicker**

`AssetPicker.vue`: add prop `kind: 'avatar' | 'background'` (default `'background'`). Use `media.byKind(props.kind)` for the gallery and `media.load(props.kind)` on mount. In `onFile`, if `props.kind === 'avatar'`, open `ImageCropper` with the chosen file; on `cropped`, upload the returned Blob (as a `File`) with `kind='avatar'`; backgrounds upload directly. `onDelete`/`rename` pass `props.kind`.
- `AvatarPicker.vue`: pass `kind="avatar"` to its `AssetPicker`.
- `SettingsView.vue` background `AssetPicker` (`:617`): pass `kind="background"`.
- `DefinitionEditor.vue` persona avatar `AssetPicker`: pass `kind="avatar"`.

- [ ] **Step 8: Write the failing AssetPicker test**

`shirita-ui/src/components/AssetPicker.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import AssetPicker from './AssetPicker.vue'
import * as client from '../api/client'

describe('AssetPicker kind', () => {
  beforeEach(() => { setActivePinia(createPinia()); vi.restoreAllMocks() })
  it('only lists assets of its kind', async () => {
    vi.spyOn(client, 'listAssets').mockResolvedValue([
      { id: 'a', name: 'f', path: 'a.png', url: '/assets/a.png', kind: 'avatar' },
    ])
    const w = mount(AssetPicker, { props: { modelValue: '', kind: 'avatar' }, global: { plugins: [createPinia()] } })
    await new Promise((r) => setTimeout(r))
    expect(client.listAssets).toHaveBeenCalledWith('avatar')
  })
})
```

- [ ] **Step 9: Run all frontend tests + typecheck**

Run: `cd shirita-ui && npx vitest run && npx vue-tsc --noEmit`
Expected: PASS. (Update `DefinitionEditor.test.ts` persona-avatar test if the prop change affects it.)

- [ ] **Step 10: Commit**

```bash
git add shirita-ui/src/components/ImageCropper.vue shirita-ui/src/components/AssetPicker.vue shirita-ui/src/components/AssetPicker.test.ts shirita-ui/src/components/AvatarPicker.vue shirita-ui/src/stores/media.ts shirita-ui/src/stores/media.test.ts shirita-ui/src/api/client.ts shirita-ui/src/views/SettingsView.vue shirita-ui/src/components/DefinitionEditor.vue
git commit -m "feat(ui): separate avatar/background libraries by kind + avatar cropper (#1)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 14: Rename buttons for template & definition (#2)

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue:938-967` (template name → display + Rename)
- Modify: `shirita-ui/src/components/DefinitionEditor.vue:110-124` (split search from name; add Rename)
- Modify: locales (Rename label)
- Test: `shirita-ui/src/components/DefinitionEditor.test.ts`

**Interfaces:**
- Produces: definition name is no longer mutated by the search field; a Rename action toggles inline name editing.

- [ ] **Step 1: Write the failing DefinitionEditor test**

Add to `DefinitionEditor.test.ts`:

```ts
describe('DefinitionEditor rename', () => {
  const d = { id: 'd', type: 'char', name: 'Neo', content: '', meta: {} }

  it('typing in the search field does not emit update:name', async () => {
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    await w.get('[data-test="def-search"]').setValue('zi')
    expect(w.emitted('update:name')).toBeFalsy()
  })

  it('Rename reveals an input that emits update:name', async () => {
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    await w.get('[data-test="def-rename"]').trigger('click')
    await w.get('[data-test="def-name-input"]').setValue('Trinity')
    expect(w.emitted('update:name')!.at(-1)).toEqual(['Trinity'])
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts`
Expected: FAIL — `def-search`/`def-rename`/`def-name-input` not present.

- [ ] **Step 3: Split search from name in DefinitionEditor**

In `DefinitionEditor.vue`, replace the merged combobox (`:115-121`):
- The combobox `<input>` becomes a pure **search** field (`data-test="def-search"`), bound to a local `search` ref, `@input` only sets `search`/`open = true` — it no longer emits `update:name`.
- Filtering (`filtered`, `:91`) uses `search` instead of `props.definition.name`.
- Add a name display row: the current `definition.name` as text plus a pencil button (`data-test="def-rename"`). Clicking sets a local `renaming = true`, revealing an `<input data-test="def-name-input">` bound to `definition.name` via `@input` emitting `update:name`; Enter/blur sets `renaming = false`.

- [ ] **Step 4: Template rename button in BookView**

In `BookView.vue:943-950`, replace the always-editable template name input with a display + Rename pattern: show `templateName` as text with a pencil button; clicking reveals the existing input (keep `@change="renameTemplate"` / Enter-to-blur). Reuse a local `renamingTemplate` ref.

- [ ] **Step 5: Add the Rename label to locales**

Add `common.rename` (e.g. `'Rename'`) to `en`, `zh-Hans`, `zh-Hant`, `ja`.

- [ ] **Step 6: Run tests + typecheck**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts && npx vue-tsc --noEmit`
Expected: PASS. Fix any existing DefinitionEditor tests that relied on the combobox doubling as the name field.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/components/DefinitionEditor.vue shirita-ui/src/components/DefinitionEditor.test.ts shirita-ui/src/views/BookView.vue shirita-ui/src/locales
git commit -m "feat(ui): rename buttons for template & definition; split search from name (#2)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Final verification (after all tasks)

- [ ] Backend: `cargo test` (workspace) and `cargo build` pass.
- [ ] Frontend: `cd shirita-ui && npx vitest run`, `npx vue-tsc --noEmit`, `npm run build` pass.
- [ ] i18n parity test passes (all new keys present in all four locales).
- [ ] Manual smoke per the spec's "Testing" section: per-provider isolation, comments stripped, empty import, custom CSS no-FOUC, center panel + width, notifications, kind-split libraries + crop, rename buttons, nav-icon colors.

## Self-review notes

- **Spec coverage:** #1 (Tasks 6, 13), #2 (14), #3 (12), #4 (11), #5 (10), #6 (3), #7 (2), #8 (1), #9 (9), #10 (5, 7), #11 (4, 8 + max-tokens key in 7). All 11 covered.
- **Cross-task types:** `Asset.kind` defined in Task 6 (Rust) and Task 13 (TS) consistently; `provider.<source>.<field>` key scheme shared by Tasks 5 and 7 (`providerKey` helper); `.app-chat-column` defined in Task 11 and reused in Task 12.
