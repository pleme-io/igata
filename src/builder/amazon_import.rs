use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::builder::{get_str, get_u64};
use crate::traits::{Artifact, Builder, Communicator};

/// Amazon Import builder: uploads a pre-built Nix disk image to S3, imports
/// it as an AMI via the `vmimport` service role, tags it, and updates an SSM
/// parameter with the resulting AMI ID.
///
/// Unlike [`amazon_ebs`](super::amazon_ebs), this builder never launches an
/// EC2 instance and never provisions over SSH — the disk image handed to it
/// is already fully configured (a NixOS `nixos-generators` output, typically).
/// There is nothing to run inside a machine, so [`run`](Builder::run) returns
/// `Ok(None)` for the communicator, same as [`NullBuilder`](super::null::NullBuilder).
///
/// The upload/import/tag/SSM-update sequence is not re-implemented here — it
/// is the exact, already-proven `ami-forge build` pipeline
/// (`ami_forge::build::run`), called in-process. This builder is the glue
/// that lets `igata build <template>` be the single front door for that
/// pipeline, so no separate manual `ami-forge build` invocation is needed
/// afterward.
pub struct AmazonImportBuilder {
    state: Arc<Mutex<ImportState>>,
}

#[derive(Default)]
struct ImportState {
    ami_id: Option<String>,
    ami_name: String,
    region: String,
}

impl AmazonImportBuilder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ImportState::default())),
        }
    }
}

/// Read the builder config into an `ami_forge::build::BuildArgs`.
///
/// Pure — no I/O beyond checking the image path exists, no AWS calls. Shared
/// by `prepare()` (which discards the value, keeping only the validation
/// side effect) and `run()` (which consumes it), so the two can never drift.
fn build_args_from_config(config: &HashMap<String, Value>) -> Result<ami_forge::build::BuildArgs> {
    let image = get_str(config, "image").context(
        "amazon-import builder requires 'image' (path to a pre-built Nix disk image, \
         or the directory containing it)",
    )?;
    if !Path::new(&image).exists() {
        anyhow::bail!("amazon-import builder: image path '{image}' does not exist");
    }

    let bucket = get_str(config, "bucket")
        .context("amazon-import builder requires 'bucket' (S3 transit bucket)")?;
    let ami_name = get_str(config, "ami_name")
        .context("amazon-import builder requires 'ami_name' (constant AMI name)")?;
    let ssm = get_str(config, "ssm").context(
        "amazon-import builder requires 'ssm' (SSM parameter path to update with the AMI ID)",
    )?;
    let region = get_str(config, "region").unwrap_or_else(|| "us-east-1".into());
    let role_name = get_str(config, "role_name");
    let timeout = get_u64(config, "timeout").unwrap_or(1800);

    Ok(ami_forge::build::BuildArgs {
        image: PathBuf::from(image),
        bucket,
        ami_name,
        ssm,
        region,
        role_name,
        timeout,
    })
}

#[async_trait::async_trait]
impl Builder for AmazonImportBuilder {
    fn prepare(&self, config: &HashMap<String, Value>) -> Result<()> {
        build_args_from_config(config)?;
        Ok(())
    }

    async fn run(
        &self,
        config: &HashMap<String, Value>,
    ) -> Result<Option<Box<dyn Communicator>>> {
        let args = build_args_from_config(config)?;

        let mut state = self.state.lock().await;
        state.ami_name = args.ami_name.clone();
        state.region = args.region.clone();
        drop(state);

        let ami_id = ami_forge::build::run(args)
            .await
            .context("amazon-import: ami-forge build pipeline failed")?;

        self.state.lock().await.ami_id = Some(ami_id);

        // No running instance to provision — the image was already fully
        // configured before it reached this builder.
        Ok(None)
    }

    async fn artifact(&self) -> Result<Artifact> {
        let state = self.state.lock().await;
        let ami_id = state.ami_id.as_ref().context(
            "amazon-import: no AMI produced yet — run() must complete successfully before artifact()",
        )?;

        Ok(Artifact {
            id: ami_id.clone(),
            description: format!("AMI: {ami_id} ({})", state.ami_name),
            files: Vec::new(),
            builder_type: "amazon-import".into(),
            builder_name: String::new(),
            metadata: HashMap::from([
                ("ami_id".into(), ami_id.clone()),
                ("ami_name".into(), state.ami_name.clone()),
                ("region".into(), state.region.clone()),
            ]),
        })
    }

    async fn cleanup(&self) -> Result<()> {
        // ami_forge::build::run() already deletes the transit S3 object as
        // the final step of its own pipeline (upload -> rotate old AMI ->
        // import -> poll -> tag -> update SSM -> delete transit object).
        // There is no local process/instance state to tear down here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_config(image: &str) -> HashMap<String, Value> {
        HashMap::from([
            ("image".into(), Value::String(image.into())),
            ("bucket".into(), Value::String("my-transit-bucket".into())),
            ("ami_name".into(), Value::String("igata-portao".into())),
            (
                "ssm".into(),
                Value::String("/pleme/camelot/portao/ami-id".into()),
            ),
        ])
    }

    #[test]
    fn test_prepare_requires_image() {
        let b = AmazonImportBuilder::new();
        let mut config = full_config("/nonexistent");
        config.remove("image");
        let err = b.prepare(&config).unwrap_err();
        assert!(err.to_string().contains("'image'"));
    }

    #[test]
    fn test_prepare_rejects_missing_image_path() {
        let b = AmazonImportBuilder::new();
        let config = full_config("/definitely/does/not/exist/on/this/machine.raw");
        let err = b.prepare(&config).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn test_prepare_requires_bucket() {
        let b = AmazonImportBuilder::new();
        let tmp = std::env::temp_dir();
        let mut config = full_config(tmp.to_str().unwrap());
        config.remove("bucket");
        let err = b.prepare(&config).unwrap_err();
        assert!(err.to_string().contains("'bucket'"));
    }

    #[test]
    fn test_prepare_requires_ami_name() {
        let b = AmazonImportBuilder::new();
        let tmp = std::env::temp_dir();
        let mut config = full_config(tmp.to_str().unwrap());
        config.remove("ami_name");
        let err = b.prepare(&config).unwrap_err();
        assert!(err.to_string().contains("'ami_name'"));
    }

    #[test]
    fn test_prepare_requires_ssm() {
        let b = AmazonImportBuilder::new();
        let tmp = std::env::temp_dir();
        let mut config = full_config(tmp.to_str().unwrap());
        config.remove("ssm");
        let err = b.prepare(&config).unwrap_err();
        assert!(err.to_string().contains("'ssm'"));
    }

    #[test]
    fn test_prepare_accepts_valid_config_with_existing_path() {
        let b = AmazonImportBuilder::new();
        let tmp = std::env::temp_dir();
        let config = full_config(tmp.to_str().unwrap());
        assert!(b.prepare(&config).is_ok());
    }

    #[test]
    fn test_build_args_from_config_defaults_region_and_timeout() {
        let tmp = std::env::temp_dir();
        let config = full_config(tmp.to_str().unwrap());
        let args = build_args_from_config(&config).unwrap();
        assert_eq!(args.region, "us-east-1");
        assert_eq!(args.timeout, 1800);
        assert!(args.role_name.is_none());
        assert_eq!(args.bucket, "my-transit-bucket");
        assert_eq!(args.ami_name, "igata-portao");
        assert_eq!(args.ssm, "/pleme/camelot/portao/ami-id");
    }

    #[test]
    fn test_build_args_from_config_honors_overrides() {
        let tmp = std::env::temp_dir();
        let mut config = full_config(tmp.to_str().unwrap());
        config.insert("region".into(), Value::String("us-west-2".into()));
        config.insert("role_name".into(), Value::String("custom-vmimport".into()));
        config.insert("timeout".into(), Value::from(600u64));
        let args = build_args_from_config(&config).unwrap();
        assert_eq!(args.region, "us-west-2");
        assert_eq!(args.role_name, Some("custom-vmimport".into()));
        assert_eq!(args.timeout, 600);
    }

    #[tokio::test]
    async fn test_artifact_before_run_errors() {
        let b = AmazonImportBuilder::new();
        let err = b.artifact().await.unwrap_err();
        assert!(err.to_string().contains("no AMI produced yet"));
    }

    #[tokio::test]
    async fn test_cleanup_is_a_noop() {
        let b = AmazonImportBuilder::new();
        assert!(b.cleanup().await.is_ok());
    }
}
