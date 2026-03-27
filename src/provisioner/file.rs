use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use crate::traits::{Communicator, Provisioner};

/// File provisioner: uploads files/directories to the machine.
///
/// Packer-compatible fields:
/// - `source`: single file or directory path
/// - `sources`: array of file paths (uploaded to same destination directory)
/// - `content`: inline string content to write
/// - `destination`: remote path (required)
/// - `direction`: "upload" (default) or "download"
/// - `generated`: if true, skip pre-build file existence check
pub struct FileProvisioner;

#[async_trait::async_trait]
impl Provisioner for FileProvisioner {
    async fn provision(
        &self,
        config: &HashMap<String, Value>,
        comm: Option<&dyn Communicator>,
    ) -> Result<()> {
        let comm = comm.context("file provisioner requires a communicator")?;

        let destination = config
            .get("destination")
            .and_then(|v| v.as_str())
            .context("file provisioner requires 'destination'")?;
        let direction = config
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("upload");

        match direction {
            "upload" => {
                // Packer: `content` creates an inline file
                if let Some(Value::String(content)) = config.get("content") {
                    let tmp = std::env::temp_dir().join("igata-file-content");
                    std::fs::write(&tmp, content)?;
                    comm.upload(&tmp, destination).await?;
                    let _ = std::fs::remove_file(&tmp);
                    return Ok(());
                }

                // Packer: `sources` uploads multiple files to same directory
                if let Some(Value::Array(sources)) = config.get("sources") {
                    for src in sources {
                        if let Value::String(path) = src {
                            let src_path = Path::new(path);
                            let fname = src_path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            let dst = format!("{destination}/{fname}");
                            comm.upload(src_path, &dst).await?;
                        }
                    }
                    return Ok(());
                }

                // Packer: `source` uploads single file or directory
                let source = config
                    .get("source")
                    .and_then(|v| v.as_str())
                    .context("file provisioner requires 'source', 'sources', or 'content'")?;

                let src_path = Path::new(source);
                if src_path.is_dir() {
                    upload_dir(comm, src_path, destination).await?;
                } else {
                    comm.upload(src_path, destination).await?;
                }
            }
            "download" => {
                let source = config
                    .get("source")
                    .and_then(|v| v.as_str())
                    .context("file provisioner download requires 'source'")?;
                let dst_path = Path::new(destination);
                comm.download(source, dst_path).await?;
            }
            other => {
                anyhow::bail!("unknown direction: {other} (expected 'upload' or 'download')");
            }
        }

        Ok(())
    }
}

async fn upload_dir(comm: &dyn Communicator, src: &Path, dst: &str) -> Result<()> {
    comm.exec(&format!("mkdir -p {dst}")).await?;

    let entries = std::fs::read_dir(src)
        .with_context(|| format!("reading directory {}", src.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let remote_path = format!("{dst}/{name}");

        if path.is_dir() {
            Box::pin(upload_dir(comm, &path, &remote_path)).await?;
        } else {
            comm.upload(&path, &remote_path).await?;
        }
    }

    Ok(())
}
