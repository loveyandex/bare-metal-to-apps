use super::{ContainerState, Orchestrator, RunSpec};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Drives a local Kubernetes cluster (kind/minikube/k3s/etc.) with the
/// `kubectl` binary already on PATH and a working `~/.kube/config` context —
/// the same setup described in the brief (a 4-node `kind` cluster). One
/// Kubernetes Namespace per project stands in for the per-project Docker
/// network; one Deployment + Service per app service inside it, named after
/// the service's slug so DNS names line up with what `envresolve` expects.
pub struct KubernetesOrchestrator;

impl KubernetesOrchestrator {
    pub fn new() -> Self {
        Self
    }
}

async fn kubectl(args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("kubectl").args(args).output().await?;
    if !output.status.success() {
        anyhow::bail!(
            "kubectl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn kubectl_apply(namespace: Option<&str>, manifest: String) -> anyhow::Result<()> {
    let mut args = vec!["apply"];
    if let Some(ns) = namespace {
        args.extend(["-n", ns]);
    }
    args.extend(["-f", "-"]);
    let mut child = Command::new("kubectl")
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(manifest.as_bytes())
        .await?;
    let output = child.wait_with_output().await?;
    if !output.status.success() {
        anyhow::bail!(
            "kubectl apply failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Minimal YAML-safe double-quoted scalar (handles the arbitrary passwords,
/// URLs, and image refs we pass through — full YAML string escaping isn't
/// needed since we control the surrounding structure).
fn yaml_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn deployment_and_service_manifest(spec: &RunSpec) -> String {
    let name = &spec.container_name;
    let mut doc = String::new();

    let env_yaml: String = if spec.env.is_empty() {
        String::new()
    } else {
        let mut s = String::from("\n        env:\n");
        for (k, v) in &spec.env {
            s.push_str(&format!(
                "        - name: {k}\n          value: {}\n",
                yaml_quote(v)
            ));
        }
        s
    };

    let cmd_yaml: String = spec
        .cmd
        .as_ref()
        .map(|cmd| {
            let items: String = cmd.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
            format!("\n        command: [{items}]")
        })
        .unwrap_or_default();

    let ports_yaml: String = if spec.published_ports.is_empty() {
        String::new()
    } else {
        let mut s = String::from("\n        ports:\n");
        for (container_port, _) in &spec.published_ports {
            s.push_str(&format!("        - containerPort: {container_port}\n"));
        }
        s
    };

    let (volume_mount_yaml, volume_yaml, pvc_yaml) = match &spec.volume {
        Some((vol_name, path)) => (
            format!("\n        volumeMounts:\n        - name: data\n          mountPath: {path}"),
            format!("\n      volumes:\n      - name: data\n        persistentVolumeClaim:\n          claimName: {vol_name}"),
            format!(
                "---\napiVersion: v1\nkind: PersistentVolumeClaim\nmetadata:\n  name: {vol_name}\nspec:\n  accessModes: [\"ReadWriteOnce\"]\n  resources:\n    requests:\n      storage: 1Gi\n---\n"
            ),
        ),
        None => (String::new(), String::new(), String::new()),
    };

    doc.push_str(&pvc_yaml);
    doc.push_str(&format!(
        "apiVersion: apps/v1\n\
kind: Deployment\n\
metadata:\n\
  name: {name}\n\
  labels:\n\
    app: {name}\n\
spec:\n\
  replicas: 1\n\
  selector:\n\
    matchLabels:\n\
      app: {name}\n\
  template:\n\
    metadata:\n\
      labels:\n\
        app: {name}\n\
    spec:\n\
      containers:\n\
      - name: {name}\n\
        image: {image}{cmd_yaml}{ports_yaml}{env_yaml}{volume_mount_yaml}{volume_yaml}\n",
        image = spec.image,
    ));

    if !spec.published_ports.is_empty() {
        let has_node_port = spec.published_ports.iter().any(|(_, hp)| hp.is_some());
        let svc_type = if has_node_port { "NodePort" } else { "ClusterIP" };
        let mut svc = format!(
            "---\napiVersion: v1\nkind: Service\nmetadata:\n  name: {name}\nspec:\n  type: {svc_type}\n  selector:\n    app: {name}\n  ports:\n"
        );
        for (i, (container_port, host_port)) in spec.published_ports.iter().enumerate() {
            svc.push_str(&format!(
                "  - port: {container_port}\n    targetPort: {container_port}\n    name: port-{i}\n"
            ));
            if let Some(node_port) = host_port {
                svc.push_str(&format!("    nodePort: {node_port}\n"));
            }
        }
        doc.push_str(&svc);
    } else {
        // Always publish a ClusterIP service so sibling pods can resolve this
        // service by name even when nothing is externally exposed.
        doc.push_str(&format!(
            "---\napiVersion: v1\nkind: Service\nmetadata:\n  name: {name}\nspec:\n  type: ClusterIP\n  selector:\n    app: {name}\n  ports:\n  - port: 80\n    targetPort: 80\n"
        ));
    }

    doc
}

#[async_trait]
impl Orchestrator for KubernetesOrchestrator {
    async fn ensure_network(&self, namespace: &str) -> anyhow::Result<()> {
        let manifest = format!("apiVersion: v1\nkind: Namespace\nmetadata:\n  name: {namespace}\n");
        kubectl_apply(None, manifest).await
    }

    async fn run(&self, spec: RunSpec) -> anyhow::Result<String> {
        let manifest = deployment_and_service_manifest(&spec);
        kubectl_apply(Some(&spec.network), manifest).await?;
        Ok(format!("{}/{}", spec.network, spec.container_name))
    }

    async fn inspect(&self, container_id: &str) -> anyhow::Result<ContainerState> {
        let (namespace, name) = split_id(container_id)?;
        let out = kubectl(&[
            "get",
            "pods",
            "-n",
            namespace,
            "-l",
            &format!("app={name}"),
            "-o",
            "jsonpath={.items[0].status.phase}",
        ])
        .await;
        let phase = match out {
            Ok(p) => p,
            Err(_) => return Ok(ContainerState::NotFound),
        };
        Ok(match phase.trim() {
            "Running" => ContainerState::Running,
            "Failed" => ContainerState::Exited { code: 1 },
            "" => ContainerState::NotFound,
            other => ContainerState::Other(other.to_string()),
        })
    }

    async fn stop_and_remove(&self, container_id: &str) -> anyhow::Result<()> {
        let (namespace, name) = split_id(container_id)?;
        let _ = kubectl(&[
            "delete",
            "deployment,service",
            name,
            "-n",
            namespace,
            "--ignore-not-found",
        ])
        .await;
        Ok(())
    }

    async fn logs(&self, container_id: &str, tail: usize) -> anyhow::Result<Vec<String>> {
        let (namespace, name) = split_id(container_id)?;
        let tail_arg = format!("--tail={tail}");
        let out = kubectl(&[
            "logs",
            &format!("deployment/{name}"),
            "-n",
            namespace,
            &tail_arg,
        ])
        .await?;
        Ok(out.lines().map(|l| l.to_string()).collect())
    }

    async fn remove_network(&self, namespace: &str) -> anyhow::Result<()> {
        let _ = kubectl(&["delete", "namespace", namespace, "--ignore-not-found"]).await;
        Ok(())
    }
}

fn split_id(container_id: &str) -> anyhow::Result<(&str, &str)> {
    container_id
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("malformed kubernetes handle {container_id}"))
}
