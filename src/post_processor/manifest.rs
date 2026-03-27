use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;

use crate::traits::{Artifact, PostProcessor};

/// Manifest post-processor: writes build artifacts to a JSON manifest file.
pub struct ManifestPostProcessor;

#[async_trait::async_trait]
impl PostProcessor for ManifestPostProcessor {
    async fn process(
        &self,
        config: &HashMap<String, Value>,
        artifact: Artifact,
    ) -> Result<Artifact> {
        let output = config
            .get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("packer-manifest.json");

        let strip_path = config
            .get("strip_path")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Build manifest entry
        let mut entry = serde_json::Map::new();
        entry.insert("name".into(), Value::String(artifact.builder_name.clone()));
        entry.insert(
            "builder_type".into(),
            Value::String(artifact.builder_type.clone()),
        );
        entry.insert("artifact_id".into(), Value::String(artifact.id.clone()));

        let files: Vec<Value> = artifact
            .files
            .iter()
            .map(|f| {
                if strip_path {
                    Value::String(
                        std::path::Path::new(f)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    )
                } else {
                    Value::String(f.clone())
                }
            })
            .collect();
        entry.insert("files".into(), Value::Array(files));

        let timestamp = chrono::Utc::now().timestamp();
        entry.insert("build_time".into(), Value::Number(timestamp.into()));

        // Read existing manifest or create new
        let mut manifest: serde_json::Map<String, Value> =
            if let Ok(content) = std::fs::read_to_string(output) {
                serde_json::from_str(&content).unwrap_or_default()
            } else {
                serde_json::Map::new()
            };

        // Append to builds array
        let builds = manifest
            .entry("builds")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(arr) = builds {
            arr.push(Value::Object(entry));
        }

        manifest.insert(
            "last_run_uuid".into(),
            Value::String(uuid::Uuid::new_v4().to_string()),
        );

        let json = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(output, json)
            .with_context(|| format!("failed to write manifest to {output}"))?;

        Ok(artifact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_artifact() -> Artifact {
        Artifact {
            id: "ami-123".into(),
            description: "test".into(),
            files: vec!["/output/disk.qcow2".into()],
            builder_type: "qemu".into(),
            builder_name: "my-qemu".into(),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_manifest_writes_json() {
        let pp = ManifestPostProcessor;
        let dir = std::env::temp_dir();
        let output_path = dir.join("igata-test-manifest.json");
        let output_str = output_path.to_string_lossy().to_string();

        let config = HashMap::from([(
            "output".to_string(),
            serde_json::json!(output_str),
        )]);

        let result = pp.process(&config, test_artifact()).await.unwrap();
        assert_eq!(result.id, "ami-123"); // artifact passed through

        // Verify manifest file
        let content = std::fs::read_to_string(&output_path).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(manifest.get("builds").is_some());
        assert!(manifest.get("last_run_uuid").is_some());
        let builds = manifest["builds"].as_array().unwrap();
        assert_eq!(builds.len(), 1);
        assert_eq!(builds[0]["artifact_id"], "ami-123");
        assert_eq!(builds[0]["builder_type"], "qemu");

        std::fs::remove_file(&output_path).unwrap();
    }

    #[tokio::test]
    async fn test_manifest_strip_path() {
        let pp = ManifestPostProcessor;
        let dir = std::env::temp_dir();
        let output_path = dir.join("igata-test-manifest-strip.json");
        let output_str = output_path.to_string_lossy().to_string();

        let config = HashMap::from([
            ("output".to_string(), serde_json::json!(output_str)),
            ("strip_path".to_string(), serde_json::json!(true)),
        ]);

        pp.process(&config, test_artifact()).await.unwrap();

        let content = std::fs::read_to_string(&output_path).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();
        let files = manifest["builds"][0]["files"].as_array().unwrap();
        assert_eq!(files[0], "disk.qcow2"); // path stripped

        std::fs::remove_file(&output_path).unwrap();
    }

    #[tokio::test]
    async fn test_manifest_appends_builds() {
        let pp = ManifestPostProcessor;
        let dir = std::env::temp_dir();
        let output_path = dir.join("igata-test-manifest-append.json");
        let output_str = output_path.to_string_lossy().to_string();

        let config = HashMap::from([(
            "output".to_string(),
            serde_json::json!(output_str),
        )]);

        pp.process(&config, test_artifact()).await.unwrap();
        pp.process(&config, test_artifact()).await.unwrap();

        let content = std::fs::read_to_string(&output_path).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&content).unwrap();
        let builds = manifest["builds"].as_array().unwrap();
        assert_eq!(builds.len(), 2); // appended, not overwritten

        std::fs::remove_file(&output_path).unwrap();
    }
}
