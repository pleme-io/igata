use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::communicator::ssh::SshCommunicator;
use crate::traits::{Artifact, Builder, Communicator};

/// QEMU builder: launches a QEMU VM, waits for SSH, then provisions.
pub struct QemuBuilder {
    state: Arc<Mutex<QemuState>>,
}

struct QemuState {
    process: Option<tokio::process::Child>,
    output_dir: Option<PathBuf>,
    disk_image: Option<PathBuf>,
    ssh_port: u16,
}

impl QemuBuilder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(QemuState {
                process: None,
                output_dir: None,
                disk_image: None,
                ssh_port: 0,
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
impl Builder for QemuBuilder {
    fn prepare(&self, config: &HashMap<String, Value>) -> Result<()> {
        // Require either iso_url or disk_image
        if get_str(config, "iso_url").is_none() && get_str(config, "disk_image").is_none() {
            anyhow::bail!("QEMU builder requires 'iso_url' or 'disk_image'");
        }
        Ok(())
    }

    async fn run(
        &self,
        config: &HashMap<String, Value>,
    ) -> Result<Option<Box<dyn Communicator>>> {
        let qemu_binary = get_str(config, "qemu_binary")
            .unwrap_or_else(|| "qemu-system-x86_64".into());
        let headless = config
            .get("headless")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let accelerator = get_str(config, "accelerator").unwrap_or_else(|| "hvf".into());
        let memory = get_str(config, "memory").unwrap_or_else(|| "1024M".into());
        let cpus = get_str(config, "cpus").unwrap_or_else(|| "2".into());
        let ssh_host_port = get_u64(config, "ssh_host_port").unwrap_or(0) as u16;
        let ssh_username = get_str(config, "ssh_username").unwrap_or_else(|| "root".into());
        let ssh_password = get_str(config, "ssh_password");
        let ssh_private_key_file = get_str(config, "ssh_private_key_file");
        let ssh_timeout = get_u64(config, "ssh_timeout").unwrap_or(300);

        // Create output directory
        let output_dir = get_str(config, "output_directory")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let name = get_str(config, "vm_name").unwrap_or_else(|| "packer-qemu".into());
                PathBuf::from(format!("output-{name}"))
            });
        std::fs::create_dir_all(&output_dir)
            .with_context(|| format!("creating {}", output_dir.display()))?;

        // Copy/create disk image
        let disk_image = if let Some(src) = get_str(config, "disk_image") {
            let dst = output_dir.join("disk.qcow2");
            std::fs::copy(&src, &dst)
                .with_context(|| format!("copying disk image {src}"))?;
            dst
        } else {
            let disk_size = get_str(config, "disk_size").unwrap_or_else(|| "40G".into());
            let dst = output_dir.join("disk.qcow2");
            let status = std::process::Command::new("qemu-img")
                .args(["create", "-f", "qcow2"])
                .arg(&dst)
                .arg(&disk_size)
                .status()
                .context("qemu-img create failed")?;
            if !status.success() {
                anyhow::bail!("qemu-img create failed");
            }
            dst
        };

        // Find a free port for SSH if not specified
        let ssh_port = if ssh_host_port > 0 {
            ssh_host_port
        } else {
            // Use a random port
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            listener.local_addr()?.port()
        };

        // Build QEMU command
        let mut args: Vec<String> = Vec::new();

        if headless {
            args.extend(["-display".into(), "none".into()]);
        }
        args.extend(["-machine".into(), format!("accel={accelerator}")]);
        args.extend(["-m".into(), memory]);
        args.extend(["-smp".into(), cpus]);
        args.extend([
            "-drive".into(),
            format!("file={},if=virtio,format=qcow2", disk_image.display()),
        ]);
        args.extend([
            "-netdev".into(),
            format!("user,id=net0,hostfwd=tcp::{ssh_port}-:22"),
        ]);
        args.extend(["-device".into(), "virtio-net-pci,netdev=net0".into()]);

        // ISO
        if let Some(iso_url) = get_str(config, "iso_url") {
            args.extend(["-cdrom".into(), iso_url]);
            if let Some(boot_command) = get_str(config, "boot_command") {
                args.extend(["-boot".into(), boot_command]);
            }
        }

        // Launch QEMU
        let child = tokio::process::Command::new(&qemu_binary)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .with_context(|| format!("failed to launch {qemu_binary}"))?;

        {
            let mut state = self.state.lock().await;
            state.process = Some(child);
            state.output_dir = Some(output_dir);
            state.disk_image = Some(disk_image);
            state.ssh_port = ssh_port;
        }

        // Wait for SSH to become available
        let deadline = tokio::time::Instant::now()
            + tokio::time::Duration::from_secs(ssh_timeout);

        loop {
            if tokio::time::Instant::now() > deadline {
                anyhow::bail!("SSH timeout after {ssh_timeout}s waiting for QEMU VM");
            }

            let connect_result = if let Some(ref key_path) = ssh_private_key_file {
                SshCommunicator::connect_key(
                    "127.0.0.1",
                    ssh_port,
                    &ssh_username,
                    &PathBuf::from(key_path),
                )
                .await
            } else if let Some(ref password) = ssh_password {
                SshCommunicator::connect_password("127.0.0.1", ssh_port, &ssh_username, password)
                    .await
            } else {
                anyhow::bail!("QEMU builder requires 'ssh_password' or 'ssh_private_key_file'");
            };

            match connect_result {
                Ok(comm) => return Ok(Some(Box::new(comm))),
                Err(_) => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    async fn artifact(&self) -> Result<Artifact> {
        let state = self.state.lock().await;
        let disk = state
            .disk_image
            .as_ref()
            .context("no disk image")?
            .display()
            .to_string();

        Ok(Artifact {
            id: disk.clone(),
            description: format!("QEMU disk image: {disk}"),
            files: vec![disk.clone()],
            builder_type: "qemu".into(),
            builder_name: String::new(),
            metadata: HashMap::from([("disk_image".into(), disk)]),
        })
    }

    async fn cleanup(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        if let Some(ref mut child) = state.process {
            let _ = child.kill().await;
        }
        Ok(())
    }
}
