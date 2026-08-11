mod db;
mod dbprovision;
mod envresolve;
mod models;
mod orchestrator;
mod routes;
mod worker;
mod ws;

use axum::routing::{delete, get, post};
use axum::Router;
use orchestrator::docker::DockerOrchestrator;
use orchestrator::kubernetes::KubernetesOrchestrator;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use worker::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,paas_backend=debug".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/paas".to_string());
    let pool = db::connect(&database_url).await?;

    let docker: Arc<dyn orchestrator::Orchestrator> = Arc::new(DockerOrchestrator::connect()?);
    let kubernetes: Arc<dyn orchestrator::Orchestrator> = Arc::new(KubernetesOrchestrator::new());
    let hub = ws::Hub::new();
    let (deploy_tx, deploy_rx) = tokio::sync::mpsc::unbounded_channel();

    let state = AppState {
        pool: pool.clone(),
        docker: docker.clone(),
        kubernetes: kubernetes.clone(),
        hub: hub.clone(),
        deploy_queue: deploy_tx.clone(),
    };

    // Re-enqueue anything that wasn't running when the server last stopped.
    let startup_ids: Vec<uuid::Uuid> =
        sqlx::query_scalar("select id from services where status != 'stopped'")
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
    for id in startup_ids {
        let _ = deploy_tx.send(id);
    }

    tokio::spawn(worker::run(pool, docker, kubernetes, hub, deploy_rx));

    let app = Router::new()
        .route("/api/projects", get(routes::projects::list_projects).post(routes::projects::create_project))
        .route(
            "/api/projects/:id",
            get(routes::projects::get_project).delete(routes::projects::delete_project),
        )
        .route(
            "/api/projects/:id/services",
            post(routes::services::create_service),
        )
        .route(
            "/api/services/:id",
            get(routes::services::get_service).delete(routes::services::delete_service),
        )
        .route("/api/services/:id/redeploy", post(routes::services::redeploy_service))
        .route("/api/services/:id/logs", get(routes::services::get_logs))
        .route(
            "/api/services/:id/env",
            get(routes::services::list_env).put(routes::services::set_env),
        )
        .route("/api/services/:id/env/raw", post(routes::services::set_env_raw))
        .route("/api/services/:sid/env/:eid", delete(routes::services::delete_env))
        .route(
            "/api/services/:id/ports",
            get(routes::services::list_ports).put(routes::services::set_ports),
        )
        .route("/ws", get(routes::ws::ws_handler))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("listening on 0.0.0.0:8080");
    axum::serve(listener, app).await?;
    Ok(())
}
