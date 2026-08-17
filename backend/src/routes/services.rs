use super::projects::internal;
use crate::dbprovision;
use crate::models::{DbEngine, DeployEvent, DeploySource, EnvVar, EnvVarMasked, PortMapping, Service};
use crate::worker::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum CreateServiceBody {
    DockerImage { name: Option<String>, image: String },
    Database { name: Option<String>, engine: DbEngine },
}

fn slugify(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "service".to_string()
    } else {
        s
    }
}

pub async fn create_service(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(body): Json<CreateServiceBody>,
) -> Result<Json<Service>, (StatusCode, String)> {
    let (project_slug, deploy_mode): (String, String) =
        sqlx::query_as("select slug, deploy_mode from projects where id = $1")
            .bind(project_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal)?
            .ok_or((StatusCode::NOT_FOUND, "project not found".to_string()))?;

    let (kind, name, deploy_source): (&str, String, DeploySource) = match &body {
        CreateServiceBody::DockerImage { name, image } => (
            "app",
            name.clone().unwrap_or_else(|| image.clone()),
            DeploySource::DockerImage {
                image: image.clone(),
            },
        ),
        CreateServiceBody::Database { name, engine } => (
            "database",
            name.clone().unwrap_or_else(|| engine.as_str().to_string()),
            DeploySource::Database { engine: *engine },
        ),
    };

    let base_slug = slugify(&name);
    let mut slug = base_slug.clone();
    let mut n = 1;
    loop {
        let exists: Option<Uuid> =
            sqlx::query_scalar("select id from services where project_id = $1 and slug = $2")
                .bind(project_id)
                .bind(&slug)
                .fetch_optional(&state.pool)
                .await
                .map_err(internal)?;
        if exists.is_none() {
            break;
        }
        n += 1;
        slug = format!("{base_slug}-{n}");
    }

    let container_name = format!("{project_slug}-{slug}");
    let source_json = serde_json::to_value(&deploy_source).map_err(internal)?;

    let service: Service = sqlx::query_as(
        "insert into services (project_id, name, slug, kind, deploy_source, container_name)
         values ($1, $2, $3, $4, $5, $6)
         returning id, project_id, name, slug, kind, deploy_source, status, status_message, container_id, container_name, desired_replicas, ports, created_at, updated_at",
    )
    .bind(project_id)
    .bind(&name)
    .bind(&slug)
    .bind(kind)
    .bind(&source_json)
    .bind(&container_name)
    .fetch_one(&state.pool)
    .await
    .map_err(internal)?;

    if let DeploySource::Database { engine } = &deploy_source {
        // The DNS-resolvable hostname other services will use to reach this
        // one differs by deploy target: a Docker container's name on the
        // shared bridge network, or a Kubernetes Service named after the slug.
        let internal_host = if deploy_mode == "kubernetes" { slug.clone() } else { container_name.clone() };
        let password = dbprovision::generate_password();
        insert_env(&state, service.id, "DB_PASSWORD", &password, true, true)
            .await
            .map_err(internal)?;
        for gv in dbprovision::output_vars(*engine, &internal_host, &password) {
            insert_env(&state, service.id, &gv.key, &gv.value, gv.is_secret, true)
                .await
                .map_err(internal)?;
        }
    }

    state.deploy_queue.send(service.id).ok();
    state
        .hub
        .publish(crate::ws::Event::ServiceCreated {
            project_id,
            service_id: service.id,
        });

    Ok(Json(service))
}

async fn insert_env(
    state: &AppState,
    service_id: Uuid,
    key: &str,
    value: &str,
    is_secret: bool,
    is_generated: bool,
) -> anyhow::Result<()> {
    sqlx::query(
        "insert into env_vars (service_id, key, value, is_secret, is_generated)
         values ($1, $2, $3, $4, $5)
         on conflict (service_id, key) do update set value = excluded.value",
    )
    .bind(service_id)
    .bind(key)
    .bind(value)
    .bind(is_secret)
    .bind(is_generated)
    .execute(&state.pool)
    .await?;
    Ok(())
}

pub async fn get_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Service>, (StatusCode, String)> {
    let service: Service = sqlx::query_as(
        "select id, project_id, name, slug, kind, deploy_source, status, status_message, container_id, container_name, desired_replicas, ports, created_at, updated_at
         from services where id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?
    .ok_or((StatusCode::NOT_FOUND, "service not found".to_string()))?;
    Ok(Json(service))
}

pub async fn delete_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let row: Option<(Option<String>, Uuid)> =
        sqlx::query_as("select container_id, project_id from services where id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal)?;
    let Some((container_id, project_id)) = row else {
        return Err((StatusCode::NOT_FOUND, "service not found".to_string()));
    };
    if let Some(cid) = container_id {
        let deploy_mode: Option<String> =
            sqlx::query_scalar("select deploy_mode from projects where id = $1")
                .bind(project_id)
                .fetch_optional(&state.pool)
                .await
                .map_err(internal)?;
        let orchestrator = state.orchestrator_for(deploy_mode.as_deref().unwrap_or("docker"));
        let _ = orchestrator.stop_and_remove(&cid).await;
    }
    sqlx::query("delete from services where id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(internal)?;

    state
        .hub
        .publish(crate::ws::Event::ServiceDeleted {
            project_id,
            service_id: id,
        });

    Ok(StatusCode::NO_CONTENT)
}

pub async fn redeploy_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let exists: Option<Uuid> = sqlx::query_scalar("select id from services where id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal)?;
    if exists.is_none() {
        return Err((StatusCode::NOT_FOUND, "service not found".to_string()));
    }
    state.deploy_queue.send(id).ok();
    Ok(StatusCode::ACCEPTED)
}

pub async fn get_logs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "select s.container_id, p.deploy_mode from services s join projects p on p.id = s.project_id where s.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?;
    let Some((Some(cid), deploy_mode)) = row else {
        return Ok(Json(vec![]));
    };
    let logs = state
        .orchestrator_for(&deploy_mode)
        .logs(&cid, 200)
        .await
        .map_err(internal)?;
    Ok(Json(logs))
}

/// In-place restart of whatever's already running — no re-apply of the spec
/// or image, just cycle the process. Distinct from `redeploy_service`, which
/// re-applies the full spec (picks up a new image tag, env var changes,
/// port changes, ...).
pub async fn restart_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "select s.container_id, p.deploy_mode from services s join projects p on p.id = s.project_id where s.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?;
    let Some((Some(cid), deploy_mode)) = row else {
        return Err((StatusCode::CONFLICT, "service has no running container/pod to restart".to_string()));
    };
    state
        .orchestrator_for(&deploy_mode)
        .restart(&cid)
        .await
        .map_err(internal)?;
    Ok(StatusCode::ACCEPTED)
}

/// Orchestrator-specific structured status detail for the UI: on Kubernetes,
/// the Deployment's rollout status plus its Pods (mirroring `kubectl get
/// deployment,pods`); on Docker, the container inspect summary.
pub async fn describe_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        "select s.container_id, p.deploy_mode from services s join projects p on p.id = s.project_id where s.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?;
    let Some((Some(cid), deploy_mode)) = row else {
        return Ok(Json(serde_json::json!(null)));
    };
    let detail = state
        .orchestrator_for(&deploy_mode)
        .describe(&cid)
        .await
        .map_err(internal)?;
    Ok(Json(detail))
}

/// Recent status transitions for this service — the "event matrix": every
/// `creating` → `pending: waiting: ContainerCreating` → `running` (or
/// `failed`) step the reconciliation worker recorded, newest first.
pub async fn list_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DeployEvent>>, (StatusCode, String)> {
    let events: Vec<DeployEvent> = sqlx::query_as(
        "select id, service_id, status, message, created_at from deploy_events
         where service_id = $1 order by created_at desc limit 100",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(events))
}

#[derive(Deserialize)]
pub struct RevealQuery {
    #[serde(default)]
    pub reveal: bool,
}

pub async fn list_env(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<RevealQuery>,
) -> Result<Json<Vec<EnvVarMasked>>, (StatusCode, String)> {
    let vars: Vec<EnvVar> = sqlx::query_as(
        "select id, service_id, key, value, is_secret, is_generated from env_vars where service_id = $1 order by key",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;

    let out = vars
        .into_iter()
        .map(|v| {
            if q.reveal {
                EnvVarMasked {
                    id: v.id,
                    key: v.key,
                    value: Some(v.value),
                    is_secret: v.is_secret,
                    is_generated: v.is_generated,
                }
            } else {
                v.into()
            }
        })
        .collect();
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct SetEnvBody {
    pub vars: Vec<SetEnvVar>,
}

#[derive(Deserialize)]
pub struct SetEnvVar {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub is_secret: bool,
}

pub async fn set_env(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetEnvBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    for v in body.vars {
        insert_env(&state, id, &v.key, &v.value, v.is_secret, false)
            .await
            .map_err(internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_env(
    State(state): State<AppState>,
    Path((service_id, env_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, String)> {
    sqlx::query("delete from env_vars where id = $1 and service_id = $2 and is_generated = false")
        .bind(env_id)
        .bind(service_id)
        .execute(&state.pool)
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SetEnvRawBody {
    /// Railway-style "paste a .env block" raw editor: one `KEY=VALUE` (or
    /// `KEY="VALUE"`) per line; blank lines and `#`-comments are ignored.
    pub text: String,
}

pub async fn set_env_raw(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetEnvRawBody>,
) -> Result<Json<Vec<EnvVarMasked>>, (StatusCode, String)> {
    for line in body.text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let mut value = value.trim();
        if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value = &value[1..value.len() - 1];
        }
        insert_env(&state, id, key, value, false, false)
            .await
            .map_err(internal)?;
    }

    let vars: Vec<EnvVar> = sqlx::query_as(
        "select id, service_id, key, value, is_secret, is_generated from env_vars where service_id = $1 order by key",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(vars.into_iter().map(Into::into).collect()))
}

pub async fn list_ports(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<PortMapping>>, (StatusCode, String)> {
    let ports_json: serde_json::Value = sqlx::query_scalar("select ports from services where id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "service not found".to_string()))?;
    let ports: Vec<PortMapping> = serde_json::from_value(ports_json).unwrap_or_default();
    Ok(Json(ports))
}

#[derive(Deserialize)]
pub struct SetPortsBody {
    pub ports: Vec<PortMapping>,
}

pub async fn set_ports(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetPortsBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    for p in &body.ports {
        if let Some(host_port) = p.host_port {
            if host_port == 0 {
                return Err((StatusCode::BAD_REQUEST, "host_port must be > 0".to_string()));
            }
        }
    }
    let ports_json = serde_json::to_value(&body.ports).map_err(internal)?;
    let exists: Option<Uuid> = sqlx::query_scalar(
        "update services set ports = $1, updated_at = now() where id = $2 returning id",
    )
    .bind(&ports_json)
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?;
    if exists.is_none() {
        return Err((StatusCode::NOT_FOUND, "service not found".to_string()));
    }

    // Ports only take effect on the running container/pod after a redeploy.
    state.deploy_queue.send(id).ok();
    Ok(StatusCode::ACCEPTED)
}
