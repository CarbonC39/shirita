# shirita-ui

Vue 3 + Vite frontend for Shirita.

## Dev

1. Start the backend (from repo root): `cargo run -p shirita-web` (listens on 127.0.0.1:8787).
2. `cp shirita-ui/.env.example shirita-ui/.env.local` and set `VITE_API_TOKEN` to the backend's `TOKEN_SECRET`.
3. `cd shirita-ui && npm install && npm run dev` — open the printed URL. `/api` and `/assets` are proxied to the backend.

## Test / build

- `npm run test` — Vitest unit/component tests.
- `npm run build` — type-check + production bundle to `dist/`.

Production embedding of `dist/` into the binary is handled later (M9).
