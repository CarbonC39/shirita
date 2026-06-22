# Shirita

**Experimental SillyTavern alternative — Rust + Vue, self-hosted, in development.**

A local-first AI chat backend with a web UI, built as a from-scratch Rust rewrite. Many features work (prompt trees, variables, import/export, branching), but the project is pre-1.0 — expect rough edges, breaking changes, and incomplete documentation. Not yet a daily driver.

- **Self-hosted** — Docker image or static-musl binary, SQLite storage, BYO model API key
- **No telemetry, no cloud, no account required**
- **Development stage** — works for tinkering; not production-ready
- **Dual build target** — web (standalone Axum server, `--features embed-ui`) and desktop (Tauri + embedded Axum)

---

## Architecture

```
shirita/
├── shirita-core/       Domain models, storage (SQLite/sqlx), prompt assembly,
│                       context engine, auto-summarization, regex rules,
│                       provider adapters, variable/state sandbox
├── shirita-web/        Axum REST + SSE layer (bearer auth, CORS, multipart uploads)
├── shirita-ui/         Vue 3 + Vite + Pinia + Tailwind v4 (view layer only)
└── shirita-tauri/      Tauri v2 desktop shell, embeds shirita-web in-process
```

### Design principles

| Principle | How |
|-----------|-----|
| **Everything is a definition** | Characters, prompts, world entries, regex rules, first messages — unified as `Definition` with a type tag |
| **Copy-on-write** | Editing a definition in a chat doesn't touch the global library; diffs are stored per-session |
| **Backend owns context engineering** | The frontend never counts tokens, assembles prompts, or parses tool calls |
| **Three trait boundaries** | `Storage`, `ModelProvider`, `TokenCounter` — core is testable without I/O |
| **Safe rendering** | No `v-html` — dynamic HTML cards use template engines; state updates go through a sandbox |

---

## Features

- **Prompt tree** — hierarchical system prompt builder with folders, containers, and triggers (keyword / random / constant). Each node can reference any definition type
- **Regex rules** — scoped (global or template-level), filtered by target (`ai_output` / `user_input`) and phase (`display` / `prompt`); supports lookaround and backreferences via `fancy-regex`
- **Variables & state** — declare variables with type and initial value on templates; update them mid-conversation via `<state_update>` tags or future native tool calls; per-message snapshots for branching
- **Auto-summarization** — rolling summary that folds older messages when a token threshold is reached; configurable window, threshold, keep-count, and summary instruction
- **Message tree** — branching, forking, editing, and hiding messages. Fork clones the full history to a new session for clean isolation
- **Import / export** — SillyTavern PNG character cards (v2/v3), worldinfo JSON, and chat-completion presets (→ editable templates: prompts imported by enabled/disabled status, `setvar`/`getvar` recognized as variables, cross-node XML bundled into folders); plus Shirita-native template bundles (.json) and pack bundles (.zip), with dedup conflict resolution (skip / overwrite / duplicate)
- **Media library** — uploaded images tagged by kind (`avatar` / `background`), with an in-browser square cropper for avatars
- **i18n** — English, 简体中文, 繁體中文, 日本語
- **Custom CSS** — injected from a live-editable textarea with stable hooks (`.app-chat-column`, `.app-message[data-role]`, `.app-composer`, `[data-app=shell]`); cached in localStorage to prevent FOUC
- **Provider isolation** — each provider source (OpenAI, Anthropic, Ollama, Google, etc.) keeps its own API key, base URL, and model selection — switching never clobbers the others

---

## Quick start

### Docker (recommended)

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

Set the active provider source and its API key/model in Settings → Provider. Each source is isolated — switching from OpenAI to Anthropic preserves both configurations.

### Environment fallback (desktop, when no settings are configured)

| Env | Default | Purpose |
|-----|---------|---------|
| `PROVIDER` | *(empty, =OpenAI compat)* | `anthropic`, `ollama`, or empty |
| `OPENAI_API_KEY` | — | API key (also used for Anthropic) |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Base URL |
| `OPENAI_MODEL` | `gpt-4o` | Default model |
| `ANTHROPIC_BASE_URL` | `https://api.anthropic.com` | Anthropic base (only when `PROVIDER=anthropic`) |
| `OLLAMA_BASE_URL` | `http://localhost:11434/v1` | Ollama base (only when `PROVIDER=ollama`) |

When both env and UI settings are configured, the UI settings win.

---

## Project layout

| Path | Purpose |
|------|---------|
| `shirita-core/src/` | Domain: models, storage, assembly, summarize, state, tokenizer, adapters |
| `shirita-core/migrations/` | SQLite schema migrations (0020 = current) |
| `shirita-web/src/routes/` | Axum route handlers (settings, provider, assets, sessions, chat, regex, etc.) |
| `shirita-ui/src/views/` | Vue page components (Chat, Book, Settings, NewChat, NewChatPrompt) |
| `shirita-ui/src/components/` | Vue shared components (MessageItem, Composer, AssetPicker, PromptTree, etc.) |
| `shirita-ui/src/stores/` | Pinia stores (chat, settings, ui, media, library) |
| `shirita-ui/src/utils/` | Frontend utilities (tree, regex, tokens, notify, providerKeys, etc.) |
| `shirita-tauri/src/` | Tauri bootstrap (embedded Axum server, webview window, graceful shutdown) |

---

## Building for distribution

### Web (Docker)

A single self-contained binary with the UI embedded, packaged as a Docker image. Pushing a `v*` tag publishes `ghcr.io/carbonc39/shirita:<tag>` + `:latest` via `.github/workflows/docker.yml`. See [`docs/deploy.md`](docs/deploy.md) for `docker run` / compose usage.

### Web (standalone binary)

`.github/workflows/web.yml` builds the embedded-UI `shirita-web` binary for Linux (static musl — no glibc dependency), macOS, and Windows on tag push or `workflow_dispatch`, uploaded as per-platform artifacts. Build it locally with:

```bash
npm --prefix shirita-ui run build
cargo build --release -p shirita-web --features embed-ui
```

### Desktop CI packages

`.deb` / `.AppImage` / `.dmg` / `.msi` are built via GitHub Actions (`.github/workflows/desktop.yml`) on tag push or `workflow_dispatch`. Artifacts are **unsigned** — macOS requires right-click → Open, Windows shows SmartScreen warnings.

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

## License

[AGPL-3.0-only](LICENSE) — the strongest copyleft license. If you modify and distribute this software, you must make your changes available under the same license, including network use (the "ASP loophole" is closed).

---

## Roadmap

| Milestone | Status |
|-----------|--------|
| M0 — Foundation (workspace, storage, auth) | ✅ Done |
| M1 — Minimal chat (send/receive, SSE) | ✅ Done |
| M2 — Definition system & assembly | ✅ Done |
| M3 — Frontend (Vue 3) | ✅ Done |
| M4 — Message tree & copy-on-write | ✅ Done |
| M5 — Variables & state sandbox | ✅ Done |
| M6 — Context engineering (summarize, budget) | ✅ Done |
| M7 — Import / export (ST cards, bundles) | ✅ Done |
| M8 — Tauri desktop shell | ✅ Done |
| M9 — Deploy (Docker, CI, release) | ✅ Done |

See `docs/superpowers/specs/` for milestone design documents and `docs/superpowers/plans/` for implementation plans.
