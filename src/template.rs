use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Core IR — the canonical data structures that feed everything
// ---------------------------------------------------------------------------

/// A complete build specification — the canonical internal representation.
///
/// This is the core type that drives the entire build pipeline.
/// Both Packer JSON templates and shikumi YAML configs deserialize into this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    #[serde(default)]
    pub description: Option<String>,

    /// Minimum igata/packer version required.
    #[serde(default, alias = "min_packer_version")]
    pub min_version: Option<String>,

    /// Sensitive variables (values masked in output).
    #[serde(default, alias = "sensitive-variables")]
    pub sensitive_variables: Vec<String>,

    /// Variables: key → default value. `null` means required.
    #[serde(default)]
    pub variables: HashMap<String, Value>,

    /// Builder definitions.
    #[serde(default)]
    pub builders: Vec<BuilderConfig>,

    /// Provisioner steps (run in order).
    #[serde(default)]
    pub provisioners: Vec<ProvisionerConfig>,

    /// Post-processor entries (single or pipeline).
    #[serde(default, alias = "post-processors")]
    pub post_processors: Vec<PostProcessorEntry>,
}

// ---------------------------------------------------------------------------
// BuilderConfig — machine lifecycle specification
// ---------------------------------------------------------------------------

/// A builder definition: what kind of machine to build and how.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderConfig {
    /// Builder type: "null", "docker", "qemu", "amazon-ebs".
    #[serde(rename = "type")]
    pub builder_type: String,

    /// Optional human name (defaults to builder_type).
    #[serde(default)]
    pub name: Option<String>,

    // -- SSH Communicator fields (Packer-compatible) --

    #[serde(default)]
    pub communicator: Option<String>,

    #[serde(default)]
    pub ssh_host: Option<String>,
    #[serde(default, alias = "ssh_port")]
    pub ssh_port: Option<u16>,
    #[serde(default)]
    pub ssh_username: Option<String>,
    #[serde(default)]
    pub ssh_password: Option<String>,
    #[serde(default)]
    pub ssh_private_key_file: Option<String>,
    #[serde(default)]
    pub ssh_keypair_name: Option<String>,
    #[serde(default)]
    pub ssh_agent_auth: Option<bool>,
    #[serde(default)]
    pub ssh_timeout: Option<String>,
    #[serde(default)]
    pub ssh_handshake_attempts: Option<u32>,
    #[serde(default)]
    pub ssh_disable_agent_forwarding: Option<bool>,
    #[serde(default)]
    pub ssh_bastion_host: Option<String>,
    #[serde(default)]
    pub ssh_bastion_port: Option<u16>,
    #[serde(default)]
    pub ssh_bastion_username: Option<String>,
    #[serde(default)]
    pub ssh_bastion_password: Option<String>,
    #[serde(default)]
    pub ssh_bastion_private_key_file: Option<String>,
    #[serde(default)]
    pub ssh_bastion_agent_auth: Option<bool>,
    #[serde(default)]
    pub ssh_file_transfer_method: Option<String>,
    #[serde(default)]
    pub ssh_proxy_host: Option<String>,
    #[serde(default)]
    pub ssh_proxy_port: Option<u16>,
    #[serde(default)]
    pub ssh_proxy_username: Option<String>,
    #[serde(default)]
    pub ssh_proxy_password: Option<String>,
    #[serde(default)]
    pub ssh_keep_alive_interval: Option<String>,
    #[serde(default)]
    pub ssh_read_write_timeout: Option<String>,
    #[serde(default)]
    pub ssh_pty: Option<bool>,
    #[serde(default)]
    pub ssh_certificate_file: Option<String>,
    #[serde(default)]
    pub ssh_clear_authorized_keys: Option<bool>,
    #[serde(default)]
    pub ssh_ciphers: Option<Vec<String>>,
    #[serde(default)]
    pub ssh_key_exchange_algorithms: Option<Vec<String>>,
    #[serde(default)]
    pub ssh_remote_tunnels: Option<Vec<String>>,
    #[serde(default)]
    pub ssh_local_tunnels: Option<Vec<String>>,
    #[serde(default)]
    pub ssh_bastion_certificate_file: Option<String>,
    #[serde(default)]
    pub ssh_bastion_interactive: Option<bool>,
    #[serde(default)]
    pub temporary_key_pair_name: Option<String>,
    #[serde(default)]
    pub temporary_key_pair_type: Option<String>,
    #[serde(default)]
    pub temporary_key_pair_bits: Option<u32>,
    #[serde(default)]
    pub pause_before_connecting: Option<String>,

    // -- WinRM Communicator fields (Packer-compatible) --

    #[serde(default)]
    pub winrm_host: Option<String>,
    #[serde(default)]
    pub winrm_port: Option<u16>,
    #[serde(default)]
    pub winrm_username: Option<String>,
    #[serde(default)]
    pub winrm_password: Option<String>,
    #[serde(default)]
    pub winrm_timeout: Option<String>,
    #[serde(default)]
    pub winrm_use_ssl: Option<bool>,
    #[serde(default)]
    pub winrm_insecure: Option<bool>,
    #[serde(default)]
    pub winrm_use_ntlm: Option<bool>,

    /// All remaining builder-specific fields (flattened from JSON).
    #[serde(flatten)]
    pub config: HashMap<String, Value>,
}

impl BuilderConfig {
    /// The effective name: explicit name or builder type.
    pub fn effective_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.builder_type)
    }

    /// Merge communicator fields into the flat config map for builder consumption.
    pub fn full_config(&self) -> HashMap<String, Value> {
        let mut cfg = self.config.clone();
        macro_rules! insert_opt {
            ($field:ident) => {
                if let Some(ref v) = self.$field {
                    cfg.entry(stringify!($field).to_string())
                        .or_insert_with(|| serde_json::to_value(v).unwrap());
                }
            };
        }
        insert_opt!(communicator);
        insert_opt!(ssh_host);
        insert_opt!(ssh_port);
        insert_opt!(ssh_username);
        insert_opt!(ssh_password);
        insert_opt!(ssh_private_key_file);
        insert_opt!(ssh_keypair_name);
        insert_opt!(ssh_agent_auth);
        insert_opt!(ssh_timeout);
        insert_opt!(ssh_handshake_attempts);
        insert_opt!(ssh_bastion_host);
        insert_opt!(ssh_bastion_port);
        insert_opt!(ssh_bastion_username);
        insert_opt!(ssh_bastion_password);
        insert_opt!(ssh_bastion_private_key_file);
        insert_opt!(ssh_file_transfer_method);
        insert_opt!(ssh_pty);
        insert_opt!(ssh_certificate_file);
        insert_opt!(ssh_clear_authorized_keys);
        insert_opt!(ssh_ciphers);
        insert_opt!(ssh_key_exchange_algorithms);
        insert_opt!(ssh_remote_tunnels);
        insert_opt!(ssh_local_tunnels);
        insert_opt!(ssh_bastion_certificate_file);
        insert_opt!(ssh_bastion_interactive);
        insert_opt!(ssh_bastion_agent_auth);
        insert_opt!(ssh_disable_agent_forwarding);
        insert_opt!(ssh_keep_alive_interval);
        insert_opt!(ssh_read_write_timeout);
        insert_opt!(ssh_proxy_host);
        insert_opt!(ssh_proxy_port);
        insert_opt!(ssh_proxy_username);
        insert_opt!(ssh_proxy_password);
        insert_opt!(temporary_key_pair_name);
        insert_opt!(temporary_key_pair_type);
        insert_opt!(temporary_key_pair_bits);
        insert_opt!(pause_before_connecting);
        insert_opt!(winrm_host);
        insert_opt!(winrm_port);
        insert_opt!(winrm_username);
        insert_opt!(winrm_password);
        insert_opt!(winrm_timeout);
        insert_opt!(winrm_use_ssl);
        insert_opt!(winrm_insecure);
        insert_opt!(winrm_use_ntlm);
        cfg
    }
}

// ---------------------------------------------------------------------------
// ProvisionerConfig — machine configuration steps
// ---------------------------------------------------------------------------

/// A provisioner step: what to do inside the running machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionerConfig {
    /// Provisioner type: "shell", "file", "shell-local", "breakpoint".
    #[serde(rename = "type")]
    pub provisioner_type: String,

    /// Only run for these builder names.
    #[serde(default)]
    pub only: Vec<String>,

    /// Skip for these builder names.
    #[serde(default)]
    pub except: Vec<String>,

    /// Builder-specific overrides for provisioner config.
    #[serde(default, rename = "override")]
    pub overrides: HashMap<String, Value>,

    /// Pause duration before this provisioner runs.
    #[serde(default)]
    pub pause_before: Option<String>,

    /// Maximum retries on failure.
    #[serde(default)]
    pub max_retries: Option<u32>,

    /// Timeout for this provisioner step.
    #[serde(default)]
    pub timeout: Option<String>,

    /// All remaining provisioner-specific fields.
    #[serde(flatten)]
    pub config: HashMap<String, Value>,
}

impl ProvisionerConfig {
    /// Whether this provisioner should run for the given builder name.
    pub fn applies_to(&self, builder_name: &str) -> bool {
        if !self.only.is_empty() {
            return self.only.iter().any(|n| n == builder_name);
        }
        if !self.except.is_empty() {
            return !self.except.iter().any(|n| n == builder_name);
        }
        true
    }

    /// Return the config with builder-specific overrides merged in.
    pub fn config_for_builder(&self, builder_name: &str) -> HashMap<String, Value> {
        let mut merged = self.config.clone();
        if let Some(Value::Object(overrides)) = self.overrides.get(builder_name) {
            for (k, v) in overrides {
                merged.insert(k.clone(), v.clone());
            }
        }
        merged
    }
}

// ---------------------------------------------------------------------------
// PostProcessorEntry — artifact transformation pipeline
// ---------------------------------------------------------------------------

/// Post-processor entry: string shorthand, single object, or pipeline (array).
///
/// Packer supports three entry formats in the top-level array:
/// - A bare string: `"compress"` (shorthand for `{"type": "compress"}`)
/// - A single object: `{"type": "manifest"}`
/// - An array of objects (pipeline — artifact flows through): `[{"type": "a"}, {"type": "b"}]`
/// - Mixed: `["compress", {"type": "manifest"}, [{"type": "a"}, {"type": "b"}]]`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PostProcessorEntry {
    /// Bare string shorthand: just the type name.
    StringShorthand(String),
    /// Single post-processor with full config.
    Single(PostProcessorConfig),
    /// Pipeline: artifact flows through each step sequentially.
    Pipeline(Vec<PostProcessorConfig>),
}

impl PostProcessorEntry {
    /// Flatten entry into a pipeline (single/string becomes a one-element pipeline).
    pub fn as_pipeline(&self) -> Vec<PostProcessorConfig> {
        match self {
            Self::StringShorthand(type_name) => vec![PostProcessorConfig {
                pp_type: type_name.clone(),
                only: Vec::new(),
                except: Vec::new(),
                keep_input_artifact: None,
                config: HashMap::new(),
            }],
            Self::Single(pp) => vec![pp.clone()],
            Self::Pipeline(pps) => pps.clone(),
        }
    }
}

/// A single post-processor configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostProcessorConfig {
    #[serde(rename = "type")]
    pub pp_type: String,

    /// Only run for these builder names.
    #[serde(default)]
    pub only: Vec<String>,

    /// Skip for these builder names.
    #[serde(default)]
    pub except: Vec<String>,

    /// Keep the input artifact (don't replace with output).
    #[serde(default)]
    pub keep_input_artifact: Option<bool>,

    /// All remaining post-processor-specific fields.
    #[serde(flatten)]
    pub config: HashMap<String, Value>,
}

impl PostProcessorConfig {
    /// Whether this post-processor should run for the given builder name.
    pub fn applies_to(&self, builder_name: &str) -> bool {
        if !self.only.is_empty() {
            return self.only.iter().any(|n| n == builder_name);
        }
        if !self.except.is_empty() {
            return !self.except.iter().any(|n| n == builder_name);
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Parsing — Packer JSON and shikumi YAML into the same IR
// ---------------------------------------------------------------------------

/// Parse a template from a JSON string (Packer format).
pub fn parse_json(json: &str) -> anyhow::Result<Template> {
    let template: Template = serde_json::from_str(json)?;
    Ok(template)
}

/// Parse a template from a YAML string (shikumi format).
pub fn parse_yaml(yaml: &str) -> anyhow::Result<Template> {
    let template: Template = serde_yaml_ng::from_str(yaml)?;
    Ok(template)
}

/// Parse a template from a file, detecting format from extension.
pub fn parse_file(path: &Path) -> anyhow::Result<Template> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;

    match path.extension().and_then(|e| e.to_str()) {
        Some("yaml" | "yml") => parse_yaml(&content),
        Some("json") | None => parse_json(&content),
        Some(ext) => anyhow::bail!("unsupported template format: .{ext} (use .json or .yaml)"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_template() {
        let json = r#"{
            "builders": [{"type": "null"}]
        }"#;
        let tmpl = parse_json(json).unwrap();
        assert_eq!(tmpl.builders.len(), 1);
        assert_eq!(tmpl.builders[0].builder_type, "null");
    }

    #[test]
    fn test_parse_full_template() {
        let json = r#"{
            "description": "test",
            "min_packer_version": "1.0.0",
            "sensitive-variables": ["secret"],
            "variables": {"name": "test", "secret": null},
            "builders": [
                {"type": "null", "name": "my-null", "ssh_username": "root"}
            ],
            "provisioners": [
                {"type": "shell", "inline": ["echo hello"], "pause_before": "5s", "max_retries": 3}
            ],
            "post-processors": [
                {"type": "manifest", "keep_input_artifact": true}
            ]
        }"#;
        let tmpl = parse_json(json).unwrap();
        assert_eq!(tmpl.description.as_deref(), Some("test"));
        assert_eq!(tmpl.builders[0].effective_name(), "my-null");
        assert_eq!(tmpl.builders[0].ssh_username.as_deref(), Some("root"));
        assert_eq!(tmpl.provisioners[0].pause_before.as_deref(), Some("5s"));
        assert_eq!(tmpl.provisioners[0].max_retries, Some(3));
        assert_eq!(tmpl.post_processors.len(), 1);
    }

    #[test]
    fn test_provisioner_applies_to() {
        let prov = ProvisionerConfig {
            provisioner_type: "shell".into(),
            only: vec!["docker".into()],
            except: vec![],
            overrides: HashMap::new(),
            pause_before: None,
            max_retries: None,
            timeout: None,
            config: HashMap::new(),
        };
        assert!(prov.applies_to("docker"));
        assert!(!prov.applies_to("qemu"));
    }

    #[test]
    fn test_post_processor_pipeline() {
        let json = r#"{
            "builders": [{"type": "null"}],
            "post-processors": [
                [
                    {"type": "checksum"},
                    {"type": "manifest"}
                ]
            ]
        }"#;
        let tmpl = parse_json(json).unwrap();
        assert_eq!(tmpl.post_processors.len(), 1);
        let pipeline = tmpl.post_processors[0].as_pipeline();
        assert_eq!(pipeline.len(), 2);
    }

    #[test]
    fn test_post_processor_string_shorthand() {
        let json = r#"{
            "builders": [{"type": "null"}],
            "post-processors": ["compress", {"type": "manifest"}, [{"type": "checksum"}, {"type": "shell-local"}]]
        }"#;
        let tmpl = parse_json(json).unwrap();
        assert_eq!(tmpl.post_processors.len(), 3);
        // String shorthand
        assert_eq!(tmpl.post_processors[0].as_pipeline().len(), 1);
        assert_eq!(tmpl.post_processors[0].as_pipeline()[0].pp_type, "compress");
        // Single object
        assert_eq!(tmpl.post_processors[1].as_pipeline()[0].pp_type, "manifest");
        // Pipeline
        assert_eq!(tmpl.post_processors[2].as_pipeline().len(), 2);
    }

    #[test]
    fn test_comment_convention() {
        let json = r#"{
            "_comment": "This is a note",
            "builders": [{"type": "null"}]
        }"#;
        let tmpl = parse_json(json).unwrap();
        assert_eq!(tmpl.builders.len(), 1);
    }

    #[test]
    fn test_yaml_parse() {
        let yaml = r#"
description: test yaml
builders:
  - type: null
    name: yaml-null
    ssh_username: admin
provisioners:
  - type: shell
    inline:
      - echo hello
"#;
        let tmpl = parse_yaml(yaml).unwrap();
        assert_eq!(tmpl.description.as_deref(), Some("test yaml"));
        assert_eq!(tmpl.builders[0].effective_name(), "yaml-null");
        assert_eq!(tmpl.builders[0].ssh_username.as_deref(), Some("admin"));
    }

    #[test]
    fn test_builder_full_config_merges_ssh() {
        let json = r#"{
            "builders": [{
                "type": "qemu",
                "ssh_username": "root",
                "ssh_port": 2222,
                "iso_url": "http://example.com/image.iso"
            }]
        }"#;
        let tmpl = parse_json(json).unwrap();
        let full = tmpl.builders[0].full_config();
        assert_eq!(
            full.get("ssh_username").and_then(|v| v.as_str()),
            Some("root")
        );
        assert_eq!(full.get("ssh_port").and_then(|v| v.as_u64()), Some(2222));
        assert!(full.contains_key("iso_url"));
    }

    #[test]
    fn test_provisioner_override() {
        let json = r#"{
            "builders": [{"type": "null"}],
            "provisioners": [{
                "type": "shell",
                "inline": ["echo default"],
                "override": {
                    "null": {
                        "inline": ["echo overridden"]
                    }
                }
            }]
        }"#;
        let tmpl = parse_json(json).unwrap();
        let cfg = tmpl.provisioners[0].config_for_builder("null");
        let inline = cfg.get("inline").unwrap().as_array().unwrap();
        assert_eq!(inline[0].as_str(), Some("echo overridden"));
    }

    #[test]
    fn test_sensitive_variables() {
        let json = r#"{
            "sensitive-variables": ["password", "token"],
            "variables": {"password": null, "token": null, "name": "public"},
            "builders": [{"type": "null"}]
        }"#;
        let tmpl = parse_json(json).unwrap();
        assert_eq!(tmpl.sensitive_variables.len(), 2);
        assert!(tmpl.sensitive_variables.contains(&"password".to_string()));
    }

    #[test]
    fn test_parse_json_invalid() {
        let result = parse_json("not valid json {{{");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_yaml_invalid() {
        let result = parse_yaml(":\n  :\n    : [invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_file_json_extension() {
        let dir = std::env::temp_dir();
        let path = dir.join("igata_test_template.json");
        std::fs::write(&path, r#"{"builders": [{"type": "null"}]}"#).unwrap();
        let tmpl = parse_file(&path).unwrap();
        assert_eq!(tmpl.builders.len(), 1);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_parse_file_yaml_extension() {
        let dir = std::env::temp_dir();
        let path = dir.join("igata_test_template.yaml");
        std::fs::write(&path, "builders:\n  - type: null\n").unwrap();
        let tmpl = parse_file(&path).unwrap();
        assert_eq!(tmpl.builders.len(), 1);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_parse_file_yml_extension() {
        let dir = std::env::temp_dir();
        let path = dir.join("igata_test_template.yml");
        std::fs::write(&path, "builders:\n  - type: null\n").unwrap();
        let tmpl = parse_file(&path).unwrap();
        assert_eq!(tmpl.builders.len(), 1);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_parse_file_unsupported_extension() {
        let dir = std::env::temp_dir();
        let path = dir.join("igata_test.toml");
        std::fs::write(&path, "").unwrap();
        let result = parse_file(&path);
        assert!(result.is_err());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_parse_file_missing() {
        let result = parse_file(Path::new("/nonexistent/file.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_provisioner_applies_to_except() {
        let prov = ProvisionerConfig {
            provisioner_type: "shell".into(),
            only: vec![],
            except: vec!["docker".into()],
            overrides: HashMap::new(),
            pause_before: None,
            max_retries: None,
            timeout: None,
            config: HashMap::new(),
        };
        assert!(!prov.applies_to("docker"));
        assert!(prov.applies_to("qemu"));
        assert!(prov.applies_to("null"));
    }

    #[test]
    fn test_provisioner_applies_to_all() {
        let prov = ProvisionerConfig {
            provisioner_type: "shell".into(),
            only: vec![],
            except: vec![],
            overrides: HashMap::new(),
            pause_before: None,
            max_retries: None,
            timeout: None,
            config: HashMap::new(),
        };
        assert!(prov.applies_to("docker"));
        assert!(prov.applies_to("qemu"));
        assert!(prov.applies_to("null"));
    }

    #[test]
    fn test_provisioner_config_for_builder_no_override() {
        let prov = ProvisionerConfig {
            provisioner_type: "shell".into(),
            only: vec![],
            except: vec![],
            overrides: HashMap::new(),
            pause_before: None,
            max_retries: None,
            timeout: None,
            config: HashMap::from([("inline".into(), serde_json::json!(["echo hi"]))]),
        };
        let cfg = prov.config_for_builder("null");
        assert_eq!(cfg.get("inline").unwrap().as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_post_processor_applies_to_only() {
        let pp = PostProcessorConfig {
            pp_type: "manifest".into(),
            only: vec!["docker".into()],
            except: vec![],
            keep_input_artifact: None,
            config: HashMap::new(),
        };
        assert!(pp.applies_to("docker"));
        assert!(!pp.applies_to("qemu"));
    }

    #[test]
    fn test_post_processor_applies_to_except() {
        let pp = PostProcessorConfig {
            pp_type: "manifest".into(),
            only: vec![],
            except: vec!["null".into()],
            keep_input_artifact: None,
            config: HashMap::new(),
        };
        assert!(!pp.applies_to("null"));
        assert!(pp.applies_to("docker"));
    }

    #[test]
    fn test_builder_effective_name_custom() {
        let json = r#"{"builders": [{"type": "docker", "name": "my-docker"}]}"#;
        let tmpl = parse_json(json).unwrap();
        assert_eq!(tmpl.builders[0].effective_name(), "my-docker");
    }

    #[test]
    fn test_builder_effective_name_default() {
        let json = r#"{"builders": [{"type": "docker"}]}"#;
        let tmpl = parse_json(json).unwrap();
        assert_eq!(tmpl.builders[0].effective_name(), "docker");
    }

    #[test]
    fn test_yaml_with_post_processors() {
        let yaml = r#"
builders:
  - type: null
post_processors:
  - type: manifest
    output: out.json
"#;
        let tmpl = parse_yaml(yaml).unwrap();
        assert_eq!(tmpl.post_processors.len(), 1);
    }

    #[test]
    fn test_empty_template_defaults() {
        let json = r#"{"builders": [{"type": "null"}]}"#;
        let tmpl = parse_json(json).unwrap();
        assert!(tmpl.description.is_none());
        assert!(tmpl.min_version.is_none());
        assert!(tmpl.sensitive_variables.is_empty());
        assert!(tmpl.variables.is_empty());
        assert!(tmpl.provisioners.is_empty());
        assert!(tmpl.post_processors.is_empty());
    }

    #[test]
    fn test_docker_builder_with_ssh_communicator() {
        let json = r#"{
            "builders": [{
                "type": "docker",
                "image": "ubuntu:22.04",
                "communicator": "ssh",
                "ssh_username": "root",
                "ssh_password": "secret",
                "ssh_timeout": "10m",
                "ssh_bastion_host": "jump.example.com",
                "ssh_bastion_port": 2222
            }]
        }"#;
        let tmpl = parse_json(json).unwrap();
        let b = &tmpl.builders[0];
        assert_eq!(b.communicator.as_deref(), Some("ssh"));
        assert_eq!(b.ssh_username.as_deref(), Some("root"));
        assert_eq!(b.ssh_password.as_deref(), Some("secret"));
        assert_eq!(b.ssh_timeout.as_deref(), Some("10m"));
        assert_eq!(b.ssh_bastion_host.as_deref(), Some("jump.example.com"));
        assert_eq!(b.ssh_bastion_port, Some(2222));

        let full = b.full_config();
        assert_eq!(full.get("ssh_username").and_then(|v| v.as_str()), Some("root"));
        assert_eq!(full.get("ssh_bastion_port").and_then(|v| v.as_u64()), Some(2222));
        assert_eq!(full.get("image").and_then(|v| v.as_str()), Some("ubuntu:22.04"));
    }

    /// Comprehensive Packer SSH field compliance test.
    /// Verifies ALL Packer SSH communicator fields are parsed and merged correctly.
    #[test]
    fn test_packer_ssh_field_compliance() {
        let json = r#"{
            "builders": [{
                "type": "qemu",
                "communicator": "ssh",
                "ssh_host": "10.0.0.1",
                "ssh_port": 2222,
                "ssh_username": "admin",
                "ssh_password": "pass123",
                "ssh_private_key_file": "/keys/id_rsa",
                "ssh_keypair_name": "my-key",
                "ssh_agent_auth": true,
                "ssh_timeout": "10m",
                "ssh_handshake_attempts": 20,
                "ssh_disable_agent_forwarding": true,
                "ssh_bastion_host": "bastion.example.com",
                "ssh_bastion_port": 22,
                "ssh_bastion_username": "jump",
                "ssh_bastion_password": "jumppass",
                "ssh_bastion_private_key_file": "/keys/bastion",
                "ssh_bastion_agent_auth": false,
                "ssh_bastion_certificate_file": "/certs/bastion.pub",
                "ssh_bastion_interactive": true,
                "ssh_file_transfer_method": "sftp",
                "ssh_proxy_host": "proxy.example.com",
                "ssh_proxy_port": 1080,
                "ssh_proxy_username": "proxyuser",
                "ssh_proxy_password": "proxypass",
                "ssh_keep_alive_interval": "5s",
                "ssh_read_write_timeout": "30s",
                "ssh_pty": true,
                "ssh_certificate_file": "/certs/user.pub",
                "ssh_clear_authorized_keys": true,
                "ssh_ciphers": ["aes256-ctr", "aes128-gcm@openssh.com"],
                "ssh_key_exchange_algorithms": ["curve25519-sha256"],
                "ssh_remote_tunnels": ["8080:localhost:80"],
                "ssh_local_tunnels": ["3306:dbhost:3306"],
                "temporary_key_pair_name": "packer-tmp",
                "temporary_key_pair_type": "ed25519",
                "temporary_key_pair_bits": 256,
                "pause_before_connecting": "5s",
                "iso_url": "http://example.com/image.iso"
            }]
        }"#;
        let tmpl = parse_json(json).unwrap();
        let b = &tmpl.builders[0];

        // Verify typed fields
        assert_eq!(b.communicator.as_deref(), Some("ssh"));
        assert_eq!(b.ssh_host.as_deref(), Some("10.0.0.1"));
        assert_eq!(b.ssh_port, Some(2222));
        assert_eq!(b.ssh_username.as_deref(), Some("admin"));
        assert_eq!(b.ssh_password.as_deref(), Some("pass123"));
        assert_eq!(b.ssh_private_key_file.as_deref(), Some("/keys/id_rsa"));
        assert_eq!(b.ssh_keypair_name.as_deref(), Some("my-key"));
        assert_eq!(b.ssh_agent_auth, Some(true));
        assert_eq!(b.ssh_timeout.as_deref(), Some("10m"));
        assert_eq!(b.ssh_handshake_attempts, Some(20));
        assert_eq!(b.ssh_disable_agent_forwarding, Some(true));
        assert_eq!(b.ssh_bastion_host.as_deref(), Some("bastion.example.com"));
        assert_eq!(b.ssh_bastion_port, Some(22));
        assert_eq!(b.ssh_bastion_username.as_deref(), Some("jump"));
        assert_eq!(b.ssh_bastion_password.as_deref(), Some("jumppass"));
        assert_eq!(b.ssh_bastion_private_key_file.as_deref(), Some("/keys/bastion"));
        assert_eq!(b.ssh_bastion_agent_auth, Some(false));
        assert_eq!(b.ssh_bastion_certificate_file.as_deref(), Some("/certs/bastion.pub"));
        assert_eq!(b.ssh_bastion_interactive, Some(true));
        assert_eq!(b.ssh_file_transfer_method.as_deref(), Some("sftp"));
        assert_eq!(b.ssh_proxy_host.as_deref(), Some("proxy.example.com"));
        assert_eq!(b.ssh_proxy_port, Some(1080));
        assert_eq!(b.ssh_proxy_username.as_deref(), Some("proxyuser"));
        assert_eq!(b.ssh_proxy_password.as_deref(), Some("proxypass"));
        assert_eq!(b.ssh_keep_alive_interval.as_deref(), Some("5s"));
        assert_eq!(b.ssh_read_write_timeout.as_deref(), Some("30s"));
        assert_eq!(b.ssh_pty, Some(true));
        assert_eq!(b.ssh_certificate_file.as_deref(), Some("/certs/user.pub"));
        assert_eq!(b.ssh_clear_authorized_keys, Some(true));
        assert_eq!(b.ssh_ciphers.as_ref().unwrap().len(), 2);
        assert_eq!(b.ssh_key_exchange_algorithms.as_ref().unwrap().len(), 1);
        assert_eq!(b.ssh_remote_tunnels.as_ref().unwrap().len(), 1);
        assert_eq!(b.ssh_local_tunnels.as_ref().unwrap().len(), 1);
        assert_eq!(b.temporary_key_pair_name.as_deref(), Some("packer-tmp"));
        assert_eq!(b.temporary_key_pair_type.as_deref(), Some("ed25519"));
        assert_eq!(b.temporary_key_pair_bits, Some(256));
        assert_eq!(b.pause_before_connecting.as_deref(), Some("5s"));

        // Verify full_config merges all SSH fields + builder-specific fields
        let full = b.full_config();
        assert_eq!(full.get("ssh_username").and_then(|v| v.as_str()), Some("admin"));
        assert_eq!(full.get("ssh_bastion_host").and_then(|v| v.as_str()), Some("bastion.example.com"));
        assert_eq!(full.get("ssh_proxy_port").and_then(|v| v.as_u64()), Some(1080));
        assert_eq!(full.get("temporary_key_pair_type").and_then(|v| v.as_str()), Some("ed25519"));
        assert_eq!(full.get("pause_before_connecting").and_then(|v| v.as_str()), Some("5s"));
        assert!(full.contains_key("iso_url")); // builder-specific field preserved
    }

    /// Comprehensive WinRM field compliance test.
    #[test]
    fn test_packer_winrm_field_compliance() {
        let json = r#"{
            "builders": [{
                "type": "qemu",
                "communicator": "winrm",
                "winrm_host": "10.0.0.2",
                "winrm_port": 5986,
                "winrm_username": "Administrator",
                "winrm_password": "P@ssw0rd",
                "winrm_timeout": "30m",
                "winrm_use_ssl": true,
                "winrm_insecure": false,
                "winrm_use_ntlm": true,
                "iso_url": "http://example.com/win.iso"
            }]
        }"#;
        let tmpl = parse_json(json).unwrap();
        let b = &tmpl.builders[0];
        assert_eq!(b.communicator.as_deref(), Some("winrm"));
        assert_eq!(b.winrm_host.as_deref(), Some("10.0.0.2"));
        assert_eq!(b.winrm_port, Some(5986));
        assert_eq!(b.winrm_username.as_deref(), Some("Administrator"));
        assert_eq!(b.winrm_password.as_deref(), Some("P@ssw0rd"));
        assert_eq!(b.winrm_timeout.as_deref(), Some("30m"));
        assert_eq!(b.winrm_use_ssl, Some(true));
        assert_eq!(b.winrm_insecure, Some(false));
        assert_eq!(b.winrm_use_ntlm, Some(true));

        let full = b.full_config();
        assert_eq!(full.get("winrm_username").and_then(|v| v.as_str()), Some("Administrator"));
        assert_eq!(full.get("winrm_use_ntlm").and_then(|v| v.as_bool()), Some(true));
    }

    /// Test provisioner common fields (pause_before, max_retries, timeout).
    #[test]
    fn test_provisioner_common_fields() {
        let json = r#"{
            "builders": [{"type": "null"}],
            "provisioners": [{
                "type": "shell",
                "inline": ["echo hello"],
                "pause_before": "10s",
                "max_retries": 3,
                "timeout": "5m"
            }]
        }"#;
        let tmpl = parse_json(json).unwrap();
        let p = &tmpl.provisioners[0];
        assert_eq!(p.pause_before.as_deref(), Some("10s"));
        assert_eq!(p.max_retries, Some(3));
        assert_eq!(p.timeout.as_deref(), Some("5m"));
    }

    /// Test post-processor keep_input_artifact field.
    #[test]
    fn test_post_processor_keep_input_artifact() {
        let json = r#"{
            "builders": [{"type": "null"}],
            "post-processors": [
                {"type": "checksum", "keep_input_artifact": true},
                {"type": "manifest", "keep_input_artifact": false}
            ]
        }"#;
        let tmpl = parse_json(json).unwrap();
        let pp0 = tmpl.post_processors[0].as_pipeline();
        let pp1 = tmpl.post_processors[1].as_pipeline();
        assert_eq!(pp0[0].keep_input_artifact, Some(true));
        assert_eq!(pp1[0].keep_input_artifact, Some(false));
    }

    /// Test that Packer JSON and shikumi YAML produce identical IR.
    #[test]
    fn test_json_yaml_ir_equivalence() {
        let json = r#"{
            "description": "test",
            "variables": {"name": "hello"},
            "builders": [{"type": "null", "name": "my-null"}],
            "provisioners": [{"type": "shell-local", "inline": ["echo hi"]}]
        }"#;
        let yaml = r#"
description: test
variables:
  name: hello
builders:
  - type: "null"
    name: my-null
provisioners:
  - type: shell-local
    inline:
      - echo hi
"#;
        let from_json = parse_json(json).unwrap();
        let from_yaml = parse_yaml(yaml).unwrap();

        assert_eq!(from_json.description, from_yaml.description);
        assert_eq!(from_json.builders.len(), from_yaml.builders.len());
        assert_eq!(
            from_json.builders[0].effective_name(),
            from_yaml.builders[0].effective_name()
        );
        assert_eq!(from_json.provisioners.len(), from_yaml.provisioners.len());
        assert_eq!(
            from_json.provisioners[0].provisioner_type,
            from_yaml.provisioners[0].provisioner_type
        );
    }

    /// Test full Packer template with all sections.
    #[test]
    fn test_full_packer_template() {
        let json = r#"{
            "_comment": "Full example template",
            "description": "Production AMI builder",
            "min_packer_version": "1.7.0",
            "sensitive-variables": ["aws_secret"],
            "variables": {
                "region": "us-east-1",
                "ami_name": "my-app-{{timestamp}}",
                "aws_secret": null
            },
            "builders": [{
                "type": "amazon-ebs",
                "name": "production",
                "region": "{{user `region`}}",
                "source_ami": "ami-12345678",
                "instance_type": "t3.micro",
                "ssh_username": "ec2-user",
                "ami_name": "{{user `ami_name`}}"
            }],
            "provisioners": [
                {
                    "type": "shell",
                    "inline": ["sudo yum update -y"],
                    "pause_before": "10s",
                    "max_retries": 2
                },
                {
                    "type": "file",
                    "source": "config/app.conf",
                    "destination": "/etc/app.conf",
                    "only": ["production"]
                },
                {
                    "type": "shell-local",
                    "inline": ["echo build complete"],
                    "except": ["staging"]
                }
            ],
            "post-processors": [
                "compress",
                {"type": "manifest", "output": "manifest.json", "strip_path": true},
                [
                    {"type": "checksum", "checksum_types": ["sha256"]},
                    {"type": "shell-local", "inline": ["echo checksummed"]}
                ]
            ]
        }"#;
        let tmpl = parse_json(json).unwrap();

        // Top-level fields
        assert_eq!(tmpl.description.as_deref(), Some("Production AMI builder"));
        assert_eq!(tmpl.min_version.as_deref(), Some("1.7.0"));
        assert_eq!(tmpl.sensitive_variables, vec!["aws_secret"]);
        assert_eq!(tmpl.variables.len(), 3);
        assert_eq!(tmpl.variables["aws_secret"], Value::Null);

        // Builder
        assert_eq!(tmpl.builders.len(), 1);
        assert_eq!(tmpl.builders[0].effective_name(), "production");
        assert_eq!(tmpl.builders[0].ssh_username.as_deref(), Some("ec2-user"));

        // Provisioners
        assert_eq!(tmpl.provisioners.len(), 3);
        assert_eq!(tmpl.provisioners[0].provisioner_type, "shell");
        assert_eq!(tmpl.provisioners[0].pause_before.as_deref(), Some("10s"));
        assert_eq!(tmpl.provisioners[0].max_retries, Some(2));
        assert_eq!(tmpl.provisioners[1].provisioner_type, "file");
        assert!(tmpl.provisioners[1].applies_to("production"));
        assert!(!tmpl.provisioners[1].applies_to("staging"));
        assert!(tmpl.provisioners[2].applies_to("production"));

        // Post-processors: string shorthand + single + pipeline
        assert_eq!(tmpl.post_processors.len(), 3);
        // String shorthand
        assert_eq!(tmpl.post_processors[0].as_pipeline()[0].pp_type, "compress");
        // Single with config
        let manifest = &tmpl.post_processors[1].as_pipeline()[0];
        assert_eq!(manifest.pp_type, "manifest");
        assert_eq!(manifest.config.get("strip_path").and_then(|v| v.as_bool()), Some(true));
        // Pipeline
        let pipeline = tmpl.post_processors[2].as_pipeline();
        assert_eq!(pipeline.len(), 2);
        assert_eq!(pipeline[0].pp_type, "checksum");
        assert_eq!(pipeline[1].pp_type, "shell-local");
    }
}
