use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;

use crate::traits::{Artifact, PostProcessor};

/// Compress post-processor: compresses artifact files.
pub struct CompressPostProcessor;

#[async_trait::async_trait]
impl PostProcessor for CompressPostProcessor {
    async fn process(
        &self,
        config: &HashMap<String, Value>,
        mut artifact: Artifact,
    ) -> Result<Artifact> {
        let format = config
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("tar.gz");

        let output = config
            .get("output")
            .and_then(|v| v.as_str())
            .map(String::from);

        match format {
            "tar.gz" | "tgz" => {
                let output_path = output.unwrap_or_else(|| {
                    format!("{}.tar.gz", artifact.builder_name)
                });

                let file = std::fs::File::create(&output_path)
                    .with_context(|| format!("creating {output_path}"))?;
                let enc = GzEncoder::new(file, Compression::default());
                let mut tar = tar::Builder::new(enc);

                for src in &artifact.files {
                    let path = std::path::Path::new(src);
                    if path.exists() {
                        let name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        tar.append_path_with_name(path, &*name)
                            .with_context(|| format!("adding {src} to tar"))?;
                    }
                }

                tar.finish().context("finalizing tar")?;
                artifact.files = vec![output_path];
            }
            "zip" => {
                let output_path = output.unwrap_or_else(|| {
                    format!("{}.zip", artifact.builder_name)
                });

                let file = std::fs::File::create(&output_path)
                    .with_context(|| format!("creating {output_path}"))?;
                let mut zip = zip::ZipWriter::new(file);
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated);

                for src in &artifact.files {
                    let path = std::path::Path::new(src);
                    if path.exists() {
                        let name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let data = std::fs::read(path)
                            .with_context(|| format!("reading {src}"))?;
                        zip.start_file(&name, options)?;
                        zip.write_all(&data)?;
                    }
                }

                zip.finish().context("finalizing zip")?;
                artifact.files = vec![output_path];
            }
            "gz" | "gzip" => {
                for src in &artifact.files {
                    let output_path = output.clone().unwrap_or_else(|| {
                        format!("{src}.gz")
                    });

                    let data = std::fs::read(src)
                        .with_context(|| format!("reading {src}"))?;
                    let file = std::fs::File::create(&output_path)
                        .with_context(|| format!("creating {output_path}"))?;
                    let mut enc = GzEncoder::new(file, Compression::default());
                    enc.write_all(&data)?;
                    enc.finish().context("finalizing gzip")?;
                }
            }
            other => {
                anyhow::bail!("unsupported compression format: {other}");
            }
        }

        Ok(artifact)
    }
}
