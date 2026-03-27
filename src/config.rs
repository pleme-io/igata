use shikumi::{ConfigDiscovery, Format, ProviderChain};

/// Global configuration for igata, loaded from `~/.config/igata/igata.yaml`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub aws: AwsConfig,
    #[serde(default)]
    pub docker: DockerConfig,
    #[serde(default)]
    pub qemu: QemuConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Defaults {
    #[serde(default = "default_ssh_timeout")]
    pub ssh_timeout: String,
    #[serde(default = "default_ssh_username")]
    pub ssh_username: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AwsConfig {
    #[serde(default = "default_aws_region")]
    pub region: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DockerConfig {
    #[serde(default = "default_docker_host")]
    pub host: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QemuConfig {
    #[serde(default = "default_qemu_binary")]
    pub binary: String,
    #[serde(default = "default_true")]
    pub headless: bool,
    #[serde(default = "default_qemu_accelerator")]
    pub accelerator: String,
}

fn default_ssh_timeout() -> String {
    "5m".to_string()
}
fn default_ssh_username() -> String {
    "root".to_string()
}
fn default_aws_region() -> String {
    "us-east-1".to_string()
}
fn default_docker_host() -> String {
    "unix:///var/run/docker.sock".to_string()
}
fn default_qemu_binary() -> String {
    "qemu-system-x86_64".to_string()
}
fn default_true() -> bool {
    true
}
fn default_qemu_accelerator() -> String {
    "hvf".to_string()
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            ssh_timeout: default_ssh_timeout(),
            ssh_username: default_ssh_username(),
        }
    }
}

impl Default for AwsConfig {
    fn default() -> Self {
        Self {
            region: default_aws_region(),
        }
    }
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            host: default_docker_host(),
        }
    }
}

impl Default for QemuConfig {
    fn default() -> Self {
        Self {
            binary: default_qemu_binary(),
            headless: true,
            accelerator: default_qemu_accelerator(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            defaults: Defaults::default(),
            aws: AwsConfig::default(),
            docker: DockerConfig::default(),
            qemu: QemuConfig::default(),
        }
    }
}

/// Load configuration using shikumi config discovery.
///
/// Discovery order:
/// 1. `$IGATA_CONFIG` env var (if set)
/// 2. `$XDG_CONFIG_HOME/igata/igata.yaml` (or .yml / .toml)
/// 3. `$HOME/.config/igata/igata.yaml` (or .yml / .toml)
///
/// If no config file is found, returns defaults. Config file values
/// are overlaid with `IGATA_` prefixed env vars.
pub fn load() -> Config {
    let defaults = Config::default();

    let config_path = ConfigDiscovery::new("igata")
        .env_override("IGATA_CONFIG")
        .formats(&[Format::Yaml, Format::Toml])
        .discover();

    match config_path {
        Ok(path) => match ProviderChain::new()
            .with_defaults(&defaults)
            .with_file(&path)
            .with_env("IGATA_")
            .extract::<Config>()
        {
            Ok(cfg) => cfg,
            Err(_) => defaults,
        },
        Err(_) => match ProviderChain::new()
            .with_defaults(&defaults)
            .with_env("IGATA_")
            .extract::<Config>()
        {
            Ok(cfg) => cfg,
            Err(_) => defaults,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.defaults.ssh_timeout, "5m");
        assert_eq!(cfg.defaults.ssh_username, "root");
        assert_eq!(cfg.aws.region, "us-east-1");
        assert!(cfg.qemu.headless);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let cfg = Config::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg2.defaults.ssh_timeout, cfg.defaults.ssh_timeout);
    }
}
