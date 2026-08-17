pub mod docker;
pub mod kubernetes;

use async_trait::async_trait;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RunSpec {
    pub container_name: String,
    pub image: String,
    pub network: String,
    pub env: HashMap<String, String>,
    /// (container_port, host_port). Internal service-to-service traffic uses
    /// the network/namespace DNS name instead — a port only needs a
    /// `host_port` here when it should be reachable from outside (published
    /// on the docker host, or a Kubernetes NodePort).
    pub published_ports: Vec<(u16, Option<u16>)>,
    pub volume: Option<(String, String)>, // (volume_name, mount_path)
    pub cmd: Option<Vec<String>>,
}

/// Point-in-time state of a running deploy target (container or pod).
/// Distinguishes "still legitimately getting there" (`Pending` — scheduling,
/// pulling the image, creating the container) from states that actually
/// warrant surfacing an error to the user. A slow registry pull is normal
/// and should read as "pending", never as "failed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerState {
    /// Scheduled/created but not serving yet — normal during image pull or
    /// container/pod startup. `reason` is a short human string when the
    /// backend knows more (e.g. Kubernetes' `ContainerCreating`,
    /// `PodInitializing`).
    Pending { reason: Option<String> },
    Running,
    /// Exited on its own (crashed) with the given exit code.
    Exited { code: i64 },
    /// Kubernetes CrashLoopBackOff (or Docker's equivalent restart-looping):
    /// the container keeps starting and dying. Distinct from a one-shot
    /// `Exited` because retrying a fresh deploy won't fix it — the image
    /// itself is failing at runtime.
    CrashLoopBackOff { restarts: i64 },
    /// The image couldn't be pulled (bad tag, private registry auth,
    /// registry down, rate-limited, ...).
    ImagePullError { reason: String },
    NotFound,
    Other(String),
}

#[async_trait]
pub trait Orchestrator: Send + Sync {
    async fn ensure_network(&self, name: &str) -> anyhow::Result<()>;
    async fn run(&self, spec: RunSpec) -> anyhow::Result<String>; // returns container id
    async fn inspect(&self, container_id: &str) -> anyhow::Result<ContainerState>;
    /// In-place restart of whatever is already running — no re-apply of the
    /// spec/image, just cycle the existing container/pod. Distinct from
    /// `run()` (a full redeploy) so a user can bounce a hung process without
    /// touching its config.
    async fn restart(&self, container_id: &str) -> anyhow::Result<()>;
    async fn stop_and_remove(&self, container_id: &str) -> anyhow::Result<()>;
    async fn logs(&self, container_id: &str, tail: usize) -> anyhow::Result<Vec<String>>;
    async fn remove_network(&self, name: &str) -> anyhow::Result<()>;
    /// Orchestrator-specific structured detail for the UI's status view —
    /// e.g. Kubernetes' Deployment rollout status + the list of Pods behind
    /// it (mirroring `kubectl get deployment,pods`), or Docker's container
    /// inspect summary. Shape is intentionally free-form JSON; the frontend
    /// renders it per deploy mode.
    async fn describe(&self, container_id: &str) -> anyhow::Result<serde_json::Value>;
}
