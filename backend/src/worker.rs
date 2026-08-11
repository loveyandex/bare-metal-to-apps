use crate::dbprovision;
use crate::envresolve::resolve_env;
use crate::models::{DeploySource, Service};
use crate::orchestrator::{ContainerState, Orchestrator, RunSpec};
use crate::ws::{Event, Hub};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub orchestrator: Arc<dyn Orchestrator>,
    pub hub: Hub,
    pub deploy_queue: tokio::sync::mpsc::UnboundedSender<Uuid>,
}

/// Runs forever: drains the deploy queue (services that were just created or
/// asked to redeploy) and separately polls running containers so crashes are
/// noticed even without an explicit trigger. This is the "cron" that keeps
/// retrying a service until it comes up healthy.
pub async fn run(
    pool: PgPool,
    orchestrator: Arc<dyn Orchestrator>,
    hub: Hub,
    mut deploy_rx: tokio::sync::mpsc::UnboundedReceiver<Uuid>,
) {
    let pool_poll = pool.clone();
    let orch_poll = orchestrator.clone();
    let hub_poll = hub.clone();
    tokio::spawn(async move {
        poll_loop(pool_poll, orch_poll, hub_poll).await;
    });

    while let Some(service_id) = deploy_rx.recv().await {
        let pool = pool.clone();
        let orchestrator = orchestrator.clone();
        let hub = hub.clone();
        tokio::spawn(async move {
            deploy_with_retry(&pool, orchestrator.as_ref(), &hub, service_id).await;
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
        "select id, project_id, name, slug, kind, deploy_source, status, status_message, container_id, container_name, desired_replicas, created_at, updated_at from services where id = $1",
    )
    .bind(service_id)
    .fetch_one(pool)
    .await?;

    let network: String = sqlx::query_scalar("select docker_network from projects where id = $1")
        .bind(svc.project_id)
        .fetch_one(pool)
        .await?;

    orchestrator.ensure_network(&network).await?;

    let source: DeploySource = serde_json::from_value(svc.deploy_source.clone())?;

    let (image, mut env, published_ports, volume, cmd) = match &source {
        DeploySource::DockerImage { image } => {
            let (resolved, warnings) = resolve_env(pool, svc.project_id, svc.id).await?;
            for w in warnings {
                tracing::warn!(%service_id, "{w}");
            }
            (image.clone(), resolved, vec![], None, None)
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
            (engine.image().to_string(), env, vec![], volume, cmd)
        }
    };
    env.retain(|_, v| !v.is_empty());

    set_status(pool, hub, service_id, "creating", None).await;

    let spec = RunSpec {
        container_name: svc.container_name.clone(),
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

    // Give the container a moment to either come up or crash immediately.
    tokio::time::sleep(Duration::from_secs(2)).await;
    match orchestrator.inspect(&container_id).await? {
        ContainerState::Running => {
            set_status(pool, hub, service_id, "running", None).await;
            Ok(())
        }
        ContainerState::Exited { code } => {
            anyhow::bail!("container exited immediately with code {code}")
        }
        other => anyhow::bail!("unexpected container state after start: {other:?}"),
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

/// Periodically checks every service with a container_id that we believe is
/// `running` and reflects reality (e.g. the container crashed on its own).
async fn poll_loop(pool: PgPool, orchestrator: Arc<dyn Orchestrator>, hub: Hub) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let rows: Vec<(Uuid, String, String)> = match sqlx::query_as(
            "select id, container_id, status from services where container_id is not null and status in ('running','crashed')",
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

        for (service_id, container_id, prev_status) in rows {
            let state = match orchestrator.inspect(&container_id).await {
                Ok(s) => s,
                Err(_) => continue,
            };
            let new_status = match state {
                ContainerState::Running => "running",
                ContainerState::Exited { .. } => "crashed",
                ContainerState::NotFound => "crashed",
                ContainerState::Other(_) => "crashed",
            };
            if new_status != prev_status {
                set_status(&pool, &hub, service_id, new_status, None).await;
            }
        }
    }
}
