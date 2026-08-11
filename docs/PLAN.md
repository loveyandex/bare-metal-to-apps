# Railway-style PaaS — Product & Build Plan

A minimal, self-hosted "Railway.com"-style PaaS. v0 runs entirely on the local
Docker daemon of a single bare-metal box. v1+ moves orchestration to
Kubernetes across multiple bare-metal server pools, with no hypervisor/VM
layer anywhere — pure containerization.

## Architecture (v0)

- **backend/** — Rust (axum + sqlx/Postgres + bollard). Owns all state
  (projects, services, env vars, deploy history) and is the only thing that
  talks to the Docker daemon. Exposes a REST API + a WebSocket channel that
  pushes live service-status events to the UI (`creating → running`,
  `running → crashed`, etc.), backed by a reconciliation loop (a Tokio task
  acting as the "cron" the brief asked for: it retries failed container
  starts with backoff until the service is healthy, and polls container
  state so the UI never has to poll).
- **frontend/** — Next.js (App Router) + Tailwind + shadcn/ui + Base UI
  primitives. Dashboard → Project canvas (React Flow) → Service detail
  panel. Live status badges via the backend WebSocket.
- **Networking model** — each project gets its own Docker bridge network
  (`paas-net-<project_id>`). Every service container joins it and is
  addressable by its container name, so `DATABASE_URL`-style env vars can
  point services at each other by DNS name without exposing host ports.

  Note on the brief's mention of Theatre.js: Theatre.js is a keyframe/
  animation library for creative-coding motion design, not a project/graph
  UI toolkit — it has no relevant primitives for a node-and-edge service
  canvas. React Flow is the library Railway's own UI pattern maps to, so v0
  uses React Flow for the canvas instead and skips Theatre.js.

## Entities (backend/src/models.rs)

- `Project { id, name, slug, docker_network, created_at }`
- `Service { id, project_id, name, kind(app|database), deploy_source(json),
  status(creating|running|crashed|failed|stopped|deleting),
  container_id, container_name, created_at, updated_at }`
  - `deploy_source` is a tagged enum: `DockerImage { image }` or
    `Database { engine: postgres|redis|mysql|mongodb }`.
- `EnvVar { id, service_id, key, value, is_secret, is_generated }`
  - `value` may reference another service's output var with
    `${{service-slug.KEY}}`; resolved at deploy time.
  - Database services auto-generate a strong password and publish standard
    output vars (`POSTGRES_*`/`REDIS_*`/`DATABASE_URL`/`REDIS_URL`),
    marked `is_secret` so the UI hides them behind a reveal action.

## Feature roadmap

### Phase 0 — Foundations (this PR)
- [x] Rust backend skeleton: axum server, Postgres via sqlx, migrations.
- [x] Project/Service/EnvVar CRUD API.
- [x] Docker orchestration via bollard: create network, run container,
      inspect status, stop/remove.
- [x] Reconciliation worker: drives services from `creating` to `running`,
      retries on failure, marks `crashed` when the container exits.
- [x] WebSocket broadcast of service status + log lines to connected UIs.
- [x] Deploy-from-Docker-image flow (no Git/registry auth needed).
- [x] One-click database provisioning (Postgres, Redis) with generated
      credentials and standard output env vars.
- [x] Env var linking between services (`${{service.VAR}}` resolution).
- [x] Next.js UI: dashboard, new project, project canvas (React Flow),
      "+Create" menu (GitHub repo placeholder / Database / Docker Image),
      service detail panel (status, env vars, reveal secrets, logs, delete).

### Phase 1 — Deploy loop polish
- [ ] GHCR/Docker Hub tag-digest poller: cron task that notices
      `ghcr.io/...:v1.0.13` got a new digest and redeploys automatically
      (the "new image ready → auto redeploy" flow from the brief).
- [ ] Structured deploy history / rollback to previous container image.
- [ ] Log streaming over the same WebSocket instead of poll-on-open.
- [ ] Health checks (HTTP/TCP) driving status instead of "container is up".
- [ ] Replica count for stateless services (N containers behind a simple
      round-robin proxy) as a stepping stone to k8s Deployments.

### Phase 2 — GitHub-native deploys
- [ ] GitHub App install, repo picker (screenshot 1's flow).
- [ ] Build via buildpacks/Nixpacks or a user Dockerfile on push.
- [ ] Per-commit deploy + auto-deploy-on-push using GitHub webhooks.

### Phase 3 — Kubernetes / multi-node
- [ ] Swap the Docker-daemon driver for a Kubernetes driver behind the same
      internal `Orchestrator` trait (Deployments for stateless services,
      StatefulSets + PVCs for Cassandra/Postgres/etc., Services for
      in-cluster DNS, HPA for replica scaling).
- [ ] Multi-supermicro-set support: register additional bare-metal node
      pools ("sup1, sup2, sup3…") as separate Kubernetes clusters/regions;
      a project picks a target region at deploy time.
- [ ] StatefulSet-backed Cassandra (or any stateful DB) as a first-class
      "database" deploy source, same UX as Postgres/Redis in v0.
- [ ] Multi-tenant isolation via Namespaces + NetworkPolicies (replacing
      the v0 per-project Docker network).

### Explicitly deferred (per brief)
- Auth/sign-in — v0 has no auth, single-tenant, local-only.
- Region/infra management UI — hardcoded to "local docker" until Phase 3.

## Orchestrator abstraction

`backend/src/orchestrator/mod.rs` defines a small trait
(`create_network`, `run_service`, `inspect`, `stop`, `remove`, `logs`) with
one implementation today (`docker.rs`, via bollard talking to the local
Unix socket). Phase 3 adds a `kubernetes.rs` implementation behind the same
trait so the rest of the backend (API handlers, reconciliation worker,
WebSocket broadcasting) does not change when the deploy target moves from a
single box to a k8s cluster.
