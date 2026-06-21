# M9 — Deployment (Web self-host + Docker + release CI) Design

> Milestone M9 of the Shirita roadmap (`docs/superpowers/specs/2026-06-12-shirita-roadmap-design.md`). M0–M8 done; this is the final roadmap milestone. Desktop (Tauri) installers already ship from M8's `desktop.yml`; M9's real work is making `shirita-web` deliverable as a self-contained, self-hosted Web service.

## 1. Goal & Deliverables

Ship Shirita as a single-user self-hosted Web service while reusing M8's desktop installers.

**Three deliverables:**
1. **Self-contained binary** — `cargo build --release -p shirita-web --features embed-ui` produces one binary with the entire Vue frontend embedded; it depends on nothing external except its runtime data volume.
2. **Docker image** — multi-stage build (node builds the frontend → rust builds the embedded binary → slim runtime), mounting `/data` (DB) + `/data/assets` (media). Published to GitHub Container Registry.
3. **Release CI** — a `docker.yml` workflow builds and pushes the image to `ghcr.io` on `v*` tags, alongside the existing `desktop.yml`. One `v*` tag = one complete release (Docker image + three-platform desktop installers).

**Done = a deliverable Docker image on ghcr.io + (M8's) desktop installers.**

## 2. Decisions (approved during brainstorming)

- **Embedding = `rust-embed` behind a `embed-ui` cargo feature (default off).** Dev/test/CI backend builds never require a prior `npm run build`; only release/Docker builds opt in with `--features embed-ui`. Dev still serves the frontend from `vite dev` (:5173, proxying `/api`).
- **Frontend asset path = `/static`, user media stays at `/assets`.** Vite `build.assetsDir = 'static'` repoints frontend chunks to `dist/static/*` (served at `/static/*`), avoiding collision with the existing `/assets/*` user-media route. Zero backend/DB/data churn.
- **Browser auth = server injects the token into `index.html` at serve time.** Mirrors Tauri's `window.__SHIRITA_RUNTIME__` injection. The token becomes a same-origin convenience guard, **not** multi-user auth; self-hosters protect the deployment at the network layer (reverse-proxy auth / VPN / firewall), documented in deploy docs. No login UI.
- **CI scope = release only.** M9 adds the Docker publish workflow (→ ghcr.io). A push/PR test-gate workflow is explicitly out of scope (separate future work).

## 3. Routing & Serving

A single Axum `app()` keeps these routes, with this match precedence (most specific first; Axum matches explicit routes/nests before the fallback):

```
/api/*      → protected API nest (Bearer middleware)         [unchanged]
/health     → public health check                            [unchanged]
/assets/*   → user-media ServeDir (config.assets_dir)        [unchanged]
/static/*   → embedded frontend chunks       [new, embed-ui]
/  + rest   → embedded index.html (token-injected)  [new, embed-ui; SPA fallback]
```

**`embed-ui` ON** (release/Docker):
- `GET /static/{*path}` serves the embedded `static/<path>` with the right `Content-Type` (by extension) and a long-lived cache header (`Cache-Control: public, max-age=31536000, immutable` — chunk filenames are content-hashed).
- `GET /` and the router **fallback** serve the embedded `index.html` with the runtime script injected (see §4). The fallback makes Vue Router history-mode deep links (`/chat/:id`, `/book`, `/settings`) load directly.
- **Fallback guard:** the fallback handler returns `404` for any path beginning `/api`, `/assets`, `/static`, or `/health` (so an unknown API path never returns HTML), and `index.html` otherwise. (Axum already routes those prefixes to their owners before the fallback; the guard is an explicit safety net.)

**`embed-ui` OFF** (dev/test, default): behavior is exactly today's — `/` serves the existing verification page (`routes::index::index`), no `/static`, no fallback. The Vue app is served by `vite dev`.

The `embed-ui`-gated routes are added in `app()` via `#[cfg(feature = "embed-ui")]` so the default build is byte-for-byte unchanged.

```rust
#[cfg(feature = "embed-ui")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../shirita-ui/dist"]   // relative to shirita-web/Cargo.toml
struct Ui;
```

## 4. Token Injection

The embedded `index.html` is served with a runtime config spliced in. Implemented as a **pure, always-compiled** function so it is unit-testable without the feature or a built `dist/`:

```rust
/// Splice `window.__SHIRITA_RUNTIME__ = { base, token }` into <head> so the
/// same-origin browser authenticates without a baked-in build-time token.
pub fn inject_runtime(html: &str, token: &str) -> String {
    let rt = serde_json::json!({ "base": "", "token": token });
    let tag = format!("<script>window.__SHIRITA_RUNTIME__={};</script>", rt);
    match html.rfind("</head>") {
        Some(i) => { let mut s = html.to_string(); s.insert_str(i, &tag); s }
        None => format!("{tag}{html}"),   // fallback: prepend if no </head>
    }
}
```

- `serde_json` produces a JS-safe, escaped literal for arbitrary `TOKEN_SECRET` values.
- `base: ""` → `client.ts`'s `RT?.base ?? …` yields `""` → requests hit `/api/...` same-origin. `client.ts` already reads `RT?.token`; **no frontend code change**.
- The serving handler reads `state.config.token_secret`, runs `inject_runtime`, returns `Html(..)`.

## 5. Frontend / Build Changes

- `shirita-ui/vite.config.ts`: add `build: { assetsDir: 'static' }`. Chunks emit to `dist/static/*`; `index.html` references `/static/...`; `base` stays `/`.
- **Build order (release/Docker):** rust-embed reads `dist/` at compile time, so `npm run build` MUST precede `cargo build --release --features embed-ui`. The Dockerfile and deploy docs fix this order. With the feature off, `dist/` is never read, so dev/test are unaffected.
- No other frontend changes.

## 6. Dockerfile, Runtime, Compose

**Multi-stage `Dockerfile`** at repo root:

1. **frontend** (`node:20-slim`): `npm ci` + `npm run build` in `shirita-ui/` → `dist/`.
2. **builder** (`rust:1-bookworm`): copy the workspace + `COPY --from=frontend …/dist ./shirita-ui/dist`, then `cargo build --release -p shirita-web --features embed-ui` → `/app/target/release/shirita-web`.
3. **runtime** (`debian:bookworm-slim`, `apt-get install -y ca-certificates curl`): copy only the binary. `ca-certificates` is required for outbound HTTPS to LLM providers (OpenAI/Anthropic); `curl` backs the `HEALTHCHECK` (slim ships neither curl nor wget).

Runtime image specifics:
- **Non-root:** create + run as an unprivileged user (`appuser`); `/data` owned by it.
- **ENV defaults (container):** `BIND_ADDR=0.0.0.0:8787` (must be `0.0.0.0` to be reachable; host default stays `127.0.0.1:8787`), `DATABASE_PATH=/data/shirita.db`, `ASSETS_DIR=/data/assets`.
- `EXPOSE 8787`; `VOLUME ["/data"]`.
- **HEALTHCHECK:** `curl -fsS http://127.0.0.1:8787/health` (public, unauthenticated).
- **`TOKEN_SECRET` is required** — `Config::from_env` already fails fast when unset. The container does **not** auto-generate one (explicit + predictable). Deploy docs show `openssl rand -hex 32`.
- **Entrypoint:** the binary; on missing `TOKEN_SECRET` it exits non-zero with the existing clear error.

**`.dockerignore`** (keep build context small, avoid leaking host artifacts): `target/`, `**/node_modules/`, `shirita-ui/dist/`, `.git/`, `*.db`, `*.db-*`, `assets/`, `gen/`.

**`docker-compose.yml`** example (in docs) — single service, one named volume → `/data`, port map, `TOKEN_SECRET` + provider env (`PROVIDER`, `OPENAI_API_KEY`, …) passed through.

**Deploy docs** `docs/deploy.md`: build/run, env reference, the network-layer-hardening note (token is a same-origin guard; put a reverse proxy / VPN in front for internet exposure), and the `v*`-tag release flow.

## 7. Release CI

New `.github/workflows/docker.yml`:
- **Triggers:** `workflow_dispatch` + `push` tags `v*` (parity with `desktop.yml`).
- **Job:** checkout → `docker/login-action` to `ghcr.io` using the built-in `GITHUB_TOKEN` (no extra secrets) → `docker/build-push-action` building the root `Dockerfile` → push `ghcr.io/<owner>/shirita:<tag>` + `:latest`.
- **Permissions:** `packages: write`, `contents: read`.

Desktop stays on M8's `desktop.yml` (unchanged). A `v*` tag fires both → image on ghcr.io + installer artifacts.

## 8. Desktop

No new desktop code. M8 already produces unsigned AppImage/deb/dmg/msi via `desktop.yml` on `v*`. M9 only **documents** the unified release: tag `vX.Y.Z` → `docker.yml` + `desktop.yml` both run. (Optionally attaching installers to a GitHub Release instead of workflow artifacts is a nice-to-have, not core M9.)

## 9. Environment Config Reference

Already implemented (`Config::from_env`, `provider_from_env`): `TOKEN_SECRET` (required), `DATABASE_PATH` (default `shirita.db`), `ASSETS_DIR` (default `./assets`), `BIND_ADDR` (default `127.0.0.1:8787`), `PROVIDER` / `OPENAI_BASE_URL` / `OPENAI_API_KEY` / `OPENAI_MODEL`. M9 adds **no new env vars** — it only sets container-appropriate defaults in the Dockerfile. The `embed-ui` feature is a build-time flag, not runtime config.

## 10. Testing Strategy

- **Pure functions (always compiled, run by default `cargo test`):**
  - `inject_runtime(html, token)` — asserts the script tag is spliced before `</head>`, the token is JSON-escaped, `base` is `""`, and the no-`</head>` fallback prepends.
  - The fallback prefix rule (`is_api_like(path) -> bool` or equivalent) — asserts `/api/x`, `/assets/x`, `/static/x`, `/health` are excluded and arbitrary app routes are not.
- **Feature-gated integration test** (`#[cfg(feature = "embed-ui")]`, runs only when built `--features embed-ui`, i.e. when `dist/` exists — exercised in the Docker/CI build): `app()` with the feature → `GET /` returns HTML containing `__SHIRITA_RUNTIME__`; `GET /static/<known>` has a JS/CSS content-type; `GET /api/<unknown>` → 404 (not HTML); `GET /book` (deep link) → the SPA `index.html`.
- **Frontend build assertion:** after `npm run build`, `dist/static/` exists and `dist/index.html` references `/static/` (a small check; also implicitly verified by the Docker build succeeding).
- **Docker smoke (CI/manual):** run the image with a `TOKEN_SECRET`; `curl /health` → ok; `curl /` → HTML with the runtime script.

Default `cargo test --workspace` (feature off) stays green without any frontend build, preserving the dev loop.

## 11. Out of Scope

Push/PR test-gate CI; login-screen auth / multi-user; TLS termination (delegated to a reverse proxy); auto-generated `TOKEN_SECRET`; attaching installers to GitHub Releases; cinema mode (point 3, parked); any new runtime env var.

## 12. Decomposition into Plans

- **Plan 1 — Serve the UI from the binary (Rust + Vite).** `embed-ui` feature + `rust-embed`; `/static` route; index serving with `inject_runtime`; SPA fallback + prefix guard; Vite `assetsDir: 'static'`. Pure-function unit tests (always run) + feature-gated integration test.
- **Plan 2 — Containerize + release CI + docs.** Multi-stage `Dockerfile`, `.dockerignore`, `docker-compose.yml` example, `docs/deploy.md`; `docker.yml` (build + push ghcr.io on `v*`). Docker smoke step.

Two plans; Plan 2 depends on Plan 1 (the image builds the embedded binary).
