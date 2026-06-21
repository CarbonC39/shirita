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
