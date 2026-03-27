use anyhow::Result;
use std::path::Path;

use crate::traits::{CommandOutput, Communicator};

/// A no-op communicator for builders that don't need remote access (e.g., null).
#[allow(dead_code)]
pub struct NoneCommunicator;

#[async_trait::async_trait]
impl Communicator for NoneCommunicator {
    async fn upload(&self, _src: &Path, _dst: &str) -> Result<()> {
        anyhow::bail!("null communicator does not support upload")
    }

    async fn download(&self, _src: &str, _dst: &Path) -> Result<()> {
        anyhow::bail!("null communicator does not support download")
    }

    async fn exec(&self, _command: &str) -> Result<CommandOutput> {
        anyhow::bail!("null communicator does not support exec")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn test_none_upload_fails() {
        let c = NoneCommunicator;
        let result = c.upload(Path::new("/tmp/f"), "/dst").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not support"));
    }

    #[tokio::test]
    async fn test_none_download_fails() {
        let c = NoneCommunicator;
        let result = c.download("/src", Path::new("/tmp/f")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_none_exec_fails() {
        let c = NoneCommunicator;
        let result = c.exec("echo hi").await;
        assert!(result.is_err());
    }
}
