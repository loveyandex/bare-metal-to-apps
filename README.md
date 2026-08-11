# Dockyard

A minimal, self-hosted Railway-style PaaS: create a project, add services from
a Docker image or a one-click database, link them with env vars, and watch
status flip from "Deploying" to "Running" live. v0 runs on your local Docker
daemon; see `docs/PLAN.md` for the roadmap to multi-node Kubernetes.

## Layout

- `backend/` — Rust (axum + sqlx/Postgres + bollard). Owns state and the
  Docker daemon; exposes REST + a WebSocket status feed.
- `frontend/` — Next.js (App Router) + Tailwind + shadcn-style components
  built on `@base-ui/react` + React Flow for the project canvas.
- `docs/PLAN.md` — full feature roadmap.

## Running locally

You need Docker (for the platform's own orchestration target) and a
Postgres instance (for the control plane's state — the docker-compose file
below runs that one for you).

```bash
# 1. control-plane database
docker compose up -d

# 2. backend
cd backend
cp .env.example .env
cargo run   # listens on :8080

# 3. frontend — build first so any build-time error surfaces before you're
#    staring at a dev server that "works" until you deploy it
cd frontend
cp .env.local.example .env.local
npm install
npm run build
npm start   # listens on :3000
```

Use `npm run dev` day-to-day while iterating, but always run `npm run build`
before calling something done — some errors (env/config issues, native
binding resolution, etc.) only show up at build time, not in dev mode.

If `npm install` leaves a broken `@tailwindcss/oxide` / `lightningcss` native
binding (`Cannot find native binding` / `Cannot find module
'@tailwindcss/oxide-linux-x64-gnu'` at build time — a known npm optional-deps
bug, npm/cli#4828), do a clean reinstall:

```bash
rm -rf node_modules package-lock.json .next
npm install
npm run build
```

Open http://localhost:3000, create a project, then add a service either from
a Docker image (e.g. `ghcr.io/love-solana/sqlx:v1.0.13`, or `hello-world` to
try it without a real app) or a one-click database (Postgres/Redis/MySQL/
MongoDB — credentials are generated automatically and hidden behind a reveal
button). Link services by setting an env var value like
`${{postgres.DATABASE_URL}}` on another service.
