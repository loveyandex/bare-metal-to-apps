use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub docker_network: String,
    pub deploy_mode: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployMode {
    Docker,
    Kubernetes,
}

impl std::fmt::Display for DeployMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployMode::Docker => write!(f, "docker"),
            DeployMode::Kubernetes => write!(f, "kubernetes"),
        }
    }
}

impl std::str::FromStr for DeployMode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "docker" => Ok(DeployMode::Docker),
            "kubernetes" => Ok(DeployMode::Kubernetes),
            other => anyhow::bail!("unknown deploy mode {other}"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PortProtocol {
    Tcp,
    Udp,
}

impl PortProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            PortProtocol::Tcp => "TCP",
            PortProtocol::Udp => "UDP",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct PortMapping {
    pub container_port: u16,
    /// Host (docker mode) or NodePort (kubernetes mode, must be 30000-32767 if set).
    pub host_port: Option<u16>,
    #[serde(default = "default_protocol")]
    pub protocol: PortProtocol,
}

fn default_protocol() -> PortProtocol {
    PortProtocol::Tcp
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text")]
pub enum ServiceKind {
    #[serde(rename = "app")]
    App,
    #[serde(rename = "database")]
    Database,
}

impl std::fmt::Display for ServiceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceKind::App => write!(f, "app"),
            ServiceKind::Database => write!(f, "database"),
        }
    }
}

impl std::str::FromStr for ServiceKind {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "app" => Ok(ServiceKind::App),
            "database" => Ok(ServiceKind::Database),
            other => Err(anyhow::anyhow!("unknown service kind {other}")),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeploySource {
    DockerImage { image: String },
    Database { engine: DbEngine },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbEngine {
    Postgres,
    Redis,
    Mysql,
    Mongodb,
}

impl DbEngine {
    pub fn image(&self) -> &'static str {
        match self {
            DbEngine::Postgres => "postgres:16-alpine",
            DbEngine::Redis => "redis:7-alpine",
            DbEngine::Mysql => "mysql:8",
            DbEngine::Mongodb => "mongo:7",
        }
    }

    pub fn internal_port(&self) -> u16 {
        match self {
            DbEngine::Postgres => 5432,
            DbEngine::Redis => 6379,
            DbEngine::Mysql => 3306,
            DbEngine::Mongodb => 27017,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DbEngine::Postgres => "postgres",
            DbEngine::Redis => "redis",
            DbEngine::Mysql => "mysql",
            DbEngine::Mongodb => "mongodb",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Creating,
    /// Scheduled/started but not serving yet — normal while an image is
    /// pulling or a pod is starting up. Not an error state.
    Pending,
    Running,
    Crashed,
    Failed,
    Stopped,
    Deleting,
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ServiceStatus::Creating => "creating",
            ServiceStatus::Pending => "pending",
            ServiceStatus::Running => "running",
            ServiceStatus::Crashed => "crashed",
            ServiceStatus::Failed => "failed",
            ServiceStatus::Stopped => "stopped",
            ServiceStatus::Deleting => "deleting",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for ServiceStatus {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "creating" => ServiceStatus::Creating,
            "pending" => ServiceStatus::Pending,
            "running" => ServiceStatus::Running,
            "crashed" => ServiceStatus::Crashed,
            "failed" => ServiceStatus::Failed,
            "stopped" => ServiceStatus::Stopped,
            "deleting" => ServiceStatus::Deleting,
            other => anyhow::bail!("unknown status {other}"),
        })
    }
}

#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct Service {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub slug: String,
    pub kind: String,
    pub deploy_source: serde_json::Value,
    pub status: String,
    pub status_message: Option<String>,
    pub container_id: Option<String>,
    pub container_name: String,
    pub desired_replicas: i32,
    pub ports: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct EnvVar {
    pub id: Uuid,
    pub service_id: Uuid,
    pub key: String,
    pub value: String,
    pub is_secret: bool,
    pub is_generated: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct EnvVarMasked {
    pub id: Uuid,
    pub key: String,
    pub value: Option<String>,
    pub is_secret: bool,
    pub is_generated: bool,
}

#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct DeployEvent {
    pub id: Uuid,
    pub service_id: Uuid,
    pub status: String,
    pub message: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<EnvVar> for EnvVarMasked {
    fn from(e: EnvVar) -> Self {
        EnvVarMasked {
            id: e.id,
            key: e.key,
            value: if e.is_secret { None } else { Some(e.value) },
            is_secret: e.is_secret,
            is_generated: e.is_generated,
        }
    }
}
