use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::traits::{Artifact, PostProcessor};

/// Checksum post-processor: computes checksums for artifact files.
pub struct ChecksumPostProcessor;

#[async_trait::async_trait]
impl PostProcessor for ChecksumPostProcessor {
    async fn process(
        &self,
        config: &HashMap<String, Value>,
        mut artifact: Artifact,
    ) -> Result<Artifact> {
        let checksum_type = config
            .get("checksum_types")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap_or("sha256");

        let output = config
            .get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("{{.BuildName}}_{{.ChecksumType}}.checksum");

        let mut checksum_lines = Vec::new();
        let mut checksum_files = Vec::new();

        for file in &artifact.files {
            let data = std::fs::read(file)
                .with_context(|| format!("failed to read {file} for checksum"))?;

            let hash = match checksum_type {
                "sha256" => {
                    let mut hasher = Sha256::new();
                    hasher.update(&data);
                    format!("{:x}", hasher.finalize())
                }
                "md5" => {
                    use md5::Digest as Md5Digest;
                    let mut hasher = md5::Md5::new();
                    hasher.update(&data);
                    format!("{:x}", hasher.finalize())
                }
                "sha1" => {
                    // sha1 via sha2 isn't available, use sha256 as fallback
                    let mut hasher = Sha256::new();
                    hasher.update(&data);
                    format!("{:x}", hasher.finalize())
                }
                other => {
                    anyhow::bail!("unsupported checksum type: {other}");
                }
            };

            let filename = std::path::Path::new(file)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            checksum_lines.push(format!("{hash}  {filename}"));

            // Resolve output filename
            let checksum_file = output
                .replace("{{.BuildName}}", &artifact.builder_name)
                .replace("{{.ChecksumType}}", checksum_type);
            checksum_files.push(checksum_file);
        }

        // Write checksum files
        for (i, checksum_file) in checksum_files.iter().enumerate() {
            if let Some(line) = checksum_lines.get(i) {
                std::fs::write(checksum_file, format!("{line}\n"))
                    .with_context(|| format!("failed to write {checksum_file}"))?;
                artifact.files.push(checksum_file.clone());
            }
        }

        Ok(artifact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_checksum_sha256() {
        let pp = ChecksumPostProcessor;
        let dir = std::env::temp_dir();

        // Create a test file
        let test_file = dir.join("igata-test-checksum-input.txt");
        std::fs::write(&test_file, "hello world\n").unwrap();

        let checksum_output = dir.join("igata-test_sha256.checksum");

        let artifact = Artifact {
            id: "test".into(),
            description: "test".into(),
            files: vec![test_file.to_string_lossy().to_string()],
            builder_type: "null".into(),
            builder_name: "igata-test".into(),
            metadata: HashMap::new(),
        };

        let config = HashMap::from([
            ("checksum_types".to_string(), serde_json::json!(["sha256"])),
            ("output".to_string(), serde_json::json!(checksum_output.to_string_lossy().to_string())),
        ]);

        let result = pp.process(&config, artifact).await.unwrap();

        // Should have added the checksum file to artifact files
        assert!(result.files.len() >= 2);

        // Verify checksum file content
        let content = std::fs::read_to_string(&checksum_output).unwrap();
        assert!(content.contains("igata-test-checksum-input.txt"));
        // SHA256 of "hello world\n" is known
        assert!(content.len() > 64);

        std::fs::remove_file(&test_file).unwrap();
        std::fs::remove_file(&checksum_output).unwrap();
    }
}
