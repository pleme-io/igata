use anyhow::{Context, Result};
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::Docker;
use futures_util::StreamExt;
use std::path::Path;

use crate::traits::{CommandOutput, Communicator};

/// Docker communicator using bollard to exec into a running container.
pub struct DockerCommunicator {
    docker: Docker,
    container_id: String,
}

impl DockerCommunicator {
    pub fn new(docker: Docker, container_id: String) -> Self {
        Self {
            docker,
            container_id,
        }
    }
}

#[async_trait::async_trait]
impl Communicator for DockerCommunicator {
    async fn upload(&self, src: &Path, dst: &str) -> Result<()> {
        let data = tokio::fs::read(src)
            .await
            .with_context(|| format!("failed to read {}", src.display()))?;

        // Create a tar archive with the file
        let mut ar = tar::Builder::new(Vec::new());
        let filename = Path::new(dst)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        ar.append_data(&mut header, &*filename, &data[..])
            .context("failed to create tar archive")?;
        let tar_data = ar.into_inner().context("failed to finalize tar")?;

        let dst_dir = Path::new(dst)
            .parent()
            .unwrap_or(Path::new("/"))
            .to_string_lossy()
            .to_string();

        self.docker
            .upload_to_container(
                &self.container_id,
                Some(bollard::container::UploadToContainerOptions {
                    path: dst_dir,
                    ..Default::default()
                }),
                tar_data.into(),
            )
            .await
            .context("docker upload failed")?;

        Ok(())
    }

    async fn download(&self, src: &str, dst: &Path) -> Result<()> {
        let mut stream = self.docker.download_from_container(
            &self.container_id,
            Some(bollard::container::DownloadFromContainerOptions { path: src }),
        );

        let mut tar_data = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("docker download stream error")?;
            tar_data.extend_from_slice(&chunk);
        }

        // Extract first file from the tar synchronously (tar::Archive is !Send)
        let contents = {
            let mut archive = tar::Archive::new(&tar_data[..]);
            let mut result = Vec::new();
            for entry in archive.entries().context("tar entries")? {
                let mut entry = entry.context("tar entry")?;
                std::io::Read::read_to_end(&mut entry, &mut result)?;
                break;
            }
            result
        };

        tokio::fs::write(dst, &contents)
            .await
            .with_context(|| format!("failed to write {}", dst.display()))?;

        Ok(())
    }

    async fn exec(&self, command: &str) -> Result<CommandOutput> {
        let exec = self
            .docker
            .create_exec(
                &self.container_id,
                CreateExecOptions {
                    cmd: Some(vec!["sh", "-c", command]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .context("docker create exec failed")?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } =
            self.docker.start_exec(&exec.id, None).await?
        {
            while let Some(msg) = output.next().await {
                match msg? {
                    LogOutput::StdOut { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    LogOutput::StdErr { message } => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
        }

        let inspect = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .context("docker inspect exec failed")?;
        let exit_code = inspect.exit_code.unwrap_or(0) as i32;

        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code,
        })
    }
}
