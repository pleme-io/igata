use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

use crate::traits::{Artifact, PostProcessor};

/// Shell-local post-processor: runs commands on the host after build completes.
///
/// Packer-compatible fields:
/// - `command`: single command string
/// - `inline`: array of commands
/// - `script` / `scripts`: script file paths
/// - `environment_vars`: array of "KEY=value" strings
/// - `env`: map of key/value env vars (takes precedence)
/// - `valid_exit_codes`: acceptable exit codes (default: [0])
///
/// Auto-injected env vars: PACKER_BUILD_NAME, PACKER_BUILDER_TYPE
pub struct ShellLocalPostProcessor;

#[async_trait::async_trait]
impl PostProcessor for ShellLocalPostProcessor {
    async fn process(
        &self,
        config: &HashMap<String, Value>,
        artifact: Artifact,
    ) -> Result<Artifact> {
        let mut commands: Vec<String> = Vec::new();

        if let Some(Value::Array(inline)) = config.get("inline") {
            for cmd in inline {
                if let Value::String(s) = cmd {
                    commands.push(s.clone());
                }
            }
        }

        if let Some(Value::String(command)) = config.get("command") {
            commands.push(command.clone());
        }

        if let Some(Value::String(script)) = config.get("script") {
            let content = std::fs::read_to_string(script)?;
            commands.push(content);
        }

        if let Some(Value::Array(scripts)) = config.get("scripts") {
            for script in scripts {
                if let Value::String(path) = script {
                    let content = std::fs::read_to_string(path)?;
                    commands.push(content);
                }
            }
        }

        // Environment variables: `env` map takes precedence over `environment_vars` array
        let mut env_vars: Vec<(String, String)> = Vec::new();
        if let Some(Value::Array(vars)) = config.get("environment_vars") {
            for var in vars {
                if let Value::String(s) = var {
                    if let Some((k, v)) = s.split_once('=') {
                        env_vars.push((k.to_string(), v.to_string()));
                    }
                }
            }
        }
        if let Some(Value::Object(map)) = config.get("env") {
            for (k, v) in map {
                if let Value::String(s) = v {
                    env_vars.push((k.clone(), s.clone()));
                }
            }
        }

        // Packer auto-injected env vars
        env_vars.push(("PACKER_BUILD_NAME".into(), artifact.builder_name.clone()));
        env_vars.push(("PACKER_BUILDER_TYPE".into(), artifact.builder_type.clone()));

        // Valid exit codes (Packer default: [0])
        let valid_exit_codes: Vec<i32> = config
            .get("valid_exit_codes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_i64().map(|n| n as i32))
                    .collect()
            })
            .unwrap_or_else(|| vec![0]);

        for cmd in &commands {
            let mut command = tokio::process::Command::new("sh");
            command.arg("-c").arg(cmd);
            for (k, v) in &env_vars {
                command.env(k, v);
            }

            let output = command.output().await?;
            let code = output.status.code().unwrap_or(-1);
            if !valid_exit_codes.contains(&code) {
                anyhow::bail!(
                    "shell-local post-processor failed (exit {code}): {}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        Ok(artifact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_artifact() -> Artifact {
        Artifact {
            id: "test".into(),
            description: "test".into(),
            files: vec![],
            builder_type: "null".into(),
            builder_name: "my-build".into(),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_pp_shell_local_inline() {
        let pp = ShellLocalPostProcessor;
        let config = HashMap::from([(
            "inline".to_string(),
            serde_json::json!(["echo hello"]),
        )]);
        let result = pp.process(&config, test_artifact()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pp_shell_local_auto_env_vars() {
        let pp = ShellLocalPostProcessor;
        let config = HashMap::from([(
            "inline".to_string(),
            serde_json::json!(["test \"$PACKER_BUILD_NAME\" = my-build && test \"$PACKER_BUILDER_TYPE\" = null"]),
        )]);
        let result = pp.process(&config, test_artifact()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pp_shell_local_valid_exit_codes() {
        let pp = ShellLocalPostProcessor;
        let config = HashMap::from([
            ("inline".to_string(), serde_json::json!(["exit 42"])),
            ("valid_exit_codes".to_string(), serde_json::json!([0, 42])),
        ]);
        let result = pp.process(&config, test_artifact()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pp_shell_local_invalid_exit_code() {
        let pp = ShellLocalPostProcessor;
        let config = HashMap::from([(
            "inline".to_string(),
            serde_json::json!(["exit 1"]),
        )]);
        let result = pp.process(&config, test_artifact()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pp_shell_local_env_map() {
        let pp = ShellLocalPostProcessor;
        let config = HashMap::from([
            ("inline".to_string(), serde_json::json!(["test \"$MY_VAR\" = hello"])),
            ("env".to_string(), serde_json::json!({"MY_VAR": "hello"})),
        ]);
        let result = pp.process(&config, test_artifact()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pp_shell_local_preserves_artifact() {
        let pp = ShellLocalPostProcessor;
        let config = HashMap::from([(
            "inline".to_string(),
            serde_json::json!(["echo pass"]),
        )]);
        let a = test_artifact();
        let result = pp.process(&config, a).await.unwrap();
        assert_eq!(result.builder_name, "my-build");
        assert_eq!(result.builder_type, "null");
    }
}
