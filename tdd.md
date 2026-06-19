# Shirita Technical Design Notes

> **Status:** Superseded by per-milestone design documents in `docs/superpowers/specs/`.
> This file retains broad architectural principles for quick reference.

## Architecture

```
             ┌─────────────────┐
             │  shirita-ui     │  Vue 3 + Vite (view layer only)
             │  (Web/Tauri WV) │
             └────────┬────────┘
                      │ HTTP + SSE (web) / embedded Axum (Tauri)
             ┌────────▼────────┐
             │  shirita-web    │  Axum REST + SSE adapter
             │  (bearer auth)  │
             └────────┬────────┘
             ┌────────▼────────┐
             │  shirita-core   │  Domain logic (no framework deps)
             │  ┌────────────┐ │
             │  │ Storage    │◄┤ SQLite (sqlx)
             │  │ Provider   │◄┤ OpenAI / Anthropic / Ollama
             │  │ TokenCounter│◄┤ tiktoken-rs
             │  └────────────┘ │
             └─────────────────┘
             ┌─────────────────┐
             │ shirita-tauri   │  Tauri v2 shell (embeds web)
             └─────────────────┘
```

## Key principles

- **Everything is a `Definition`** — characters, prompts, world entries, regex rules, first messages, tools. Differentiated by `def_type` (`char`, `prompt`, `world`, `regex_rule`, `first_message`, `persona`, `tool`).
- **Copy-on-write** — editing a definition inside a chat applies local overrides; the global library is untouched until explicit promotion.
- **Backend-owned context engineering** — the frontend never counts tokens, assembles prompts, or parses tool calls.
- **Three trait boundaries** — `Storage`, `ModelProvider`, `TokenCounter` keep core testable without I/O.
- **Safe rendering** — no `v-html`. Dynamic content uses template rendering; state updates go through a sandboxed instruction set.

## Data model (SQLite + WAL)

| Table | Key contents |
|-------|-------------|
| `definitions` | `id, type(name), content, meta(JSON)` |
| `chat_sessions` | `id, name, active_leaf_id, current_state(JSON), override_config(JSON)` |
| `messages` | `id, session_id, parent_id, role, raw_content, display_content, is_hidden, snapshot_state(JSON)` |
| `assets` | `id, name, path, kind(avatar|background)` |
| `settings` | Flat key-value store (`key TEXT PK, value TEXT`) |
| `prompt_nodes` | Tree nodes: `id, owner_id, parent_id, kind(ref|folder|history), definition_id, enabled, sort_order, meta(JSON)` |
| `summaries` | `id, session_id, cutoff_message_id, content` |
| `templates` | Loreset containers; nodes are referenced by `owner_id = template.id` |

## Provider resolution

1. Per-source settings (`provider.<source>.api_key`, `provider.<source>.base_url`, `provider.<source>.model`) — set via Settings UI
2. Legacy flat keys (`provider_api_key`, etc.) — migrated into the active source's namespace on first access
3. Environment variables (`PROVIDER`, `OPENAI_API_KEY`, etc.) — fallback for desktop first-launch

See `docs/providers.md` for the full provider matrix and `docs/superpowers/specs/` for per-milestone designs.
