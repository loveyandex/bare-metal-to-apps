# Railway-style PaaS — Product & Build Plan

A minimal, self-hosted "Railway.com"-style PaaS. Each project picks its own
deploy target at creation time: **Docker** (containers on the local Docker
daemon, one bridge network per project) or **Kubernetes** (Deployments in
their own namespace on any cluster reachable via `kubectl` — a local `kind`
cluster works fine). No hypervisor/VM layer either way — pure
containerization, and both targets sit behind the same `Orchestrator` trait
so the rest of the backend doesn't care which one a project uses.

## Architecture

- **backend/** — Rust (axum + sqlx/Postgres + bollard). Owns all state
  (projects, services, env vars, deploy history) and is the only thing that
  talks to the deploy target. Exposes a REST API + a WebSocket channel that
  pushes live service-status events to the UI (`creating → running`,
  `running → crashed`, etc.), backed by a reconciliation loop (a Tokio task
  acting as the "cron" the brief asked for: it retries failed deploys with
  backoff until the service is healthy, and polls state so the UI never has
  to poll).
- **frontend/** — Next.js (App Router) + Tailwind + shadcn/ui + Base UI
  primitives. Dashboard → Project canvas (React Flow) → Service detail
  panel. Live status badges via the backend WebSocket.
- **Networking model** — Docker mode: each project gets its own bridge
  network (`paas-net-<slug>`), and every service container joins it,
  addressable by its container name. Kubernetes mode: each project gets its
  own Namespace (named after the project slug), and every service gets a
  Deployment + Service named after its slug, addressable by that Service
  name via in-cluster DNS — same `${{service-slug.KEY}}` linking syntax
  works identically in both modes because env var *values* (not the
  transport) carry the resolved hostname.
- **Published ports** — a service can optionally publish a container port
  externally: a host-published Docker port, or a Kubernetes NodePort.
  Internal service-to-service traffic never needs this — any container port
  is already reachable by other services on the same network/namespace by
  name.

  Note on the brief's mention of Theatre.js: Theatre.js is a keyframe/
  animation library for creative-coding motion design, not a project/graph
  UI toolkit — it has no relevant primitives for a node-and-edge service
  canvas. React Flow is the library Railway's own UI pattern maps to, so v0
  uses React Flow for the canvas instead and skips Theatre.js.

## Entities (backend/src/models.rs)

- `Project { id, name, slug, docker_network, deploy_mode(docker|kubernetes),
  created_at }` — `deploy_mode` is fixed at creation time.
- `Service { id, project_id, name, kind(app|database), deploy_source(json),
  status(creating|running|crashed|failed|stopped|deleting),
  container_id, container_name, ports(json), created_at, updated_at }`
  - `deploy_source` is a tagged enum: `DockerImage { image }` or
    `Database { engine: postgres|redis|mysql|mongodb }`.
  - `ports` is a list of `{ container_port, host_port?, protocol }`; editable
    from the UI's Networking tab, applying triggers a redeploy.
- `EnvVar { id, service_id, key, value, is_secret, is_generated }`
  - `value` may reference another service's output var with
    `${{service-slug.KEY}}`; resolved at deploy time.
  - Database services auto-generate a strong password and publish standard
    output vars (`POSTGRES_*`/`REDIS_*`/`DATABASE_URL`/`REDIS_URL`),
    marked `is_secret` so the UI hides them behind a reveal action.
  - Bulk-editable via a raw `.env`-paste endpoint (`POST .../env/raw`),
    matching Railway's raw variables editor.

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
- [x] Published ports per service (Docker host port / Kubernetes NodePort),
      editable from the UI, applying triggers a redeploy.
- [x] Raw `.env`-paste bulk env var editor (Railway-style raw mode).
- [ ] GHCR/Docker Hub tag-digest poller: cron task that notices
      `ghcr.io/...:v1.0.13` got a new digest and redeploys automatically
      (the "new image ready → auto redeploy" flow from the brief).
- [ ] Structured deploy history / rollback to previous container image.
- [ ] Log streaming over the same WebSocket instead of poll-on-open.
- [ ] Health checks (HTTP/TCP) driving status instead of "container is up".
- [ ] Replica count for stateless services (N containers behind a simple
      round-robin proxy in Docker mode; `spec.replicas` directly in
      Kubernetes mode).

### Phase 2 — GitHub-native deploys
- [ ] GitHub App install, repo picker (screenshot 1's flow).
- [ ] Build via buildpacks/Nixpacks or a user Dockerfile on push.
- [ ] Per-commit deploy + auto-deploy-on-push using GitHub webhooks.

### Phase 3 — Kubernetes / multi-node
- [x] Kubernetes deploy target, chosen per project at creation time,
      alongside Docker — see "Kubernetes orchestrator" below. Currently
      Deployments only; StatefulSets for genuinely stateful engines
      (Cassandra, etc.) are still open.
- [ ] StatefulSet + PVC-backed Cassandra (or any stateful DB) as a
      first-class "database" deploy source, same UX as Postgres/Redis.
- [ ] Multi-supermicro-set support: register additional bare-metal node
      pools ("sup1, sup2, sup3…") as separate kubeconfig contexts/clusters;
      a project picks a target cluster at deploy time (today the backend
      always uses whatever context `kubectl` is current-configured with).
- [ ] NetworkPolicies for stronger tenant isolation between namespaces.
- [ ] Ingress-based routing instead of NodePort, so published ports don't
      need cluster-specific `extraPortMappings`/`port-forward` workarounds.

### Explicitly deferred (per brief)
- Auth/sign-in — v0 has no auth, single-tenant, local-only.
- Multi-cluster/region picker UI — a project's Kubernetes mode always
  targets `kubectl`'s current context until multi-supermicro-set lands.

## Orchestrator abstraction

`backend/src/orchestrator/mod.rs` defines a small trait
(`ensure_network`, `run`, `inspect`, `stop_and_remove`, `logs`,
`remove_network`) with two implementations:

- `docker.rs` — bollard talking to the local Docker Unix socket. `network`
  is a bridge network name; `container_name` is the actual container name.
- `kubernetes.rs` — shells out to `kubectl` (`apply -f -` with a generated
  Deployment+Service[+PVC] manifest, `get pods -o jsonpath` for status,
  `delete`/`logs`). `network` is interpreted as the target Namespace;
  `container_name` becomes the Deployment/Service name (set to the
  service's slug so DNS names are short and match what `${{slug.KEY}}`
  linking expects). Requires `kubectl` on PATH with a working
  `~/.kube/config` context — no additional Kubernetes client dependency.

`AppState` holds both orchestrators (`docker`, `kubernetes`) and picks the
right one per request based on the owning project's `deploy_mode`, so a
single running backend serves Docker-mode and Kubernetes-mode projects
side by side.
