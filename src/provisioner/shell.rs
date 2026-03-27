use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;

use crate::traits::{Communicator, Provisioner};

/// Shell provisioner: runs shell commands on the machine.
///
/// Packer-compatible fields:
/// - `inline`: array of commands
/// - `script`: single script file path
/// - `scripts`: array of script file paths
/// - `environment_vars`: array of "KEY=value" strings
/// - `env`: map of key/value env vars (takes precedence over environment_vars)
/// - `execute_command`: template for running scripts (supports {{.Vars}}, {{.Path}})
/// - `inline_shebang`: shebang line for inline scripts (default: "/bin/sh -e")
/// - `valid_exit_codes`: array of acceptable exit codes (default: [0])
/// - `expect_disconnect`: don't error on SSH disconnect
/// - `pause_after`: wait after successful provisioning
pub struct ShellProvisioner;

#[async_trait::async_trait]
impl Provisioner for ShellProvisioner {
    async fn provision(
        &self,
        config: &HashMap<String, Value>,
        comm: Option<&dyn Communicator>,
    ) -> Result<()> {
        let comm = comm.context("shell provisioner requires a communicator")?;

        // Collect commands from "inline" or "script"/"scripts"
        let mut commands: Vec<String> = Vec::new();

        let inline_shebang = config
            .get("inline_shebang")
            .and_then(|v| v.as_str())
            .unwrap_or("/bin/sh -e");

        if let Some(Value::Array(inline)) = config.get("inline") {
            // Inline commands are joined into a single script with shebang
            let lines: Vec<String> = inline
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if !lines.is_empty() {
                let script = format!("#!{inline_shebang}\n{}", lines.join("\n"));
                commands.push(script);
            }
        }

        if let Some(Value::String(script)) = config.get("script") {
            let content = std::fs::read_to_string(script)
                .with_context(|| format!("failed to read script: {script}"))?;
            commands.push(content);
        }

        if let Some(Value::Array(scripts)) = config.get("scripts") {
            for script in scripts {
                if let Value::String(path) = script {
                    let content = std::fs::read_to_string(path)
                        .with_context(|| format!("failed to read script: {path}"))?;
                    commands.push(content);
                }
            }
        }

        // Environment variables: `env` map takes precedence over `environment_vars` array
        let mut env_vars: Vec<String> = Vec::new();
        if let Some(Value::Array(arr)) = config.get("environment_vars") {
            for v in arr {
                if let Value::String(s) = v {
                    env_vars.push(s.clone());
                }
            }
        }
        if let Some(Value::Object(map)) = config.get("env") {
            for (k, v) in map {
                if let Value::String(s) = v {
                    env_vars.push(format!("{k}={s}"));
                }
            }
        }

        let env_prefix = if env_vars.is_empty() {
            String::new()
        } else {
            format!(
                "export {} && ",
                env_vars.join(" && export ")
            )
        };

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

        let expect_disconnect = config
            .get("expect_disconnect")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        for cmd in &commands {
            let full_cmd = format!("{env_prefix}{cmd}");
            let output = comm.exec(&full_cmd).await;

            match output {
                Ok(out) => {
                    if !valid_exit_codes.contains(&out.exit_code) {
                        anyhow::bail!(
                            "shell command failed (exit {}): {}\n{}",
                            out.exit_code,
                            out.stdout,
                            out.stderr
                        );
                    }
                }
                Err(e) => {
                    if expect_disconnect {
                        // Packer: expect_disconnect means SSH disconnect is OK (e.g., reboot)
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        // Packer: pause_after
        if let Some(Value::String(pause)) = config.get("pause_after") {
            if let Some(dur) = crate::build::parse_duration(pause) {
                tokio::time::sleep(dur).await;
            }
        }

        Ok(())
    }
}
