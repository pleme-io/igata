use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

use crate::traits::{Communicator, Provisioner};

/// Shell-local provisioner: runs commands on the host machine (not remote).
///
/// Packer-compatible fields:
/// - `command`: single command string
/// - `inline`: array of commands
/// - `script` / `scripts`: script file paths
/// - `environment_vars`: array of "KEY=value" strings
/// - `env`: map of key/value env vars (takes precedence)
/// - `valid_exit_codes`: acceptable exit codes (default: [0])
/// - `only_on`: OS filter (e.g., ["linux", "darwin"])
pub struct ShellLocalProvisioner;

#[async_trait::async_trait]
impl Provisioner for ShellLocalProvisioner {
    async fn provision(
        &self,
        config: &HashMap<String, Value>,
        _comm: Option<&dyn Communicator>,
    ) -> Result<()> {
        // Packer: only_on OS filter
        if let Some(Value::Array(only_on)) = config.get("only_on") {
            let current_os = std::env::consts::OS;
            let allowed: Vec<&str> = only_on
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            if !allowed.is_empty() && !allowed.contains(&current_os) {
                return Ok(()); // Skip on non-matching OS
            }
        }

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
                    "shell-local command failed (exit {code}): {}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Provisioner;

    #[tokio::test]
    async fn test_shell_local_inline() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::from([(
            "inline".to_string(),
            serde_json::json!(["echo hello"]),
        )]);
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shell_local_command() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::from([(
            "command".to_string(),
            serde_json::json!("echo hello"),
        )]);
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shell_local_valid_exit_codes() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::from([
            ("inline".to_string(), serde_json::json!(["exit 42"])),
            ("valid_exit_codes".to_string(), serde_json::json!([0, 42])),
        ]);
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shell_local_invalid_exit_code() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::from([(
            "inline".to_string(),
            serde_json::json!(["exit 1"]),
        )]);
        let result = prov.provision(&config, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shell_local_env_vars_array() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::from([
            ("inline".to_string(), serde_json::json!(["test \"$MY_VAR\" = hello"])),
            ("environment_vars".to_string(), serde_json::json!(["MY_VAR=hello"])),
        ]);
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shell_local_env_map() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::from([
            ("inline".to_string(), serde_json::json!(["test \"$FOO\" = bar"])),
            ("env".to_string(), serde_json::json!({"FOO": "bar"})),
        ]);
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shell_local_only_on_current_os() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::from([
            ("inline".to_string(), serde_json::json!(["echo hello"])),
            ("only_on".to_string(), serde_json::json!([std::env::consts::OS])),
        ]);
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shell_local_only_on_other_os_skips() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::from([
            // This command would fail, but it should be skipped because only_on doesn't match
            ("inline".to_string(), serde_json::json!(["exit 1"])),
            ("only_on".to_string(), serde_json::json!(["nonexistent-os"])),
        ]);
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok()); // Skipped, so no error
    }

    #[tokio::test]
    async fn test_shell_local_multiple_commands() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::from([(
            "inline".to_string(),
            serde_json::json!(["echo one", "echo two", "echo three"]),
        )]);
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shell_local_empty_commands() {
        let prov = ShellLocalProvisioner;
        let config = HashMap::new();
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok()); // No commands = no-op
    }
}
