use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

use crate::traits::{Communicator, Provisioner};

/// Breakpoint provisioner: pauses the build for debugging.
pub struct BreakpointProvisioner;

#[async_trait::async_trait]
impl Provisioner for BreakpointProvisioner {
    async fn provision(
        &self,
        config: &HashMap<String, Value>,
        _comm: Option<&dyn Communicator>,
    ) -> Result<()> {
        let disable = config
            .get("disable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if disable {
            return Ok(());
        }

        let note = config
            .get("note")
            .and_then(|v| v.as_str())
            .unwrap_or("Pausing build");

        eprintln!("==> breakpoint: {note}");
        eprintln!("==> breakpoint: Press enter to continue.");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_breakpoint_disabled_skips() {
        let prov = BreakpointProvisioner;
        let config = HashMap::from([(
            "disable".to_string(),
            serde_json::json!(true),
        )]);
        // When disabled, should return immediately without blocking on stdin
        let result = prov.provision(&config, None).await;
        assert!(result.is_ok());
    }
}
