use super::{ContainerState, Orchestrator, RunSpec};
use async_trait::async_trait;
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, LogOutput, LogsOptions,
    RemoveContainerOptions, StopContainerOptions,
};
use bollard::network::CreateNetworkOptions;
use bollard::secret::{HostConfig, Mount, MountTypeEnum, PortBinding};
use bollard::Docker;
use futures_util::StreamExt;
use std::collections::HashMap;

pub struct DockerOrchestrator {
    client: Docker,
}

impl DockerOrchestrator {
    pub fn connect() -> anyhow::Result<Self> {
        let client = Docker::connect_with_local_defaults()?;
        Ok(Self { client })
    }
}

#[async_trait]
impl Orchestrator for DockerOrchestrator {
    async fn ensure_network(&self, name: &str) -> anyhow::Result<()> {
        let existing = self.client.list_networks::<String>(None).await?;
        if existing.iter().any(|n| n.name.as_deref() == Some(name)) {
            return Ok(());
        }
        self.client
            .create_network(CreateNetworkOptions {
                name: name.to_string(),
                driver: "bridge".to_string(),
                ..Default::default()
            })
            .await?;
        Ok(())
    }

    async fn remove_network(&self, name: &str) -> anyhow::Result<()> {
        let _ = self.client.remove_network(name).await;
        Ok(())
    }

    async fn run(&self, spec: RunSpec) -> anyhow::Result<String> {
        // Pull the image if it isn't present locally.
        let images = self.client.list_images::<String>(None).await?;
        let have = images.iter().any(|i| i.repo_tags.contains(&spec.image));
        if !have {
            use bollard::image::CreateImageOptions;
            let mut stream = self.client.create_image(
                Some(CreateImageOptions {
                    from_image: spec.image.clone(),
                    ..Default::default()
                }),
                None,
                None,
            );
            while let Some(item) = stream.next().await {
                item?;
            }
        }

        // Remove any stale container with the same name first.
        let _ = self
            .client
            .remove_container(
                &spec.container_name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;

        let env: Vec<String> = spec
            .env
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();

        let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
        let mut exposed_ports: HashMap<String, HashMap<(), ()>> = HashMap::new();
        for (container_port, host_port) in &spec.published_ports {
            let key = format!("{container_port}/tcp");
            exposed_ports.insert(key.clone(), HashMap::new());
            port_bindings.insert(
                key,
                Some(vec![PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(host_port.to_string()),
                }]),
            );
        }

        let mounts = spec.volume.as_ref().map(|(vol, path)| {
            vec![Mount {
                target: Some(path.clone()),
                source: Some(vol.clone()),
                typ: Some(MountTypeEnum::VOLUME),
                ..Default::default()
            }]
        });

        let host_config = HostConfig {
            network_mode: Some(spec.network.clone()),
            port_bindings: if port_bindings.is_empty() {
                None
            } else {
                Some(port_bindings)
            },
            mounts,
            restart_policy: Some(bollard::secret::RestartPolicy {
                name: Some(bollard::secret::RestartPolicyNameEnum::UNLESS_STOPPED),
                ..Default::default()
            }),
            ..Default::default()
        };

        let config = Config {
            image: Some(spec.image.clone()),
            cmd: spec.cmd.clone(),
            env: Some(env),
            exposed_ports: if exposed_ports.is_empty() {
                None
            } else {
                Some(exposed_ports)
            },
            host_config: Some(host_config),
            ..Default::default()
        };

        let created = self
            .client
            .create_container(
                Some(CreateContainerOptions {
                    name: spec.container_name.clone(),
                    platform: None,
                }),
                config,
            )
            .await?;

        self.client
            .start_container::<String>(&created.id, None)
            .await?;

        Ok(created.id)
    }

    async fn inspect(&self, container_id: &str) -> anyhow::Result<ContainerState> {
        let mut filters = HashMap::new();
        filters.insert("id".to_string(), vec![container_id.to_string()]);
        let containers = self
            .client
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await?;

        let Some(c) = containers.into_iter().next() else {
            return Ok(ContainerState::NotFound);
        };

        match c.state.as_deref() {
            Some("running") => Ok(ContainerState::Running),
            Some("exited") => {
                let code = self
                    .client
                    .inspect_container(container_id, None)
                    .await
                    .ok()
                    .and_then(|d| d.state)
                    .and_then(|s| s.exit_code)
                    .unwrap_or(-1);
                Ok(ContainerState::Exited { code })
            }
            Some(other) => Ok(ContainerState::Other(other.to_string())),
            None => Ok(ContainerState::NotFound),
        }
    }

    async fn stop_and_remove(&self, container_id: &str) -> anyhow::Result<()> {
        let _ = self
            .client
            .stop_container(container_id, Some(StopContainerOptions { t: 10 }))
            .await;
        let _ = self
            .client
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;
        Ok(())
    }

    async fn logs(&self, container_id: &str, tail: usize) -> anyhow::Result<Vec<String>> {
        let mut stream = self.client.logs(
            container_id,
            Some(LogsOptions::<String> {
                stdout: true,
                stderr: true,
                tail: tail.to_string(),
                ..Default::default()
            }),
        );
        let mut lines = Vec::new();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(LogOutput::StdOut { message } | LogOutput::StdErr { message }) => {
                    lines.push(String::from_utf8_lossy(&message).to_string());
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        Ok(lines)
    }
}
