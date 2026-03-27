use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

use crate::traits::{Artifact, Builder, Communicator};

/// A null builder that does nothing — useful for testing pipelines.
pub struct NullBuilder;

impl NullBuilder {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Builder for NullBuilder {
    fn prepare(&self, _config: &HashMap<String, Value>) -> Result<()> {
        Ok(())
    }

    async fn run(
        &self,
        _config: &HashMap<String, Value>,
    ) -> Result<Option<Box<dyn Communicator>>> {
        Ok(None)
    }

    async fn artifact(&self) -> Result<Artifact> {
        Ok(Artifact::empty("null", "null"))
    }

    async fn cleanup(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_null_builder_prepare() {
        let b = NullBuilder::new();
        assert!(b.prepare(&HashMap::new()).is_ok());
    }

    #[tokio::test]
    async fn test_null_builder_run_returns_none() {
        let b = NullBuilder::new();
        let comm = b.run(&HashMap::new()).await.unwrap();
        assert!(comm.is_none());
    }

    #[tokio::test]
    async fn test_null_builder_artifact() {
        let b = NullBuilder::new();
        let a = b.artifact().await.unwrap();
        assert_eq!(a.builder_type, "null");
        assert!(a.id.is_empty());
        assert!(a.files.is_empty());
    }

    #[tokio::test]
    async fn test_null_builder_cleanup() {
        let b = NullBuilder::new();
        assert!(b.cleanup().await.is_ok());
    }

    #[tokio::test]
    async fn test_null_builder_full_lifecycle() {
        let b = NullBuilder::new();
        let config = HashMap::new();
        b.prepare(&config).unwrap();
        let comm = b.run(&config).await.unwrap();
        assert!(comm.is_none());
        let artifact = b.artifact().await.unwrap();
        assert_eq!(artifact.builder_type, "null");
        b.cleanup().await.unwrap();
    }
}
