use super::{ContainerState, Orchestrator, RunSpec};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Drives a local Kubernetes cluster (kind/minikube/k3s/etc.) with the
/// `kubectl` binary already on PATH and a working `~/.kube/config` context —
/// the same setup described in the brief (a 4-node `kind` cluster). One
/// Kubernetes Namespace per project stands in for the per-project Docker
/// network; one Deployment + Service per app service inside it, named after
/// the service's slug so DNS names line up with what `envresolve` expects.
///
/// A published port with a host port set gets its own `kubectl port-forward`
/// child process binding `localhost:<host_port>` straight to the Service, in
/// addition to the NodePort on the Service itself — on a local `kind`
/// cluster a NodePort alone usually isn't reachable at `localhost` without
/// cluster-creation-time `extraPortMappings`, so the port-forward is what
/// actually makes "publish this port" work out of the box.
pub struct KubernetesOrchestrator {
    /// Keyed by "<namespace>/<name>/<container_port>".
    forwards: Mutex<HashMap<String, Child>>,
}

impl KubernetesOrchestrator {
    pub fn new() -> Self {
        Self {
            forwards: Mutex::new(HashMap::new()),
        }
    }

    async fn kill_forwards_for(&self, namespace: &str, name: &str) {
        let prefix = format!("{namespace}/{name}/");
        let mut forwards = self.forwards.lock().await;
        let keys: Vec<String> = forwards.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
        for key in keys {
            if let Some(mut child) = forwards.remove(&key) {
                let _ = child.kill().await;
            }
        }
    }

    /// Kills and re-creates the port-forwards for this service so they match
    /// its current published-ports list (called on every deploy/redeploy).
    async fn reconcile_port_forwards(&self, spec: &RunSpec) {
        self.kill_forwards_for(&spec.network, &spec.container_name).await;
        for (container_port, host_port) in &spec.published_ports {
            let Some(host_port) = host_port else { continue };
            let key = format!("{}/{}/{}", spec.network, spec.container_name, container_port);
            let child = Command::new("kubectl")
                .args([
                    "port-forward",
                    &format!("svc/{}", spec.container_name),
                    &format!("{host_port}:{container_port}"),
                    "-n",
                    &spec.network,
                    "--address=0.0.0.0",
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn();
            match child {
                Ok(child) => {
                    self.forwards.lock().await.insert(key, child);
                }
                Err(e) => {
                    tracing::warn!(port = container_port, error = %e, "failed to start kubectl port-forward");
                }
            }
        }
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

/// Builds the Deployment (+ optional PVC, + Service) manifest as a sequence
/// of complete lines joined with `\n`, rather than one long `format!` with
/// source-level line continuations — Rust's `\`-at-end-of-line string
/// continuation strips all leading whitespace off the following source
/// line, which silently ate this template's YAML indentation before
/// (`metadata:` ended up with no nested `name:`, and kubectl rejected the
/// object with "resource name may not be empty"). Building it line-by-line
/// like this keeps every line's indentation explicit and never trims it.
fn deployment_and_service_manifest(spec: &RunSpec) -> String {
    let name = &spec.container_name;
    let mut lines: Vec<String> = Vec::new();

    if let Some((vol_name, _)) = &spec.volume {
        lines.push("apiVersion: v1".into());
        lines.push("kind: PersistentVolumeClaim".into());
        lines.push("metadata:".into());
        lines.push(format!("  name: {vol_name}"));
        lines.push("spec:".into());
        lines.push("  accessModes: [\"ReadWriteOnce\"]".into());
        lines.push("  resources:".into());
        lines.push("    requests:".into());
        lines.push("      storage: 1Gi".into());
        lines.push("---".into());
    }

    lines.push("apiVersion: apps/v1".into());
    lines.push("kind: Deployment".into());
    lines.push("metadata:".into());
    lines.push(format!("  name: {name}"));
    lines.push("  labels:".into());
    lines.push(format!("    app: {name}"));
    lines.push("spec:".into());
    lines.push("  replicas: 1".into());
    lines.push("  selector:".into());
    lines.push("    matchLabels:".into());
    lines.push(format!("      app: {name}"));
    lines.push("  template:".into());
    lines.push("    metadata:".into());
    lines.push("      labels:".into());
    lines.push(format!("        app: {name}"));
    lines.push("    spec:".into());
    lines.push("      containers:".into());
    lines.push(format!("      - name: {name}"));
    lines.push(format!("        image: {}", spec.image));

    if let Some(cmd) = &spec.cmd {
        let items: String = cmd.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        lines.push(format!("        command: [{items}]"));
    }

    if !spec.published_ports.is_empty() {
        lines.push("        ports:".into());
        for (container_port, _) in &spec.published_ports {
            lines.push(format!("        - containerPort: {container_port}"));
        }
    }

    if !spec.env.is_empty() {
        lines.push("        env:".into());
        for (k, v) in &spec.env {
            lines.push(format!("        - name: {k}"));
            lines.push(format!("          value: {}", yaml_quote(v)));
        }
    }

    if let Some((_, path)) = &spec.volume {
        lines.push("        volumeMounts:".into());
        lines.push("        - name: data".into());
        lines.push(format!("          mountPath: {path}"));
    }
    if let Some((vol_name, _)) = &spec.volume {
        lines.push("      volumes:".into());
        lines.push("      - name: data".into());
        lines.push("        persistentVolumeClaim:".into());
        lines.push(format!("          claimName: {vol_name}"));
    }

    lines.push("---".into());
    lines.push("apiVersion: v1".into());
    lines.push("kind: Service".into());
    lines.push("metadata:".into());
    lines.push(format!("  name: {name}"));
    lines.push("spec:".into());

    if spec.published_ports.is_empty() {
        // Always publish a ClusterIP service so sibling pods can resolve
        // this one by name even when nothing is externally exposed.
        lines.push("  type: ClusterIP".into());
        lines.push("  selector:".into());
        lines.push(format!("    app: {name}"));
        lines.push("  ports:".into());
        lines.push("  - port: 80".into());
        lines.push("    targetPort: 80".into());
    } else {
        let has_node_port = spec.published_ports.iter().any(|(_, hp)| hp.is_some());
        lines.push(format!("  type: {}", if has_node_port { "NodePort" } else { "ClusterIP" }));
        lines.push("  selector:".into());
        lines.push(format!("    app: {name}"));
        lines.push("  ports:".into());
        for (i, (container_port, host_port)) in spec.published_ports.iter().enumerate() {
            lines.push(format!("  - port: {container_port}"));
            lines.push(format!("    targetPort: {container_port}"));
            lines.push(format!("    name: port-{i}"));
            if let Some(node_port) = host_port {
                if (30000..=32767).contains(node_port) {
                    lines.push(format!("    nodePort: {node_port}"));
                }
            }
        }
    }

    lines.join("\n") + "\n"
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
        self.reconcile_port_forwards(&spec).await;
        Ok(format!("{}/{}", spec.network, spec.container_name))
    }

    async fn inspect(&self, container_id: &str) -> anyhow::Result<ContainerState> {
        let (namespace, name) = split_id(container_id)?;
        let Some(pod) = first_pod(namespace, name).await? else {
            return Ok(ContainerState::NotFound);
        };
        Ok(pod_container_state(&pod))
    }

    async fn restart(&self, container_id: &str) -> anyhow::Result<()> {
        let (namespace, name) = split_id(container_id)?;
        kubectl(&[
            "rollout",
            "restart",
            &format!("deployment/{name}"),
            "-n",
            namespace,
        ])
        .await?;
        Ok(())
    }

    async fn describe(&self, container_id: &str) -> anyhow::Result<serde_json::Value> {
        let (namespace, name) = split_id(container_id)?;

        let deployment = kubectl(&["get", "deployment", name, "-n", namespace, "-o", "json"])
            .await
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        let deployment_summary = deployment.as_ref().map(|d| {
            let status = d.get("status").cloned().unwrap_or_default();
            let replicas = status.get("replicas").and_then(|v| v.as_i64()).unwrap_or(0);
            let ready = status.get("readyReplicas").and_then(|v| v.as_i64()).unwrap_or(0);
            let available = status.get("availableReplicas").and_then(|v| v.as_i64()).unwrap_or(0);
            let updated = status.get("updatedReplicas").and_then(|v| v.as_i64()).unwrap_or(0);
            let condition = status
                .get("conditions")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.iter().find(|c| c.get("type").and_then(|t| t.as_str()) == Some("Available")))
                .and_then(|c| c.get("status"))
                .and_then(|s| s.as_str());
            let status_label = if available >= replicas.max(1) && replicas > 0 {
                "Available"
            } else if condition == Some("False") && replicas == 0 {
                "Scaled to zero"
            } else {
                "Progressing"
            };
            serde_json::json!({
                "name": name,
                "status": status_label,
                "replicas": replicas,
                "ready": ready,
                "available": available,
                "updated": updated,
            })
        });

        let pods_json = kubectl(&["get", "pods", "-n", namespace, "-l", &format!("app={name}"), "-o", "json"])
            .await
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        let pods: Vec<serde_json::Value> = pods_json
            .as_ref()
            .and_then(|v| v.get("items"))
            .and_then(|v| v.as_array())
            .map(|items| items.iter().map(pod_summary).collect())
            .unwrap_or_default();

        Ok(serde_json::json!({
            "kind": "kubernetes",
            "namespace": namespace,
            "deployment": deployment_summary,
            "pods": pods,
        }))
    }

    async fn stop_and_remove(&self, container_id: &str) -> anyhow::Result<()> {
        let (namespace, name) = split_id(container_id)?;
        self.kill_forwards_for(namespace, name).await;
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
        let mut forwards = self.forwards.lock().await;
        let prefix = format!("{namespace}/");
        let keys: Vec<String> = forwards.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
        for key in keys {
            if let Some(mut child) = forwards.remove(&key) {
                let _ = child.kill().await;
            }
        }
        drop(forwards);
        let _ = kubectl(&["delete", "namespace", namespace, "--ignore-not-found"]).await;
        Ok(())
    }
}

/// Fetches the first pod matching `app=<name>` in `namespace`, parsed as raw
/// JSON, or `None` if there isn't one yet (still scheduling, or gone).
async fn first_pod(namespace: &str, name: &str) -> anyhow::Result<Option<serde_json::Value>> {
    let out = kubectl(&["get", "pods", "-n", namespace, "-l", &format!("app={name}"), "-o", "json"]).await;
    let json_str = match out {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
    Ok(parsed
        .get("items")
        .and_then(|v| v.as_array())
        .and_then(|items| items.first())
        .cloned())
}

/// Maps a single Pod's JSON into our `ContainerState`, distinguishing normal
/// startup (`Pending`) from genuinely actionable problems (image pull
/// failure, crash-loop, terminal exit) — this is the mapping that used to
/// collapse everything non-`Running` into "failed" after ~30s, which is what
/// made a slow-but-healthy image pull look like a broken deploy.
fn pod_container_state(pod: &serde_json::Value) -> ContainerState {
    let phase = pod.pointer("/status/phase").and_then(|v| v.as_str()).unwrap_or("");
    let cs = pod.pointer("/status/containerStatuses/0");
    let restart_count = cs.and_then(|c| c.get("restartCount")).and_then(|v| v.as_i64()).unwrap_or(0);
    let ready = cs.and_then(|c| c.get("ready")).and_then(|v| v.as_bool()).unwrap_or(false);

    if let Some(waiting) = cs.and_then(|c| c.pointer("/state/waiting")) {
        let reason = waiting.get("reason").and_then(|v| v.as_str()).unwrap_or("");
        return match reason {
            "ImagePullBackOff" | "ErrImagePull" | "InvalidImageName" | "ImageInspectError" => {
                let message = waiting.get("message").and_then(|v| v.as_str()).unwrap_or(reason);
                ContainerState::ImagePullError { reason: message.to_string() }
            }
            "CrashLoopBackOff" => ContainerState::CrashLoopBackOff { restarts: restart_count },
            "" => ContainerState::Pending { reason: None },
            other => ContainerState::Pending { reason: Some(other.to_string()) },
        };
    }

    if let Some(terminated) = cs.and_then(|c| c.pointer("/state/terminated")) {
        let code = terminated.get("exitCode").and_then(|v| v.as_i64()).unwrap_or(-1);
        return ContainerState::Exited { code };
    }

    match phase {
        "Running" if ready => ContainerState::Running,
        "Running" => ContainerState::Pending { reason: Some("starting".to_string()) },
        "Pending" => ContainerState::Pending { reason: Some("scheduling".to_string()) },
        "Succeeded" => ContainerState::Exited { code: 0 },
        "Failed" => ContainerState::Exited { code: 1 },
        "" => ContainerState::NotFound,
        other => ContainerState::Other(other.to_string()),
    }
}

fn pod_summary(pod: &serde_json::Value) -> serde_json::Value {
    let name = pod.pointer("/metadata/name").and_then(|v| v.as_str()).unwrap_or("");
    let phase = pod.pointer("/status/phase").and_then(|v| v.as_str()).unwrap_or("Unknown");
    let cs = pod.pointer("/status/containerStatuses/0");
    let ready = cs.and_then(|c| c.get("ready")).and_then(|v| v.as_bool()).unwrap_or(false);
    let restarts = cs.and_then(|c| c.get("restartCount")).and_then(|v| v.as_i64()).unwrap_or(0);
    let reason = cs
        .and_then(|c| c.pointer("/state/waiting/reason").or_else(|| c.pointer("/state/terminated/reason")))
        .and_then(|v| v.as_str());
    serde_json::json!({
        "name": name,
        "status": reason.unwrap_or(phase),
        "phase": phase,
        "ready": ready,
        "restarts": restarts,
    })
}

fn split_id(container_id: &str) -> anyhow::Result<(&str, &str)> {
    container_id
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("malformed kubernetes handle {container_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn manifest_has_correctly_indented_metadata_name() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        let spec = RunSpec {
            container_name: "with-kubernetes-ghcr-io-love-solana-sqlx-v1-0-13".to_string(),
            image: "ghcr.io/love-solana/sqlx:v1.0.13".to_string(),
            network: "with-kubernetes".to_string(),
            env,
            published_ports: vec![(3000, Some(30080))],
            volume: None,
            cmd: None,
        };
        let manifest = deployment_and_service_manifest(&spec);
        println!("{manifest}");
        assert!(manifest.contains("metadata:\n  name: with-kubernetes-ghcr-io-love-solana-sqlx-v1-0-13\n"));
        assert!(!manifest.contains("\nname:")); // would indicate an un-indented, top-level `name:`
    }

    fn pod_json(v: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "status": v })
    }

    #[test]
    fn pending_pod_pulling_image_is_pending_not_failed() {
        // This is the exact shape kubectl returns while a slow ghcr.io pull is
        // in progress: phase Pending, container waiting on ContainerCreating.
        let pod = pod_json(serde_json::json!({
            "phase": "Pending",
            "containerStatuses": [{"ready": false, "restartCount": 0, "state": {"waiting": {"reason": "ContainerCreating"}}}],
        }));
        assert_eq!(
            pod_container_state(&pod),
            ContainerState::Pending { reason: Some("ContainerCreating".to_string()) }
        );
    }

    #[test]
    fn scheduling_with_no_container_statuses_yet_is_pending() {
        let pod = pod_json(serde_json::json!({ "phase": "Pending" }));
        assert_eq!(
            pod_container_state(&pod),
            ContainerState::Pending { reason: Some("scheduling".to_string()) }
        );
    }

    #[test]
    fn image_pull_backoff_is_a_real_error() {
        let pod = pod_json(serde_json::json!({
            "phase": "Pending",
            "containerStatuses": [{"ready": false, "restartCount": 0, "state": {"waiting": {"reason": "ImagePullBackOff", "message": "rate limited"}}}],
        }));
        assert_eq!(
            pod_container_state(&pod),
            ContainerState::ImagePullError { reason: "rate limited".to_string() }
        );
    }

    #[test]
    fn crash_loop_is_distinct_from_a_one_shot_exit() {
        let pod = pod_json(serde_json::json!({
            "phase": "Running",
            "containerStatuses": [{"ready": false, "restartCount": 4, "state": {"waiting": {"reason": "CrashLoopBackOff"}}}],
        }));
        assert_eq!(pod_container_state(&pod), ContainerState::CrashLoopBackOff { restarts: 4 });
    }

    #[test]
    fn ready_running_pod_is_running() {
        let pod = pod_json(serde_json::json!({
            "phase": "Running",
            "containerStatuses": [{"ready": true, "restartCount": 0, "state": {"running": {"startedAt": "now"}}}],
        }));
        assert_eq!(pod_container_state(&pod), ContainerState::Running);
    }
}
