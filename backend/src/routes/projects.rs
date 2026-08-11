use crate::models::Project;
use crate::worker::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateProject {
    pub name: String,
    #[serde(default = "default_deploy_mode")]
    pub deploy_mode: String,
}

fn default_deploy_mode() -> String {
    "docker".to_string()
}

fn slugify(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    let mut out = String::new();
    let mut last_dash = false;
    for c in s.chars() {
        if c == '-' {
            if !last_dash {
                out.push(c);
            }
            last_dash = true;
        } else {
            out.push(c);
            last_dash = false;
        }
    }
    if out.is_empty() {
        out = "project".to_string();
    }
    out
}

pub async fn create_project(
    State(state): State<AppState>,
    Json(body): Json<CreateProject>,
) -> Result<Json<Project>, (StatusCode, String)> {
    let base_slug = slugify(&body.name);
    let mut slug = base_slug.clone();
    let mut n = 1;
    loop {
        let exists: Option<Uuid> = sqlx::query_scalar("select id from projects where slug = $1")
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

    let deploy_mode = match body.deploy_mode.as_str() {
        "docker" | "kubernetes" => body.deploy_mode.clone(),
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown deploy_mode {other}, expected \"docker\" or \"kubernetes\""),
            ))
        }
    };
    let docker_network = format!("paas-net-{slug}");

    let project: Project = sqlx::query_as(
        "insert into projects (name, slug, docker_network, deploy_mode) values ($1, $2, $3, $4)
         returning id, name, slug, docker_network, deploy_mode, created_at",
    )
    .bind(&body.name)
    .bind(&slug)
    .bind(&docker_network)
    .bind(&deploy_mode)
    .fetch_one(&state.pool)
    .await
    .map_err(internal)?;

    // Docker mode provisions its bridge network up front; Kubernetes mode
    // provisions its namespace up front using the project slug (not the
    // docker_network name, which is unused in that mode).
    let network_or_namespace = if deploy_mode == "kubernetes" {
        slug.clone()
    } else {
        docker_network.clone()
    };
    state
        .orchestrator_for(&deploy_mode)
        .ensure_network(&network_or_namespace)
        .await
        .map_err(internal)?;

    Ok(Json(project))
}

pub async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<Vec<Project>>, (StatusCode, String)> {
    let projects: Vec<Project> = sqlx::query_as(
        "select id, name, slug, docker_network, deploy_mode, created_at from projects order by created_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    Ok(Json(projects))
}

#[derive(Serialize)]
pub struct ProjectDetail {
    #[serde(flatten)]
    pub project: Project,
    pub services: Vec<crate::models::Service>,
}

pub async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProjectDetail>, (StatusCode, String)> {
    let project: Project = sqlx::query_as(
        "select id, name, slug, docker_network, deploy_mode, created_at from projects where id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?
    .ok_or((StatusCode::NOT_FOUND, "project not found".to_string()))?;

    let services: Vec<crate::models::Service> = sqlx::query_as(
        "select id, project_id, name, slug, kind, deploy_source, status, status_message, container_id, container_name, desired_replicas, ports, created_at, updated_at
         from services where project_id = $1 order by created_at asc",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;

    Ok(Json(ProjectDetail { project, services }))
}

pub async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let project: Option<(String, String, String)> =
        sqlx::query_as("select slug, docker_network, deploy_mode from projects where id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal)?;
    let Some((slug, docker_network, deploy_mode)) = project else {
        return Ok(StatusCode::NO_CONTENT);
    };

    let orchestrator = state.orchestrator_for(&deploy_mode);

    let services: Vec<(Uuid, Option<String>)> =
        sqlx::query_as("select id, container_id from services where project_id = $1")
            .bind(id)
            .fetch_all(&state.pool)
            .await
            .map_err(internal)?;
    for (_svc_id, container_id) in services {
        if let Some(cid) = container_id {
            let _ = orchestrator.stop_and_remove(&cid).await;
        }
    }

    let network_or_namespace = if deploy_mode == "kubernetes" { slug } else { docker_network };
    let _ = orchestrator.remove_network(&network_or_namespace).await;

    sqlx::query("delete from projects where id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
