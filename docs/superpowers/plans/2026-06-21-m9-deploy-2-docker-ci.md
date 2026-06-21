# M9 Deploy — Plan 2: Containerize + release CI + docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Package the embedded `shirita-web` binary (Plan 1) into a small, hardened Docker image and publish it to GitHub Container Registry on `v*` tags, with a compose example and deploy docs. The image survives an empty host bind-mount at `/data` (root entrypoint chowns then drops to an unprivileged user via gosu), and CI builds stay fast via cargo-chef + GitHub Actions layer cache.

**Architecture:** A root `Dockerfile` with four build concerns kept in separate cacheable stages — frontend (`npm`), cargo-chef planner, cargo-chef cook+build (`-p shirita-web --features embed-ui`), and a slim `debian` runtime. `deploy/entrypoint.sh` fixes `/data` ownership as root then `exec gosu appuser`. `.github/workflows/docker.yml` builds with buildx + `cache-to/from: type=gha` and pushes `ghcr.io/<owner>/shirita`.

**Tech Stack:** Docker (BuildKit), cargo-chef, `debian:bookworm-slim`, gosu, GitHub Actions (`docker/*` actions, GHCR).

## Global Constraints

- Image name: `ghcr.io/carbonc39/shirita` (owner slug lowercased). CI derives tags from the pushed `v*` tag + `latest`.
- Builder compiles **only `shirita-web`** (`-p shirita-web`) — never `shirita-tauri` (its gtk/webkit system deps aren't in the build image; `-p` scopes them out).
- Build order: the frontend stage produces `dist/`, copied into the builder before `cargo build --features embed-ui` (rust-embed reads `dist/` at compile time — Plan 1).
- `TOKEN_SECRET` is required; the container never auto-generates it. Container ENV: `BIND_ADDR=0.0.0.0:8787`, `DATABASE_PATH=/data/shirita.db`, `ASSETS_DIR=/data/assets`.
- Comments/docs in English. The image build is the test (a cold build cooks all deps and can take ~10 min; warm CI builds reuse the gha cache).

---

## File Structure

- `Dockerfile` — **new**, repo root. Multi-stage build.
- `deploy/entrypoint.sh` — **new.** Root chown `/data` → `gosu appuser shirita-web`.
- `.dockerignore` — **new.** Trim build context.
- `docker-compose.yml` — **new**, repo root. Single-service example.
- `docs/deploy.md` — **new.** Build/run, env reference, hardening note, release flow.
- `.github/workflows/docker.yml` — **new.** Build + push to GHCR on `v*`.

---

### Task 1: Dockerfile + entrypoint + .dockerignore

**Files:**
- Create: `Dockerfile`, `deploy/entrypoint.sh`, `.dockerignore`

**Interfaces:**
- Consumes: Plan 1's `--features embed-ui` build; `Config::from_env` env vars; `/health`.
- Produces: an image whose `ENTRYPOINT` runs the unprivileged server serving the embedded UI.

- [ ] **Step 1: Write `.dockerignore`**

Create `.dockerignore`:

```
target/
**/node_modules/
shirita-ui/dist/
.git/
*.db
*.db-*
/assets/
gen/
```

- [ ] **Step 2: Write the entrypoint**

Create `deploy/entrypoint.sh`:

```sh
#!/bin/sh
set -e
# An empty host bind-mount at /data lands root:root; make it writable by the
# unprivileged runtime user, then drop privileges. A no-op (|| true) when the
# caller already passed --user.
chown -R appuser:appuser /data 2>/dev/null || true
exec gosu appuser shirita-web "$@"
```

- [ ] **Step 3: Write the Dockerfile**

Create `Dockerfile`:

```dockerfile
# syntax=docker/dockerfile:1

# ---- frontend: build the Vue app → dist/ (with /static chunks, Plan 1) ----
FROM node:20-slim AS frontend
WORKDIR /ui
COPY shirita-ui/package.json shirita-ui/package-lock.json ./
RUN npm ci
COPY shirita-ui/ ./
RUN npm run build

# ---- chef: shared base carrying cargo-chef ----
FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# ---- planner: derive the dependency recipe from the workspace ----
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- builder: cook deps (cached layer), then build the app with UI embedded ----
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Compiles ONLY shirita-web's dependencies — invalidated solely by Cargo.lock.
RUN cargo chef cook --release -p shirita-web --features embed-ui --recipe-path recipe.json
COPY . .
COPY --from=frontend /ui/dist ./shirita-ui/dist
RUN cargo build --release -p shirita-web --features embed-ui

# ---- runtime: slim image with the single binary ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl gosu \
 && rm -rf /var/lib/apt/lists/*
RUN useradd --system --create-home --uid 10001 appuser \
 && mkdir -p /data && chown appuser:appuser /data
COPY --from=builder /app/target/release/shirita-web /usr/local/bin/shirita-web
COPY deploy/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
ENV BIND_ADDR=0.0.0.0:8787 \
    DATABASE_PATH=/data/shirita.db \
    ASSETS_DIR=/data/assets
EXPOSE 8787
VOLUME ["/data"]
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8787/health || exit 1
ENTRYPOINT ["/entrypoint.sh"]
```

- [ ] **Step 4: Build the image**

Run: `docker build -t shirita:smoke . 2>&1 | tail -15`
Expected: build succeeds through all stages (cold build cooks deps — patience). The final `runtime` image is produced. (If `cargo chef cook` tries to build tauri/gtk, the `-p shirita-web` scope is missing — recheck Step 3.)

- [ ] **Step 5: Smoke-test the running container (named volume path)**

```bash
docker rm -f shirita-smoke 2>/dev/null || true
docker run -d --name shirita-smoke -p 8787:8787 -e TOKEN_SECRET=smoke shirita:smoke
sleep 3
curl -fsS http://127.0.0.1:8787/health && echo " <- health ok"
curl -s http://127.0.0.1:8787/ | grep -o '__SHIRITA_RUNTIME__' | head -1
docker rm -f shirita-smoke
```

Expected: `/health` returns 200 (`ok`-ish body); `/` contains `__SHIRITA_RUNTIME__` (token-injected SPA shell served from the embedded UI).

- [ ] **Step 6: Smoke-test an empty host bind-mount (the ownership fix)**

```bash
D=$(mktemp -d)
docker rm -f shirita-bind 2>/dev/null || true
docker run -d --name shirita-bind -p 8788:8787 -e TOKEN_SECRET=smoke -v "$D":/data shirita:smoke
sleep 3
docker ps --filter name=shirita-bind --format '{{.Status}}'   # must show "Up", not "Restarting/Exited"
curl -fsS http://127.0.0.1:8788/health && echo " <- bind-mount health ok"
ls "$D"   # shirita.db created by the unprivileged user
docker rm -f shirita-bind; rm -rf "$D"
```

Expected: the container stays **Up** (no Permission Denied crash), `/health` ok, and `shirita.db` appears in the bind-mounted dir — proving the gosu entrypoint chowned `/data`.

- [ ] **Step 7: Commit**

```bash
git add Dockerfile deploy/entrypoint.sh .dockerignore
git commit -m "feat(deploy): multi-stage Dockerfile (cargo-chef + embed-ui) + gosu entrypoint"
```

---

### Task 2: docker-compose + deploy docs

**Files:**
- Create: `docker-compose.yml`, `docs/deploy.md`

**Interfaces:**
- Consumes: the image/Dockerfile from Task 1; the env vars from `Config::from_env`.
- Produces: a runnable compose example + operator docs.

- [ ] **Step 1: Write `docker-compose.yml`**

Create `docker-compose.yml`:

```yaml
services:
  shirita:
    build: .                       # or: image: ghcr.io/carbonc39/shirita:latest
    ports:
      - "8787:8787"
    environment:
      # Required. Generate once: openssl rand -hex 32
      TOKEN_SECRET: "${TOKEN_SECRET:?set TOKEN_SECRET (openssl rand -hex 32)}"
      PROVIDER: "${PROVIDER:-openai}"
      OPENAI_API_KEY: "${OPENAI_API_KEY:-}"
      OPENAI_MODEL: "${OPENAI_MODEL:-gpt-4o-mini}"
    volumes:
      - shirita-data:/data
    restart: unless-stopped

volumes:
  shirita-data:
```

- [ ] **Step 2: Write `docs/deploy.md`**

Create `docs/deploy.md`:

```markdown
# Deploying Shirita (self-hosted Web)

Shirita ships as a single self-contained binary with the UI embedded, packaged
as a Docker image. It is **single-user**: the API token is a same-origin
convenience guard, not multi-user auth (see Security).

## Quick start (Docker)

```bash
export TOKEN_SECRET=$(openssl rand -hex 32)
docker run -d --name shirita -p 8787:8787 \
  -e TOKEN_SECRET="$TOKEN_SECRET" \
  -e PROVIDER=openai -e OPENAI_API_KEY=sk-... \
  -v shirita-data:/data \
  ghcr.io/carbonc39/shirita:latest
```

Open http://127.0.0.1:8787 — the page authenticates automatically (the server
injects the token into `index.html`).

## Compose

`docker compose up -d` using the repo's `docker-compose.yml`. Set `TOKEN_SECRET`
(and provider vars) in your shell or a `.env` file.

## Build the image locally

```bash
docker build -t shirita .
```

The build runs the frontend (`npm run build`), then compiles the Rust binary
with the UI embedded (`--features embed-ui`).

## Environment

| Var | Required | Default (container) | Purpose |
|-----|----------|---------------------|---------|
| `TOKEN_SECRET` | yes | — | API bearer token; the server injects it into the UI |
| `DATABASE_PATH` | no | `/data/shirita.db` | SQLite file |
| `ASSETS_DIR` | no | `/data/assets` | uploaded media |
| `BIND_ADDR` | no | `0.0.0.0:8787` | listen address |
| `PROVIDER` | no | (OpenAI-compatible) | `openai` / `ollama` / `anthropic` |
| `OPENAI_API_KEY` / `OPENAI_BASE_URL` / `OPENAI_MODEL` | no | — | provider config |

Persist `/data` (named volume or bind-mount). An empty host bind-mount works —
the container fixes its ownership on start.

## Security

The injected token means anyone who can load the page can call the API. Shirita
has no built-in login. For anything beyond a trusted LAN, put a reverse proxy
with its own authentication (and TLS) in front, or restrict access by network
(VPN / firewall). Do not expose it directly to the internet unauthenticated.

## Releases

Pushing a `v*` tag runs two workflows: `docker.yml` (publishes
`ghcr.io/carbonc39/shirita:<tag>` + `:latest`) and `desktop.yml` (builds the
unsigned AppImage/deb/dmg/msi desktop installers).
```

- [ ] **Step 3: Commit**

```bash
git add docker-compose.yml docs/deploy.md
git commit -m "docs(deploy): docker-compose example + deploy/operator guide"
```

---

### Task 3: Release CI — publish to GHCR

**Files:**
- Create: `.github/workflows/docker.yml`

**Interfaces:**
- Consumes: the root `Dockerfile`.
- Produces: a `v*`-tag workflow pushing `ghcr.io/<owner>/shirita`.

- [ ] **Step 1: Write the workflow**

Create `.github/workflows/docker.yml`:

```yaml
name: docker-build

on:
  workflow_dispatch: {}
  push:
    tags:
      - 'v*'

permissions:
  contents: read
  packages: write

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: docker/setup-buildx-action@v3

      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/${{ github.repository_owner }}/shirita
          tags: |
            type=ref,event=tag
            type=raw,value=latest

      - uses: docker/build-push-action@v6
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

- [ ] **Step 2: Validate the workflow YAML**

Run: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/docker.yml')); print('yaml ok')"`
Expected: `yaml ok`. (`metadata-action` lowercases the image ref, so a mixed-case owner like `CarbonC39` still produces a valid `ghcr.io/carbonc39/shirita` reference.)

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/docker.yml
git commit -m "ci: publish Docker image to ghcr.io on v* tags (buildx + gha cache)"
```

---

## Final Verification

- [ ] **Image builds + both smoke paths + workflow valid**

Run:
```bash
docker build -t shirita:smoke . 2>&1 | tail -5
docker rm -f shirita-smoke 2>/dev/null || true
docker run -d --name shirita-smoke -p 8787:8787 -e TOKEN_SECRET=smoke shirita:smoke && sleep 3
curl -fsS http://127.0.0.1:8787/health && curl -s http://127.0.0.1:8787/ | grep -o __SHIRITA_RUNTIME__ | head -1
docker rm -f shirita-smoke
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/docker.yml')); print('yaml ok')"
```
Expected: image builds; `/health` ok; `/` carries `__SHIRITA_RUNTIME__`; workflow YAML valid. (The empty-bind-mount path from Task 1 Step 6 also passes.)

---

## Self-Review

**Spec coverage (spec §6, §7, §8):**
- Multi-stage Dockerfile (frontend → chef → builder → runtime) — Task 1; cargo-chef dep layer + `--from=frontend dist` + `-p shirita-web --features embed-ui`.
- Bind-mount ownership fix (root entrypoint chown → gosu appuser) — Task 1 `entrypoint.sh`; asserted by Task 1 Step 6.
- Runtime hardening: non-root server, `ca-certificates`+`curl`+`gosu`, `0.0.0.0` bind, `/data` volume, `/health` healthcheck, required `TOKEN_SECRET` — Task 1.
- `.dockerignore` — Task 1.
- compose + deploy docs (env reference + network-hardening note + release flow) — Task 2.
- Release CI to GHCR on `v*` with buildx + `type=gha` cache — Task 3.
- Desktop (M8 `desktop.yml`) unchanged; release flow documented — `docs/deploy.md`.

**Placeholder scan:** none — full Dockerfile, full entrypoint, full compose, full docs, full workflow, exact build/run/smoke commands.

**Type consistency:** env var names (`TOKEN_SECRET`/`DATABASE_PATH`/`ASSETS_DIR`/`BIND_ADDR`/`PROVIDER`/`OPENAI_*`) match `shirita_core::Config::from_env`/`apply_provider_env`. The binary is `shirita-web` (workspace package built `-p shirita-web`), installed to `/usr/local/bin/shirita-web` and invoked by name in `entrypoint.sh`. `/health` is the public unauthenticated route from `app()`. The `--features embed-ui` build matches Plan 1. Image ref `ghcr.io/carbonc39/shirita` matches the repo owner.

**Risk notes:**
- `cargo chef cook -p shirita-web` scopes dependency compilation to shirita-web, so the gtk/webkit-less `rust:bookworm` builder never tries to build `shirita-tauri`. If a future change makes shirita-web depend on a system lib, add the `apt-get` to the builder stage.
- A cold CI build (no gha cache yet) cooks all deps (~10+ min); subsequent `v*` builds reuse `cache-from: type=gha` and only recompile app crates.
- `package-lock.json` must exist for `npm ci` (it does — used by `desktop.yml`).
