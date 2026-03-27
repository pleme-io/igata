use anyhow::{Context, Result};
use aws_sdk_ec2 as ec2;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::communicator::ssh::SshCommunicator;
use crate::traits::{Artifact, Builder, Communicator};

/// Amazon EBS builder: launches an EC2 instance, provisions it, creates an AMI.
pub struct AmazonEbsBuilder {
    state: Arc<Mutex<EbsState>>,
}

struct EbsState {
    client: Option<ec2::Client>,
    instance_id: Option<String>,
    ami_id: Option<String>,
    region: String,
}

impl AmazonEbsBuilder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(EbsState {
                client: None,
                instance_id: None,
                ami_id: None,
                region: String::new(),
            })),
        }
    }
}

fn get_str(config: &HashMap<String, Value>, key: &str) -> Option<String> {
    config.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn get_u64(config: &HashMap<String, Value>, key: &str) -> Option<u64> {
    config.get(key).and_then(|v| v.as_u64())
}

#[async_trait::async_trait]
impl Builder for AmazonEbsBuilder {
    fn prepare(&self, config: &HashMap<String, Value>) -> Result<()> {
        if get_str(config, "source_ami").is_none()
            && get_str(config, "source_ami_filter").is_none()
        {
            anyhow::bail!("amazon-ebs builder requires 'source_ami' or 'source_ami_filter'");
        }
        if get_str(config, "instance_type").is_none() {
            anyhow::bail!("amazon-ebs builder requires 'instance_type'");
        }
        if get_str(config, "ami_name").is_none() {
            anyhow::bail!("amazon-ebs builder requires 'ami_name'");
        }
        Ok(())
    }

    async fn run(
        &self,
        config: &HashMap<String, Value>,
    ) -> Result<Option<Box<dyn Communicator>>> {
        let region = get_str(config, "region").unwrap_or_else(|| "us-east-1".into());
        let source_ami = get_str(config, "source_ami").context("source_ami required")?;
        let instance_type = get_str(config, "instance_type").context("instance_type required")?;
        let ssh_username = get_str(config, "ssh_username").unwrap_or_else(|| "ec2-user".into());
        let ssh_private_key_file = get_str(config, "ssh_private_key_file");
        let ssh_timeout = get_u64(config, "ssh_timeout").unwrap_or(300);

        // Build AWS client
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region.clone()))
            .load()
            .await;
        let client = ec2::Client::new(&aws_config);

        // Launch instance
        let run_result = client
            .run_instances()
            .image_id(&source_ami)
            .instance_type(ec2::types::InstanceType::from(instance_type.as_str()))
            .min_count(1)
            .max_count(1)
            .send()
            .await
            .context("failed to launch EC2 instance")?;

        let instance = run_result
            .instances()
            .first()
            .context("no instance returned")?;
        let instance_id = instance
            .instance_id()
            .context("no instance ID")?
            .to_string();

        {
            let mut state = self.state.lock().await;
            state.client = Some(client.clone());
            state.instance_id = Some(instance_id.clone());
            state.region = region;
        }

        // Wait for instance to be running
        let deadline =
            tokio::time::Instant::now() + tokio::time::Duration::from_secs(ssh_timeout);

        loop {
            if tokio::time::Instant::now() > deadline {
                anyhow::bail!("timeout waiting for EC2 instance {instance_id}");
            }

            let desc = client
                .describe_instances()
                .instance_ids(&instance_id)
                .send()
                .await?;

            let inst = desc
                .reservations()
                .first()
                .and_then(|r| r.instances().first());

            if let Some(inst) = inst {
                if inst.state().and_then(|s| s.name()).map(|n| n.as_str())
                    == Some("running")
                {
                    let public_ip = inst.public_ip_address().context(
                        "instance has no public IP — ensure subnet assigns public IPs",
                    )?;

                    // Wait for SSH
                    let ssh_deadline = tokio::time::Instant::now()
                        + tokio::time::Duration::from_secs(ssh_timeout / 2);

                    loop {
                        if tokio::time::Instant::now() > ssh_deadline {
                            anyhow::bail!(
                                "SSH timeout connecting to {public_ip}"
                            );
                        }

                        let connect_result = if let Some(ref key_path) = ssh_private_key_file {
                            SshCommunicator::connect_key(
                                public_ip,
                                22,
                                &ssh_username,
                                &PathBuf::from(key_path),
                            )
                            .await
                        } else {
                            anyhow::bail!(
                                "amazon-ebs builder requires 'ssh_private_key_file'"
                            );
                        };

                        match connect_result {
                            Ok(comm) => return Ok(Some(Box::new(comm))),
                            Err(_) => {
                                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                            }
                        }
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }

    async fn artifact(&self) -> Result<Artifact> {
        let mut state = self.state.lock().await;
        let client = state.client.as_ref().context("AWS client not initialized")?;
        let instance_id = state
            .instance_id
            .as_ref()
            .context("no instance running")?;

        // Stop the instance first
        client
            .stop_instances()
            .instance_ids(instance_id)
            .send()
            .await
            .context("failed to stop instance")?;

        // Wait for stopped state
        loop {
            let desc = client
                .describe_instances()
                .instance_ids(instance_id)
                .send()
                .await?;

            let inst_state = desc
                .reservations()
                .first()
                .and_then(|r| r.instances().first())
                .and_then(|i| i.state())
                .and_then(|s| s.name())
                .map(|n| n.as_str().to_string());

            if inst_state.as_deref() == Some("stopped") {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }

        // Create AMI
        let ami_name = format!("igata-{}", chrono::Utc::now().timestamp());
        let create_image = client
            .create_image()
            .instance_id(instance_id)
            .name(&ami_name)
            .no_reboot(true)
            .send()
            .await
            .context("failed to create AMI")?;

        let ami_id = create_image
            .image_id()
            .context("no AMI ID returned")?
            .to_string();
        state.ami_id = Some(ami_id.clone());

        Ok(Artifact {
            id: ami_id.clone(),
            description: format!("AMI: {ami_id} ({ami_name})"),
            files: Vec::new(),
            builder_type: "amazon-ebs".into(),
            builder_name: String::new(),
            metadata: HashMap::from([
                ("ami_id".into(), ami_id),
                ("ami_name".into(), ami_name),
                ("region".into(), state.region.clone()),
            ]),
        })
    }

    async fn cleanup(&self) -> Result<()> {
        let state = self.state.lock().await;
        if let (Some(client), Some(instance_id)) = (&state.client, &state.instance_id) {
            let _ = client
                .terminate_instances()
                .instance_ids(instance_id)
                .send()
                .await;
        }
        Ok(())
    }
}
