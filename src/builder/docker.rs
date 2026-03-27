use anyhow::{Context, Result};
use bollard::container::{Config as ContainerConfig, CreateContainerOptions};
use bollard::image::CommitContainerOptions;
use bollard::Docker;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::communicator::docker::DockerCommunicator;
use crate::traits::{Artifact, Builder, Communicator};

/// Docker builder: runs a container, provisions it, then commits the result.
pub struct DockerBuilder {
    state: Arc<Mutex<DockerState>>,
}

struct DockerState {
    docker: Option<Docker>,
    container_id: Option<String>,
    image_id: Option<String>,
    image: String,
}

impl DockerBuilder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(DockerState {
                docker: None,
                container_id: None,
                image_id: None,
                image: String::new(),
            })),
        }
    }
}

fn get_str(config: &HashMap<String, Value>, key: &str) -> Option<String> {
    config.get(key).and_then(|v| v.as_str()).map(String::from)
}

#[async_trait::async_trait]
impl Builder for DockerBuilder {
    fn prepare(&self, config: &HashMap<String, Value>) -> Result<()> {
        if get_str(config, "image").is_none() {
            anyhow::bail!("docker builder requires 'image' field");
        }
        Ok(())
    }

    async fn run(
        &self,
        config: &HashMap<String, Value>,
    ) -> Result<Option<Box<dyn Communicator>>> {
        let image = get_str(config, "image").unwrap();
        let docker = Docker::connect_with_local_defaults()
            .context("failed to connect to Docker")?;

        // Pull image
        {
            use bollard::image::CreateImageOptions;
            use futures_util::StreamExt;

            let mut pull_stream = docker.create_image(
                Some(CreateImageOptions {
                    from_image: image.as_str(),
                    ..Default::default()
                }),
                None,
                None,
            );
            while let Some(result) = pull_stream.next().await {
                result.context("docker pull failed")?;
            }
        }

        // Create and start container
        let cmd: Option<Vec<String>> = config.get("run_command").and_then(|v| {
            v.as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        });

        let container_config = ContainerConfig {
            image: Some(image.clone()),
            cmd: cmd.clone(),
            tty: Some(true),
            open_stdin: Some(true),
            ..Default::default()
        };

        let container = docker
            .create_container(
                Some(CreateContainerOptions::<String> {
                    ..Default::default()
                }),
                container_config,
            )
            .await
            .context("failed to create container")?;

        docker
            .start_container::<String>(&container.id, None)
            .await
            .context("failed to start container")?;

        let container_id = container.id.clone();

        {
            let mut state = self.state.lock().await;
            state.docker = Some(docker.clone());
            state.container_id = Some(container_id.clone());
            state.image = image;
        }

        let comm = DockerCommunicator::new(docker, container_id);
        Ok(Some(Box::new(comm)))
    }

    async fn artifact(&self) -> Result<Artifact> {
        let mut state = self.state.lock().await;
        let docker = state.docker.as_ref().context("docker not initialized")?;
        let container_id = state
            .container_id
            .as_ref()
            .context("no container running")?;

        // Commit the container to create an image
        let commit = docker
            .commit_container(
                CommitContainerOptions {
                    container: container_id.as_str(),
                    ..Default::default()
                },
                ContainerConfig::<String>::default(),
            )
            .await
            .context("docker commit failed")?;

        let image_id = commit.id.clone().unwrap_or_default();
        state.image_id = Some(image_id.clone());

        Ok(Artifact {
            id: image_id.clone(),
            description: format!("Docker image: {image_id}"),
            files: Vec::new(),
            builder_type: "docker".into(),
            builder_name: String::new(),
            metadata: HashMap::from([("image_id".into(), image_id)]),
        })
    }

    async fn cleanup(&self) -> Result<()> {
        let state = self.state.lock().await;
        if let (Some(docker), Some(container_id)) = (&state.docker, &state.container_id) {
            let _ = docker.stop_container(container_id, None).await;
            let _ = docker
                .remove_container(
                    container_id,
                    Some(bollard::container::RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;
        }
        Ok(())
    }
}
