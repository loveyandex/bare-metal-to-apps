use crate::dbprovision;
use crate::envresolve::resolve_env;
use crate::models::{DeployMode, DeploySource, PortMapping, Service};
use crate::orchestrator::{ContainerState, Orchestrator, RunSpec};
use crate::ws::{Event, Hub};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// How long we'll keep polling a freshly-started container/pod for it to
/// become `Running` before giving up on this attempt. Image pulls from a
/// registry like ghcr.io can legitimately take a couple of minutes on a slow
/// connection or a large image — the old code gave up after ~2 seconds and
/// reported "failed", which is what made a merely-slow pull look broken.
const READY_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub docker: Arc<dyn Orchestrator>,
    pub kubernetes: Arc<dyn Orchestrator>,
    pub hub: Hub,
    pub deploy_queue: tokio::sync::mpsc::UnboundedSender<Uuid>,
}

impl AppState {
    pub fn orchestrator_for(&self, mode: &str) -> &Arc<dyn Orchestrator> {
        match mode {
            "kubernetes" => &self.kubernetes,
            _ => &self.docker,
        }
    }
}

/// Runs forever: drains the deploy queue (services that were just created or
/// asked to redeploy) and separately polls settled containers so drift
/// (crashes, evictions) is noticed even without an explicit trigger. This is
/// the "cron" that keeps retrying a service until it comes up healthy.
pub async fn run(
    pool: PgPool,
    docker: Arc<dyn Orchestrator>,
    kubernetes: Arc<dyn Orchestrator>,
    hub: Hub,
    mut deploy_rx: tokio::sync::mpsc::UnboundedReceiver<Uuid>,
) {
    let pool_poll = pool.clone();
    let docker_poll = docker.clone();
    let k8s_poll = kubernetes.clone();
    let hub_poll = hub.clone();
    tokio::spawn(async move {
        poll_loop(pool_poll, docker_poll, k8s_poll, hub_poll).await;
    });

    while let Some(service_id) = deploy_rx.recv().await {
        let pool = pool.clone();
        let docker = docker.clone();
        let kubernetes = kubernetes.clone();
        let hub = hub.clone();
        tokio::spawn(async move {
            let mode: Option<String> = sqlx::query_scalar(
                "select p.deploy_mode from services s join projects p on p.id = s.project_id where s.id = $1",
            )
            .bind(service_id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
            let orchestrator: &dyn Orchestrator = match mode.as_deref() {
                Some("kubernetes") => kubernetes.as_ref(),
                _ => docker.as_ref(),
            };
            deploy_with_retry(&pool, orchestrator, &hub, service_id).await;
        });
    }
}

async fn deploy_with_retry(
    pool: &PgPool,
    orchestrator: &dyn Orchestrator,
    hub: &Hub,
    service_id: Uuid,
) {
    let mut backoff = Duration::from_secs(1);
    for attempt in 1..=5 {
        match deploy_once(pool, orchestrator, hub, service_id).await {
            Ok(()) => return,
            Err(e) => {
                tracing::warn!(%service_id, attempt, error = %e, "deploy attempt failed");
                set_status(pool, hub, service_id, "creating", Some(format!("retry {attempt}/5: {e}"))).await;
                tokio::time::sleep(backoff).await;
                backoff *= 2;
            }
        }
    }
    set_status(
        pool,
        hub,
        service_id,
        "failed",
        Some("exhausted retries starting container".to_string()),
    )
    .await;
}

async fn deploy_once(
    pool: &PgPool,
    orchestrator: &dyn Orchestrator,
    hub: &Hub,
    service_id: Uuid,
) -> anyhow::Result<()> {
    let svc: Service = sqlx::query_as(
        "select id, project_id, name, slug, kind, deploy_source, status, status_message, container_id, container_name, desired_replicas, ports, created_at, updated_at from services where id = $1",
    )
    .bind(service_id)
    .fetch_one(pool)
    .await?;

    let (docker_network, project_slug, deploy_mode): (String, String, String) = sqlx::query_as(
        "select docker_network, slug, deploy_mode from projects where id = $1",
    )
    .bind(svc.project_id)
    .fetch_one(pool)
    .await?;
    let network = if deploy_mode == "kubernetes" {
        project_slug
    } else {
        docker_network
    };

    orchestrator.ensure_network(&network).await?;

    let source: DeploySource = serde_json::from_value(svc.deploy_source.clone())?;
    let published_ports: Vec<(u16, Option<u16>)> = serde_json::from_value::<Vec<PortMapping>>(svc.ports.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.container_port, p.host_port))
        .collect();

    let (image, mut env, volume, cmd) = match &source {
        DeploySource::DockerImage { image } => {
            let (resolved, warnings) = resolve_env(pool, svc.project_id, svc.id).await?;
            for w in warnings {
                tracing::warn!(%service_id, "{w}");
            }
            (image.clone(), resolved, None, None)
        }
        DeploySource::Database { engine } => {
            let password: String = sqlx::query_scalar(
                "select value from env_vars where service_id = $1 and key = 'DB_PASSWORD'",
            )
            .bind(svc.id)
            .fetch_one(pool)
            .await?;
            let env: std::collections::HashMap<String, String> =
                dbprovision::container_env(*engine, &password).into_iter().collect();
            let volume = Some((format!("{}-data", svc.container_name), data_path(*engine)));
            let cmd = dbprovision::container_command(*engine, &password);
            (engine.image().to_string(), env, volume, cmd)
        }
    };
    env.retain(|_, v| !v.is_empty());

    set_status(pool, hub, service_id, "creating", Some("applying".to_string())).await;

    let container_name = if deploy_mode == DeployMode::Kubernetes.to_string() {
        svc.slug.clone()
    } else {
        svc.container_name.clone()
    };

    let spec = RunSpec {
        container_name,
        image,
        network,
        env: std::mem::take(&mut env),
        published_ports,
        volume,
        cmd,
    };

    let container_id = orchestrator.run(spec).await?;

    sqlx::query("update services set container_id = $1, updated_at = now() where id = $2")
        .bind(&container_id)
        .bind(svc.id)
        .execute(pool)
        .await?;

    wait_until_ready(pool, orchestrator, hub, service_id, &container_id).await
}

/// Polls a just-started container/pod until it settles into a state worth
/// reporting: `Running` (success), `CrashLoopBackOff` (deployed, but the app
/// itself is unhealthy — retrying the deploy won't fix that), or a genuine
/// failure to ever come up within `READY_TIMEOUT`. Everything in between
/// (`Pending`, `ImagePullError`) is reported live as `pending` with whatever
/// reason the orchestrator can tell us, and never counted as a failure on
/// its own — only exhausting the timeout while still stuck there is.
async fn wait_until_ready(
    pool: &PgPool,
    orchestrator: &dyn Orchestrator,
    hub: &Hub,
    service_id: Uuid,
    container_id: &str,
) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    let mut last_state: Option<ContainerState> = None;

    loop {
        let state = orchestrator.inspect(container_id).await?;
        let changed = last_state.as_ref() != Some(&state);

        match &state {
            ContainerState::Running => {
                set_status(pool, hub, service_id, "running", None).await;
                return Ok(());
            }
            ContainerState::CrashLoopBackOff { restarts } => {
                set_status(
                    pool,
                    hub,
                    service_id,
                    "crashed",
                    Some(format!("crash-looping ({restarts} restarts)")),
                )
                .await;
                return Ok(());
            }
            ContainerState::Exited { code } => {
                anyhow::bail!("container exited immediately with code {code}");
            }
            ContainerState::NotFound => {
                anyhow::bail!("container/pod disappeared while waiting to become ready");
            }
            ContainerState::Pending { reason } if changed => {
                let msg = reason
                    .clone()
                    .map(|r| format!("waiting: {r}"))
                    .unwrap_or_else(|| "waiting to start".to_string());
                set_status(pool, hub, service_id, "pending", Some(msg)).await;
            }
            ContainerState::ImagePullError { reason } if changed => {
                set_status(pool, hub, service_id, "pending", Some(format!("pulling image: {reason}"))).await;
            }
            ContainerState::Other(s) if changed => {
                set_status(pool, hub, service_id, "pending", Some(format!("state: {s}"))).await;
            }
            _ => {} // unchanged Pending/ImagePullError/Other — avoid spamming the event log every poll tick
        }

        if tokio::time::Instant::now() >= deadline {
            let detail = match &state {
                ContainerState::Pending { reason } => reason.clone().unwrap_or_else(|| "pending".to_string()),
                ContainerState::ImagePullError { reason } => format!("pulling image: {reason}"),
                ContainerState::Other(s) => s.clone(),
                other => format!("{other:?}"),
            };
            anyhow::bail!("still not ready after {}s (last state: {detail})", READY_TIMEOUT.as_secs());
        }

        last_state = Some(state);
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn data_path(engine: crate::models::DbEngine) -> String {
    use crate::models::DbEngine::*;
    match engine {
        Postgres => "/var/lib/postgresql/data".to_string(),
        Redis => "/data".to_string(),
        Mysql => "/var/lib/mysql".to_string(),
        Mongodb => "/data/db".to_string(),
    }
}

async fn set_status(pool: &PgPool, hub: &Hub, service_id: Uuid, status: &str, message: Option<String>) {
    let project_id: Option<Uuid> = sqlx::query_scalar("select project_id from services where id = $1")
        .bind(service_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

    let _ = sqlx::query(
        "update services set status = $1, status_message = $2, updated_at = now() where id = $3",
    )
    .bind(status)
    .bind(&message)
    .bind(service_id)
    .execute(pool)
    .await;

    let _ = sqlx::query(
        "insert into deploy_events (service_id, status, message) values ($1, $2, $3)",
    )
    .bind(service_id)
    .bind(status)
    .bind(&message)
    .execute(pool)
    .await;

    if let Some(project_id) = project_id {
        hub.publish(Event::ServiceStatus {
            service_id,
            project_id,
            status: status.to_string(),
            message,
        });
    }
}

/// Periodically checks every service in a settled state (`running`,
/// `crashed`, or a `pending` state left over from a backend restart mid-wait)
/// and reflects reality — e.g. the container crashed on its own, or a
/// Kubernetes pod got evicted and is scheduling again.
async fn poll_loop(pool: PgPool, docker: Arc<dyn Orchestrator>, kubernetes: Arc<dyn Orchestrator>, hub: Hub) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let rows: Vec<(Uuid, String, String, String)> = match sqlx::query_as(
            "select s.id, s.container_id, s.status, p.deploy_mode
             from services s join projects p on p.id = s.project_id
             where s.container_id is not null and s.status in ('running','crashed','pending')",
        )
        .fetch_all(&pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "poll query failed");
                continue;
            }
        };

        for (service_id, container_id, prev_status, deploy_mode) in rows {
            let orchestrator: &dyn Orchestrator = if deploy_mode == "kubernetes" {
                kubernetes.as_ref()
            } else {
                docker.as_ref()
            };
            let state = match orchestrator.inspect(&container_id).await {
                Ok(s) => s,
                Err(_) => continue,
            };
            let (new_status, message): (&str, Option<String>) = match &state {
                ContainerState::Running => ("running", None),
                ContainerState::CrashLoopBackOff { restarts } => {
                    ("crashed", Some(format!("crash-looping ({restarts} restarts)")))
                }
                ContainerState::Exited { code } => ("crashed", Some(format!("exited with code {code}"))),
                ContainerState::NotFound => ("crashed", Some("container/pod not found".to_string())),
                ContainerState::Pending { reason } => (
                    "pending",
                    Some(reason.clone().map(|r| format!("waiting: {r}")).unwrap_or_else(|| "waiting to start".to_string())),
                ),
                ContainerState::ImagePullError { reason } => ("pending", Some(format!("pulling image: {reason}"))),
                ContainerState::Other(s) => ("crashed", Some(format!("state: {s}"))),
            };
            if new_status != prev_status {
                set_status(&pool, &hub, service_id, new_status, message).await;
            }
        }
    }
}
