<div align="center">

<img src="shirita-tauri/icons/icon.png" width="120" alt="Shirita logo">

# Shirita

**A modern AI role-playing platform for desktop and self-hosted deployment.**

[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)
&nbsp;![Rust](https://img.shields.io/badge/Rust-1.80%2B-CE422B?logo=rust&logoColor=white)
&nbsp;![Vue 3](https://img.shields.io/badge/Vue-3-42B883?logo=vuedotjs&logoColor=white)
&nbsp;![Tauri 2](https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=black)
&nbsp;![Status](https://img.shields.io/badge/status-pre--1.0-yellow)

</div>

Shirita is an experimental, user-controlled platform for AI role-playing. It combines a Rust runtime with a Vue interface and supports both a Tauri desktop application and a self-hosted web service. Its direction is a composable agent system for characters, knowledge, prompts, state, tools, and text transforms.

Many features already work, but the project is pre-1.0 and is entering a deliberate simplification phase. SillyTavern compatibility has been removed from the main application; the default UI is being rebuilt and core chat behavior is being hardened. Shirita is not yet a daily driver.

- **Desktop and self-hosted are equal targets** — Tauri desktop, Docker, or standalone web binary
- **User-controlled** — SQLite storage, BYO model API key, no required hosted account
- **No telemetry or required third-party cloud service**
- **Development stage** — works for tinkering; not production-ready
- **i18n** — English, 简体中文, 繁體中文, 日本語

See [Product direction](PRODUCT.md) for the project boundaries and [Current development direction](docs/current-direction.md) for the active cleanup plan. Historical milestone documents are archived and are not the current roadmap.

---

## Contents

- [Architecture](#architecture)
- [Features](#features)
- [Quick start](#quick-start)
- [Provider configuration](#provider-configuration)
- [Project layout](#project-layout)
- [Building for distribution](#building-for-distribution)
- [Development](#development)
- [Current direction](#current-direction)
- [License](#license)

---

## Architecture

```
shirita/
├── shirita-core/       Domain models, storage (SQLite/sqlx), prompt assembly,
│                       context engine, auto-summarization, regex rules,
│                       provider adapters, variable/state sandbox, HTML patching,
│                       content hashing, identity resolution, portable import/export
├── shirita-web/        Axum REST + SSE layer (bearer auth, HTTP Basic Auth,
│                       CORS, multipart, embedded static assets)
├── shirita-ui/         Vue 3 + Vite + Pinia + vue-router (view layer only)
└── shirita-tauri/      Tauri v2 desktop shell, embeds shirita-web in-process
```

### Design principles

| Principle | How |
|-----------|-----|
| **Composable content** | Characters, prompts, knowledge, regex rules, and other reusable content can be assembled from registered definitions |
| **Copy-on-write** | Editing a definition in a chat doesn't touch the global library; diffs are stored per-session (materialized node trees, local definitions) |
| **Backend owns context engineering** | The frontend never counts tokens, assembles prompts, or parses tool calls |
| **Three trait boundaries** | `Storage`, `ModelProvider`, `TokenCounter` — core is testable without I/O |
| **Safe rendering** | No `v-html` — dynamic HTML cards use sandboxed iframes; state updates go through a parsing engine |
| **User-controlled deployment** | Desktop and self-hosted deployments share the same core and HTTP boundary |

---

## Features

- **Prompt tree** — hierarchical system prompt builder with folders, containers, and triggers (keyword / random / constant). Each node references any definition type. Node trees are owned by templates, sessions (copy-on-write), or packs
- **Regex rules** — scoped via `is_global` flag (global or template/pack-scoped), filtered by target (`ai_output` / `user_input`) and phase (`display` / `prompt` / `both`); supports lookaround and backreferences via `fancy-regex`
- **Variables & state** — declare variables with type and initial value on `variables` brick definitions; update them mid-conversation via `<state_update>` tags; per-message snapshots for branching; schema resolved from bricks across template and mounted packs
- **HTML cards (panels)** — build live status panels from `html`/`css` definition bricks grouped under `panel`-tagged folders in the node tree; the model updates them via an HTML-patch protocol (SEARCH/REPLACE blocks) applied in the engine; rendered through sandboxed iframes — never `v-html`. Panels can be hidden until a minimum message count is reached (`min_messages`)
- **Auto-summarization** — rolling summary that folds older messages when a token threshold is reached; configurable window, threshold, keep-count, and summary instruction
- **Message tree** — branching, forking, editing, and hiding messages. Fork clones the full history to a new session for clean isolation. Regenerate creates a sibling (swipe-style) rather than overwriting
- **Per-message identity** — each message carries the `$assistant_name` / `$assistant_avatar` / `$user_name` / `$user_avatar` that was active when it was created, so later template/pack changes don't rewrite old messages
- **Import / export** — Shirita-native definition/template bundles (.json) and pack bundles (.zip), with dedup conflict resolution (skip / overwrite / duplicate). The unified `/api/import` accepts only these native formats; legacy SillyTavern formats are rejected
- **Media library** — uploaded images tagged by kind (`avatar` / `background`), with an in-browser square cropper for avatars; content-addressed dedup via SHA-256 hashing
- **Composer attachments** — attach images to chat messages (resolved as data URLs in the prompt)
- **i18n** — English, 简体中文, 繁體中文, 日本語 (vue-i18n v10, locale switcher in settings)
- **Custom CSS** — injected from a live-editable textarea; cached in localStorage to prevent FOUC. See [Custom CSS hooks](#custom-css-hooks)
- **Provider isolation** — each provider source (OpenAI, Anthropic, Ollama, Google, OpenRouter, Mistral, DeepSeek, Groq, xAI, Cohere, Together, Perplexity) keeps its own API key, base URL, and model selection — switching never clobbers the others. Model listing endpoint normalizes vendor-specific responses
- **Book (library) UI** — manage templates, packs, and definitions in a unified book view with stack-based drill-down navigation (BookNavigator), section components (TemplateSection, PackSection, DefinitionSection), and local-override editing
- **Desktop notifications** — Tauri plugin sends native OS notifications
- **HTTP Basic Auth** — optional outer auth layer for public deployments, gating the entire app (UI HTML/JS + API)
- **PWA support** — mobile/PWA icons and manifest for install-to-homescreen

### Custom CSS hooks

Custom CSS is injected from Settings → Appearance → Custom CSS and cached in
localStorage to prevent a flash of unstyled content. The **supported
compatibility surface** is these stable hooks — internal Tailwind utility
classes are **not** a compatibility API and may change between releases.

| Hook | What it targets |
|------|-----------------|
| `[data-app="shell"]` / `.app-shell` | the application shell root |
| `.app-topbar` | the single compact top navigation bar |
| `.app-chat-column` | the chat workspace column |
| `.app-chat-bar` | the compact chat header row (back, identity, details trigger) |
| `.app-transcript-region` | the transcript wrapper (never scrolls itself) |
| `.app-transcript-scroller` | MessageList's scroller — the only transcript scroll owner |
| `.app-composer-region` | the composer region |
| `.app-composer` / `.app-composer-textarea` | the composer surface and its textarea |
| `.app-message[data-role]` | a message row (`user` / `assistant`) |
| `.app-message-action-sheet` | the viewport-level mobile message action sheet |
| `.app-chat-details` | the session-information drawer overlay |

---

## Quick start

### Self-hosted with Docker

```bash
export TOKEN_SECRET=$(openssl rand -hex 32)
docker run -d --name shirita -p 8787:8787 \
  -e TOKEN_SECRET="$TOKEN_SECRET" \
  -e PROVIDER=openai -e OPENAI_API_KEY=sk-... \
  -v shirita-data:/data \
  ghcr.io/carbonc39/shirita:latest
```

Open `http://localhost:8787`. See [`docs/deploy.md`](docs/deploy.md) for compose, env reference, and hardening notes.

### From source (development)

Requires Rust 1.80+ and Node.js 20+.

```bash
# Terminal 1 — backend
TOKEN_SECRET=dev cargo run -p shirita-web

# Terminal 2 — frontend
npm --prefix shirita-ui run dev
```

Open `http://localhost:5173`. The API secret is `dev`.

### Desktop (Tauri)

**Linux (Debian 13 / Bookworm):**

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  librsvg2-dev libayatana-appindicator3-dev patchelf
cargo install tauri-cli --version "^2" --locked

# Dev mode (starts Vite + desktop window)
cargo tauri dev

# Production build
npm --prefix shirita-ui run build
cargo tauri build --bundles deb
```

**macOS / Windows:** Install Tauri prerequisites per the [Tauri v2 guide](https://v2.tauri.app/start/prerequisites/), then `cargo tauri dev`.

> **Note:** Production build doesn't use `beforeBuildCommand` — CWD differs between local and CI. Always `npm run build` first, then `cargo tauri build`. `beforeDevCommand` is kept for `tauri dev`.

---

## Provider configuration

Set the active provider source and its API key/model in **Settings → Provider**. Each source is isolated — switching from OpenAI to Anthropic preserves both configurations. Supported sources: `openai`, `anthropic`, `ollama`, `google`, `openrouter`, `mistral`, `deepseek`, `groq`, `xai`, `cohere`, `together`, `perplexity`.

### Environment fallback (desktop, when no settings are configured)

| Env | Default | Purpose |
|-----|---------|---------|
| `PROVIDER` | *(empty, = OpenAI compat)* | `anthropic`, `ollama`, or empty |
| `OPENAI_API_KEY` | — | API key (also used for Anthropic) |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Base URL |
| `OPENAI_MODEL` | `gpt-4o-mini` | Default model |
| `ANTHROPIC_BASE_URL` | `https://api.anthropic.com` | Anthropic base (only when `PROVIDER=anthropic`) |
| `OLLAMA_BASE_URL` | `http://localhost:11434/v1` | Ollama base (only when `PROVIDER=ollama`) |
| `HTTP_AUTH_USER` | — | HTTP Basic Auth username (both user + pass required) |
| `HTTP_AUTH_PASS` | — | HTTP Basic Auth password |

When both env and UI settings are configured, the UI settings win.

### HTTP Basic Auth

For public deployments, set both `HTTP_AUTH_USER` and `HTTP_AUTH_PASS` to gate the entire app (UI + API) behind a browser-native login dialog. Setting only one has no effect.

---

## Project layout

| Path | Purpose |
|------|---------|
| `shirita-core/src/` | Domain: models, storage, assembly, summarize, state, tokenizer, panels, HTML patching, hashing, identity, attachments |
| `shirita-core/migrations/` | Numbered SQLite schema migrations |
| `shirita-web/src/routes/` | Axum route handlers (settings, provider, assets, sessions, chat, regex, variables, local overrides, export, etc.) |
| `shirita-ui/src/views/` | Vue page components (Chat, Book, Settings, NewChat, Home) |
| `shirita-ui/src/components/` | Vue shared components (MessageItem, Composer, AssetPicker, PromptTree, PackEditor, DefinitionEditor, BookNavigator, VariablesEditor, PanelView, etc.) |
| `shirita-ui/src/components/book/` | Book-specific components (BookNavigator, DefinitionSection, PackSection, TemplateSection, SessionTemplateRoot) |
| `shirita-ui/src/stores/` | Pinia stores (chat, sessions, library, media, settings, ui) |
| `shirita-ui/src/composables/` | Vue composables (useTheme, useCustomCss, useDefinitionOps, usePackOps, useTemplateOps, useImportExport, useLocalOverrides, useTreeEditor, useToast) |
| `shirita-ui/src/utils/` | Frontend utilities (tree, regex, tokens, notify, providerKeys, markdown, thinking, clone, panel, time) |
| `shirita-ui/src/api/` | HTTP client, TypeScript types, model catalog |
| `shirita-ui/src/locales/` | i18n locales (en, zh-Hans, zh-Hant, ja) |
| `shirita-tauri/src/` | Tauri bootstrap (embedded Axum server, webview window, graceful shutdown) |

---

## Building for distribution

### Web (Docker)

A single self-contained binary with the UI embedded, packaged as a Docker image. Pushing a `v*` tag publishes `ghcr.io/carbonc39/shirita:<tag>` + `:latest` via [`.github/workflows/docker.yml`](.github/workflows/docker.yml). See [`docs/deploy.md`](docs/deploy.md) for `docker run` / compose usage.

### Web (standalone binary)

[`.github/workflows/web.yml`](.github/workflows/web.yml) builds the embedded-UI `shirita-web` binary for Linux (static musl — no glibc dependency), macOS, and Windows on tag push or `workflow_dispatch`, uploaded as per-platform artifacts. Build it locally with:

```bash
npm --prefix shirita-ui run build
cargo build --release -p shirita-web --features embed-ui
```

### Desktop CI packages

`.deb` / `.AppImage` / `.dmg` / `.msi` are built via GitHub Actions ([`.github/workflows/desktop.yml`](.github/workflows/desktop.yml)) on tag push or `workflow_dispatch`. Artifacts are **unsigned** — macOS requires right-click → Open, Windows shows SmartScreen warnings.

---

## Development

### Running tests

```bash
# Backend (Rust)
cargo test --workspace

# Frontend (Vue)
npm --prefix shirita-ui run test        # vitest
npm --prefix shirita-ui run typecheck   # vue-tsc
npm --prefix shirita-ui run build       # vite build
```

### Code conventions

- Comments and commit messages in **English**
- TDD: failing test → implement → passing test → commit
- No `v-html` anywhere in the frontend
- Each migration file is a numbered `.sql` in `shirita-core/migrations/`

---

## Current direction

Development is now focused on reducing maintenance cost and clarifying Shirita's identity before adding more features.

1. Document the product boundary and keep current documentation authoritative.
2. Fix only the chat correctness and recovery bugs that materially affect use.
3. Removed SillyTavern compatibility from the main application, retaining generally useful capabilities such as regex text transforms and message branching.
4. Rebuild the default UI around a simpler, mobile-friendly layout.
5. Normalize state changes and other agent capabilities around registered tools in a later phase.

Prompt composition remains intentionally out of scope for the current cleanup. The existing composable approach is retained while its future redesign is deferred.

See [Current development direction](docs/current-direction.md) for scope and sequencing. The old milestone specs and implementation plans are preserved under [`docs/archive/superpowers/`](docs/archive/superpowers/) as historical context only.

---

## License

[AGPL-3.0-only](LICENSE)
