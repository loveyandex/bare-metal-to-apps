pub mod docker;

use async_trait::async_trait;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RunSpec {
    pub container_name: String,
    pub image: String,
    pub network: String,
    pub env: HashMap<String, String>,
    /// (container_port, host_port) — only used for services that need a
    /// host-published port (e.g. dev convenience). Internal service-to-service
    /// traffic uses the docker network DNS name instead.
    pub published_ports: Vec<(u16, u16)>,
    pub volume: Option<(String, String)>, // (volume_name, mount_path)
    pub cmd: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerState {
    Running,
    Exited { code: i64 },
    NotFound,
    Other(String),
}

#[async_trait]
pub trait Orchestrator: Send + Sync {
    async fn ensure_network(&self, name: &str) -> anyhow::Result<()>;
    async fn run(&self, spec: RunSpec) -> anyhow::Result<String>; // returns container id
    async fn inspect(&self, container_id: &str) -> anyhow::Result<ContainerState>;
    async fn stop_and_remove(&self, container_id: &str) -> anyhow::Result<()>;
    async fn logs(&self, container_id: &str, tail: usize) -> anyhow::Result<Vec<String>>;
    async fn remove_network(&self, name: &str) -> anyhow::Result<()>;
}
