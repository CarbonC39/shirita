# Unimplemented features & known gaps

Last updated: 2026-06-19

This document inventories what hasn't been built yet or doesn't meet expectations,
organized by severity.

---

## 🔴 Must-fix before release

### No Docker image (M9)
Web deployment is the primary target, but there is no Dockerfile, no frontend-embedded
binary, and no documented production deployment path.

**Required:**
- Dockerfile with multi-stage build (frontend → Rust binary → slim runtime image)
- `/data` and `/data/assets` volume mounts
- Environment variable configuration (`TOKEN_SECRET`, `DATABASE_PATH`, `ASSETS_DIR`)
- `docker-compose.yml` for quick-start
- CI publish to ghcr.io or Docker Hub

### Tauri CI not triggered
`.github/workflows/desktop.yml` exists but has never been triggered (needs a `v*` tag
or `workflow_dispatch`). Windows (.msi) and macOS (.dmg) builds are completely
unverified. The Linux AppImage build has a known `librsvg` compatibility issue with
`linuxdeploy-plugin-gtk` on Debian trixie.

**Required:**
- Tag a release and trigger the workflow
- Verify Windows and macOS artifacts are functional
- Fix the AppImage build (or document the limitation)

### Desktop GUI has never been visually verified
The Tauri window starts without panics (validated by automated tests), but no human
has ever looked at it. Risk areas:
- SSE streaming renders correctly in WebKit
- Background images and avatars load via the embedded Axum server
- The CSP in `tauri.conf.json` doesn't block any critical resource
- Notification API actually fires in the WebView

---

## 🟡 Feature gaps from roadmap

### No native tool-call support (M5 deferred item)
State updates currently work through `<state_update>` XML tags, which are detected
by regex-scanning the streaming output. OpenAI/Anthropic native `tool_calls` are
not implemented — the system never sends function definitions, and never parses
a tool_calls block from the response.

**Affects:** Dynamic state manipulation (HP, flags, inventory) requires the model
to output HTML tags, which is fragile.

### No summary visualization in the chat UI
Auto-summarization (M6) works server-side: old messages are folded and a summary
is stored. But the frontend never shows:
- Which messages have been summarized / are hidden
- A "manual summarise now" button
- The current summary text

**Affects:** Users can't tell when summarization happens, and can't trigger it
on demand.

### Export only produces native Shirita JSON
`GET /definitions/{id}/export` and `GET /templates/{id}/export` output
`shirita.definition` / `shirita.template` JSON format only. There is no export
to SillyTavern PNG character cards or worldinfo JSON.

### Multi-character name/avatar
The identity system only supports one active avatar and name per session. There is
no concept of "now the assistant speaks as character B" — the avatar, name, and
description are fixed at session start.

### Anthropic provider not end-to-end tested with a real API key
`AnthropicProvider` passes all unit tests, but the stream-chat path has never been
called against a real `api.anthropic.com` endpoint. The adapter's body format,
header structure, and delta parsing may have edge-case bugs.

---

## 🟢 Polish / UX gaps

### Notification API in Tauri
`notify.ts` uses the standard Web `Notification` API. In Tauri's WebKit WebView
this should work, but it hasn't been verified. If it doesn't, a
`tauri-plugin-notification` integration would be needed.

### Notification not configurable per-conversation
The toggle in Settings is global. There's no per-chat mute or per-event filtering
(e.g., notify only on errors or certain keywords).

### Empty-import filter may miss edge-cases
The `persist_defs` filter (`import_export.rs`) skips empty-content definitions
unless they're identity anchors or meta-only types. This works for the common
cases but doesn't handle:
- Definitions with whitespace-only content
- Definitions whose type is unknown (not char/persona/regex_rule/first_message)
  but happen to carry non-empty meta

### An empty `char` definition with no avatar and no name is still created
The unconditional identity-anchor rule (`char`/`persona` is always kept) means a
completely empty char with neither name nor avatar will still be persisted. The
assembly filter makes it harmless (empty bodies are skipped), but it's still in
the database.

### Auto-summarize settings not reflected in the chat
The settings section exists (context window, threshold, keep_recent, instruction)
but changes take effect only on the next summarization cycle. There's no preview,
no "apply now" button, and no indication of the current set values in the chat
header.

### CSS hooks documented only in the editor placeholder
The custom CSS editor placeholder lists available hooks (`.app-chat-column`,
`.app-message[data-role]`, `.app-composer`, `[data-app=shell]`) but there's
no dedicated help page or tooltip explaining what each hook selects.

### Configurable width doesn't apply to Composer
The chat column width (`ui.contentWidth`) is applied to the message list but not
to the composer's inner elements — they still use hardcoded `max-w-[600px]`.

---

## ♻️ Technical debt

| Item | Details |
|------|---------|
| `provider_select.rs` | Two separate async helpers (`setting_str` → removed, `setting_str_storage` → kept) with nearly identical names; the old `setting_str` taking `&AppState` was dead code post Task 5 cleanup |
| `Cargo.toml` license | AGPL-3.0 added but the crate `README.md` files (in `shirita-ui/node_modules/` etc.) are third-party and unaffected |
| Route `mod.rs` | Groups routes in `lib.rs` as builder chains; no middleware-level grouping or version prefix — fine for now but will need organization as routes grow |
| Test coverage | Core: 169 tests. Web: ~45 integration tests. UI: 198 tests. No E2E tests, no provider integration tests with live API keys |
