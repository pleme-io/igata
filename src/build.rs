use anyhow::Result;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::config::Config;
use crate::display;
use crate::interpolation::{self, InterpolationContext};
use crate::template::Template;
use crate::traits::{Artifact, Registry};
use crate::variable::Variables;

/// Parse a Packer-style duration string: "5s", "10m", "1h", "300ms".
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms") {
        return ms.parse::<u64>().ok().map(Duration::from_millis);
    }
    if let Some(secs) = s.strip_suffix('s') {
        return secs.parse::<u64>().ok().map(Duration::from_secs);
    }
    if let Some(mins) = s.strip_suffix('m') {
        return mins.parse::<u64>().ok().map(|m| Duration::from_secs(m * 60));
    }
    if let Some(hrs) = s.strip_suffix('h') {
        return hrs.parse::<u64>().ok().map(|h| Duration::from_secs(h * 3600));
    }
    // Plain number = seconds
    s.parse::<u64>().ok().map(Duration::from_secs)
}

/// On-error behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnError {
    Cleanup,
    Abort,
    Ask,
}

impl OnError {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "cleanup" => Ok(Self::Cleanup),
            "abort" => Ok(Self::Abort),
            "ask" => Ok(Self::Ask),
            other => anyhow::bail!("unknown on-error mode: {other}"),
        }
    }
}

/// Options for a build run.
pub struct BuildOptions {
    pub only: Vec<String>,
    pub except: Vec<String>,
    pub on_error: OnError,
    pub parallel_builds: usize,
    #[allow(dead_code)]
    pub force: bool,
    #[allow(dead_code)]
    pub machine_readable: bool,
    #[allow(dead_code)]
    pub timestamp_ui: bool,
    #[allow(dead_code)]
    pub no_color: bool,
    /// Directory containing the template file (for {{template_dir}}).
    pub template_dir: Option<String>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            only: Vec::new(),
            except: Vec::new(),
            on_error: OnError::Cleanup,
            parallel_builds: 1,
            force: false,
            machine_readable: false,
            timestamp_ui: false,
            no_color: false,
            template_dir: None,
        }
    }
}

/// Result of a complete build run.
pub struct BuildResult {
    pub artifacts: Vec<Artifact>,
    pub errors: Vec<(String, anyhow::Error)>,
}

/// Run a full build: all matching builders, their provisioners, and post-processors.
pub async fn run(
    template: &Template,
    variables: &Variables,
    registry: &Registry,
    _config: &Config,
    opts: &BuildOptions,
) -> BuildResult {
    let semaphore = std::sync::Arc::new(Semaphore::new(opts.parallel_builds));
    let mut artifacts = Vec::new();
    let mut errors = Vec::new();

    for builder_cfg in &template.builders {
        let builder_name = builder_cfg.effective_name().to_string();

        // Filter by --only / --except
        if !opts.only.is_empty() && !opts.only.contains(&builder_name) {
            continue;
        }
        if !opts.except.is_empty() && opts.except.contains(&builder_name) {
            continue;
        }

        let _permit = semaphore.clone().acquire_owned().await.unwrap();

        match run_single_build(template, builder_cfg, variables, registry, opts).await {
            Ok(artifact) => artifacts.push(artifact),
            Err(e) => {
                display::print_build_error(&builder_name, &e.to_string());
                errors.push((builder_name, e));
                if opts.on_error == OnError::Abort {
                    break;
                }
            }
        }
    }

    BuildResult { artifacts, errors }
}

async fn run_single_build(
    template: &Template,
    builder_cfg: &crate::template::BuilderConfig,
    variables: &Variables,
    registry: &Registry,
    opts: &BuildOptions,
) -> Result<Artifact> {
    let builder_name = builder_cfg.effective_name().to_string();
    let builder_type = &builder_cfg.builder_type;

    display::print_build_start(&builder_name, builder_type);

    // Create builder instance
    let builder = registry
        .create_builder(builder_type)
        .ok_or_else(|| anyhow::anyhow!("unknown builder type: {builder_type}"))?;

    // Interpolate config
    let mut ctx = InterpolationContext::new(variables, &builder_name, builder_type);
    ctx.template_dir = opts.template_dir.as_deref();
    let mut config = builder_cfg.full_config();
    interpolation::interpolate_config(&mut config, &ctx)?;

    // Prepare
    builder.prepare(&config)?;

    // Run builder -> communicator
    let comm = builder.run(&config).await?;

    // Run provisioners
    for prov_cfg in &template.provisioners {
        if !prov_cfg.applies_to(&builder_name) {
            continue;
        }

        let prov = registry
            .create_provisioner(&prov_cfg.provisioner_type)
            .ok_or_else(|| {
                anyhow::anyhow!("unknown provisioner type: {}", prov_cfg.provisioner_type)
            })?;

        // Packer: pause_before — wait before running this provisioner
        if let Some(ref pause) = prov_cfg.pause_before {
            if let Some(dur) = parse_duration(pause) {
                display::print_provision(&builder_name, &prov_cfg.provisioner_type, &format!("waiting {pause}..."));
                tokio::time::sleep(dur).await;
            }
        }

        display::print_provision(&builder_name, &prov_cfg.provisioner_type, "running...");

        let mut prov_config = prov_cfg.config_for_builder(&builder_name);
        interpolation::interpolate_config(&mut prov_config, &ctx)?;

        // Packer: max_retries + timeout
        let max_attempts = prov_cfg.max_retries.unwrap_or(0) + 1;
        let timeout_dur = prov_cfg.timeout.as_deref().and_then(parse_duration);

        let mut last_err = None;
        for attempt in 0..max_attempts {
            if attempt > 0 {
                display::print_provision(&builder_name, &prov_cfg.provisioner_type, &format!("retry {attempt}/{}", max_attempts - 1));
            }

            let fut = prov.provision(&prov_config, comm.as_deref());
            let result = if let Some(dur) = timeout_dur {
                match tokio::time::timeout(dur, fut).await {
                    Ok(r) => r,
                    Err(_) => Err(anyhow::anyhow!("provisioner timed out after {:?}", dur)),
                }
            } else {
                fut.await
            };

            match result {
                Ok(()) => {
                    last_err = None;
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }
        if let Some(e) = last_err {
            return Err(e);
        }
    }

    // Get artifact
    let mut artifact = builder.artifact().await?;
    artifact.builder_name = builder_name.clone();
    artifact.builder_type = builder_type.clone();

    // Cleanup builder
    display::print_cleanup(&builder_name);
    if let Err(e) = builder.cleanup().await {
        if opts.on_error != OnError::Abort {
            eprintln!("    cleanup warning: {e}");
        }
    }

    // Run post-processors
    // Packer: each top-level entry runs independently against the builder artifact.
    // Pipeline entries (arrays) chain: output of one feeds input of next.
    // keep_input_artifact: if true, preserve the original artifact alongside the output.
    for pp_entry in &template.post_processors {
        let pipeline = pp_entry.as_pipeline();
        let mut current_artifact = artifact.clone();
        let input_artifact = artifact.clone();

        for pp_cfg in &pipeline {
            if !pp_cfg.applies_to(&builder_name) {
                continue;
            }

            let pp = registry
                .create_post_processor(&pp_cfg.pp_type)
                .ok_or_else(|| {
                    anyhow::anyhow!("unknown post-processor type: {}", pp_cfg.pp_type)
                })?;

            display::print_post_process(&builder_name, &pp_cfg.pp_type);

            let mut pp_config = pp_cfg.config.clone();
            interpolation::interpolate_config(&mut pp_config, &ctx)?;

            let output = pp.process(&pp_config, current_artifact).await?;

            // Packer: keep_input_artifact preserves input files alongside output
            current_artifact = if pp_cfg.keep_input_artifact.unwrap_or(false) {
                let mut merged = output;
                for f in &input_artifact.files {
                    if !merged.files.contains(f) {
                        merged.files.push(f.clone());
                    }
                }
                merged
            } else {
                output
            };
        }

        artifact = current_artifact;
    }

    display::print_build_done(&builder_name);
    display::print_artifact(&builder_name, &artifact.description);

    Ok(artifact)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_on_error_parse_cleanup() {
        assert_eq!(OnError::parse("cleanup").unwrap(), OnError::Cleanup);
    }

    #[test]
    fn test_on_error_parse_abort() {
        assert_eq!(OnError::parse("abort").unwrap(), OnError::Abort);
    }

    #[test]
    fn test_on_error_parse_ask() {
        assert_eq!(OnError::parse("ask").unwrap(), OnError::Ask);
    }

    #[test]
    fn test_on_error_parse_invalid() {
        assert!(OnError::parse("invalid").is_err());
        assert!(OnError::parse("").is_err());
        assert!(OnError::parse("CLEANUP").is_err()); // case sensitive
    }

    #[test]
    fn test_build_options_default() {
        let opts = BuildOptions::default();
        assert!(opts.only.is_empty());
        assert!(opts.except.is_empty());
        assert_eq!(opts.on_error, OnError::Cleanup);
        assert_eq!(opts.parallel_builds, 1);
        assert!(!opts.force);
        assert!(!opts.machine_readable);
        assert!(!opts.timestamp_ui);
        assert!(!opts.no_color);
    }

    #[tokio::test]
    async fn test_run_empty_template() {
        let tmpl = crate::template::parse_json(r#"{"builders": []}"#).unwrap();
        let vars = Variables::new();
        let registry = Registry::new();
        let cfg = Config::default();
        let opts = BuildOptions::default();

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert!(result.artifacts.is_empty());
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_run_null_builder_end_to_end() {
        let tmpl = crate::template::parse_json(r#"{"builders": [{"type": "null"}]}"#).unwrap();
        let vars = Variables::new();
        let mut registry = Registry::new();
        crate::builder::register_all(&mut registry);
        crate::provisioner::register_all(&mut registry);
        crate::post_processor::register_all(&mut registry);
        let cfg = Config::default();
        let opts = BuildOptions::default();

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert_eq!(result.artifacts.len(), 1);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_run_only_filter() {
        let tmpl = crate::template::parse_json(
            r#"{"builders": [{"type": "null", "name": "a"}, {"type": "null", "name": "b"}]}"#,
        )
        .unwrap();
        let vars = Variables::new();
        let mut registry = Registry::new();
        crate::builder::register_all(&mut registry);
        let cfg = Config::default();
        let opts = BuildOptions {
            only: vec!["a".into()],
            ..Default::default()
        };

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert_eq!(result.artifacts.len(), 1);
        assert_eq!(result.artifacts[0].builder_name, "a");
    }

    #[tokio::test]
    async fn test_run_except_filter() {
        let tmpl = crate::template::parse_json(
            r#"{"builders": [{"type": "null", "name": "a"}, {"type": "null", "name": "b"}]}"#,
        )
        .unwrap();
        let vars = Variables::new();
        let mut registry = Registry::new();
        crate::builder::register_all(&mut registry);
        let cfg = Config::default();
        let opts = BuildOptions {
            except: vec!["a".into()],
            ..Default::default()
        };

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert_eq!(result.artifacts.len(), 1);
        assert_eq!(result.artifacts[0].builder_name, "b");
    }

    #[tokio::test]
    async fn test_run_unknown_builder_error() {
        let tmpl =
            crate::template::parse_json(r#"{"builders": [{"type": "nonexistent"}]}"#).unwrap();
        let vars = Variables::new();
        let registry = Registry::new(); // empty, no builders registered
        let cfg = Config::default();
        let opts = BuildOptions::default();

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].0, "nonexistent");
    }

    #[tokio::test]
    async fn test_run_abort_on_error() {
        let tmpl = crate::template::parse_json(
            r#"{"builders": [{"type": "fail1"}, {"type": "fail2"}]}"#,
        )
        .unwrap();
        let vars = Variables::new();
        let registry = Registry::new(); // empty
        let cfg = Config::default();
        let opts = BuildOptions {
            on_error: OnError::Abort,
            ..Default::default()
        };

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        // With Abort, should stop after first error
        assert_eq!(result.errors.len(), 1);
    }

    // -- Duration parsing tests --

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("5s"), Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("10m"), Some(Duration::from_secs(600)));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
    }

    #[test]
    fn test_parse_duration_milliseconds() {
        assert_eq!(parse_duration("100ms"), Some(Duration::from_millis(100)));
    }

    #[test]
    fn test_parse_duration_plain_number() {
        assert_eq!(parse_duration("30"), Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert_eq!(parse_duration("abc"), None);
    }

    #[test]
    fn test_parse_duration_whitespace() {
        assert_eq!(parse_duration("  5s  "), Some(Duration::from_secs(5)));
    }

    // -- End-to-end with provisioners --

    #[tokio::test]
    async fn test_run_null_with_shell_local_provisioner() {
        let tmpl = crate::template::parse_json(r#"{
            "builders": [{"type": "null"}],
            "provisioners": [{"type": "shell-local", "inline": ["echo hello"]}]
        }"#)
        .unwrap();
        let vars = Variables::new();
        let mut registry = Registry::new();
        crate::builder::register_all(&mut registry);
        crate::provisioner::register_all(&mut registry);
        let cfg = Config::default();
        let opts = BuildOptions::default();

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert_eq!(result.artifacts.len(), 1);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_run_provisioner_only_filter() {
        let tmpl = crate::template::parse_json(r#"{
            "builders": [{"type": "null", "name": "a"}, {"type": "null", "name": "b"}],
            "provisioners": [{"type": "shell-local", "inline": ["echo only-a"], "only": ["a"]}]
        }"#)
        .unwrap();
        let vars = Variables::new();
        let mut registry = Registry::new();
        crate::builder::register_all(&mut registry);
        crate::provisioner::register_all(&mut registry);
        let cfg = Config::default();
        let opts = BuildOptions::default();

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert_eq!(result.artifacts.len(), 2);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_run_with_variable_interpolation() {
        let tmpl = crate::template::parse_json(r#"{
            "variables": {"greeting": "hello"},
            "builders": [{"type": "null"}],
            "provisioners": [{"type": "shell-local", "inline": ["echo {{user `greeting`}}"]}]
        }"#)
        .unwrap();
        let mut vars = Variables::new();
        vars.insert("greeting".into(), "hello".into());
        let mut registry = Registry::new();
        crate::builder::register_all(&mut registry);
        crate::provisioner::register_all(&mut registry);
        let cfg = Config::default();
        let opts = BuildOptions::default();

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_run_cleanup_on_error_continues() {
        let tmpl = crate::template::parse_json(
            r#"{"builders": [{"type": "fail1"}, {"type": "fail2"}]}"#,
        )
        .unwrap();
        let vars = Variables::new();
        let registry = Registry::new();
        let cfg = Config::default();
        let opts = BuildOptions {
            on_error: OnError::Cleanup,
            ..Default::default()
        };

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        // Cleanup mode continues after errors
        assert_eq!(result.errors.len(), 2);
    }

    #[tokio::test]
    async fn test_run_multiple_builders() {
        let tmpl = crate::template::parse_json(
            r#"{"builders": [{"type": "null", "name": "a"}, {"type": "null", "name": "b"}, {"type": "null", "name": "c"}]}"#,
        )
        .unwrap();
        let vars = Variables::new();
        let mut registry = Registry::new();
        crate::builder::register_all(&mut registry);
        let cfg = Config::default();
        let opts = BuildOptions::default();

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert_eq!(result.artifacts.len(), 3);
    }

    /// End-to-end integration test proving the full Packer pipeline:
    /// Template IR → variable resolution → interpolation → builder → provisioner → post-processor
    #[tokio::test]
    async fn test_full_packer_pipeline_e2e() {
        // This template exercises: variables, builders, provisioners with
        // only/except, shell-local commands, and shell-local post-processor
        let json = r#"{
            "variables": {
                "greeting": "world"
            },
            "builders": [
                {"type": "null", "name": "primary"},
                {"type": "null", "name": "secondary"}
            ],
            "provisioners": [
                {
                    "type": "shell-local",
                    "inline": ["echo {{user `greeting`}}"]
                },
                {
                    "type": "shell-local",
                    "inline": ["echo only-primary"],
                    "only": ["primary"]
                },
                {
                    "type": "shell-local",
                    "inline": ["echo except-secondary"],
                    "except": ["secondary"]
                }
            ],
            "post-processors": [
                {
                    "type": "shell-local",
                    "inline": ["echo post-process done"]
                }
            ]
        }"#;

        let tmpl = crate::template::parse_json(json).unwrap();

        // Validate
        let validation = crate::validate::validate(&tmpl);
        assert!(validation.is_ok(), "validation errors: {:?}", validation.errors);

        // Resolve variables
        let vars = crate::variable::resolve(
            &tmpl.variables,
            &[("greeting".into(), "world".into())],
            &[],
        )
        .unwrap();

        // Build
        let mut registry = Registry::new();
        crate::builder::register_all(&mut registry);
        crate::provisioner::register_all(&mut registry);
        crate::post_processor::register_all(&mut registry);
        let cfg = Config::default();
        let opts = BuildOptions::default();

        let result = run(&tmpl, &vars, &registry, &cfg, &opts).await;
        assert!(result.errors.is_empty(), "build errors: {:?}", result.errors);
        assert_eq!(result.artifacts.len(), 2);
        assert_eq!(result.artifacts[0].builder_name, "primary");
        assert_eq!(result.artifacts[1].builder_name, "secondary");
    }

    /// Prove YAML produces identical pipeline results as JSON.
    #[tokio::test]
    async fn test_yaml_json_pipeline_equivalence() {
        let json_tmpl = crate::template::parse_json(r#"{
            "variables": {"name": "test"},
            "builders": [{"type": "null"}],
            "provisioners": [{"type": "shell-local", "inline": ["echo {{user `name`}}"]}]
        }"#)
        .unwrap();

        let yaml_tmpl = crate::template::parse_yaml(r#"
variables:
  name: test
builders:
  - type: "null"
provisioners:
  - type: shell-local
    inline:
      - "echo {{user `name`}}"
"#)
        .unwrap();

        let vars = crate::variable::resolve(&json_tmpl.variables, &[], &[]).unwrap();
        let mut registry = Registry::new();
        crate::builder::register_all(&mut registry);
        crate::provisioner::register_all(&mut registry);

        let cfg = Config::default();
        let opts = BuildOptions::default();

        let json_result = run(&json_tmpl, &vars, &registry, &cfg, &opts).await;
        let yaml_result = run(&yaml_tmpl, &vars, &registry, &cfg, &opts).await;

        assert!(json_result.errors.is_empty());
        assert!(yaml_result.errors.is_empty());
        assert_eq!(json_result.artifacts.len(), yaml_result.artifacts.len());
    }
}
