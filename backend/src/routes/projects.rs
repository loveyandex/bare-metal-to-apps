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

    let docker_network = format!("paas-net-{slug}");

    let project: Project = sqlx::query_as(
        "insert into projects (name, slug, docker_network) values ($1, $2, $3)
         returning id, name, slug, docker_network, created_at",
    )
    .bind(&body.name)
    .bind(&slug)
    .bind(&docker_network)
    .fetch_one(&state.pool)
    .await
    .map_err(internal)?;

    state
        .orchestrator
        .ensure_network(&docker_network)
        .await
        .map_err(internal)?;

    Ok(Json(project))
}

pub async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<Vec<Project>>, (StatusCode, String)> {
    let projects: Vec<Project> = sqlx::query_as(
        "select id, name, slug, docker_network, created_at from projects order by created_at desc",
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
        "select id, name, slug, docker_network, created_at from projects where id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?
    .ok_or((StatusCode::NOT_FOUND, "project not found".to_string()))?;

    let services: Vec<crate::models::Service> = sqlx::query_as(
        "select id, project_id, name, slug, kind, deploy_source, status, status_message, container_id, container_name, desired_replicas, created_at, updated_at
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
    let services: Vec<(Uuid, Option<String>)> =
        sqlx::query_as("select id, container_id from services where project_id = $1")
            .bind(id)
            .fetch_all(&state.pool)
            .await
            .map_err(internal)?;
    for (_svc_id, container_id) in services {
        if let Some(cid) = container_id {
            let _ = state.orchestrator.stop_and_remove(&cid).await;
        }
    }
    let network: Option<String> = sqlx::query_scalar("select docker_network from projects where id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(internal)?;
    if let Some(net) = network {
        let _ = state.orchestrator.remove_network(&net).await;
    }
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
