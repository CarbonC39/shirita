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

Pushing a `v*` tag runs three workflows: `docker.yml` (publishes
`ghcr.io/carbonc39/shirita:<tag>` + `:latest`), `web.yml` (builds the
standalone embed-UI `shirita-web` binary for Linux static-musl / macOS /
Windows as artifacts), and `desktop.yml` (builds the unsigned
AppImage/deb/dmg/msi desktop installers).
