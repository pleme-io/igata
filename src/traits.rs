use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Output from a command executed on a machine.
#[derive(Debug, Clone, Default)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    #[allow(dead_code)]
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// An artifact produced by a builder.
#[derive(Debug, Clone)]
pub struct Artifact {
    /// Unique ID for the artifact (e.g., Docker image ID, AMI ID).
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Files that compose this artifact.
    pub files: Vec<String>,
    /// Builder type that produced this artifact.
    pub builder_type: String,
    /// Builder name that produced this artifact.
    pub builder_name: String,
    /// Additional metadata.
    #[allow(dead_code)]
    pub metadata: HashMap<String, String>,
}

impl Artifact {
    pub fn empty(builder_type: &str, builder_name: &str) -> Self {
        Self {
            id: String::new(),
            description: String::new(),
            files: Vec::new(),
            builder_type: builder_type.to_string(),
            builder_name: builder_name.to_string(),
            metadata: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Communicator — how we talk to machines
// ---------------------------------------------------------------------------

/// Communicates with a running machine (SSH, docker exec, etc.).
#[async_trait::async_trait]
pub trait Communicator: Send + Sync {
    /// Upload a local file to the remote machine.
    async fn upload(&self, src: &Path, dst: &str) -> Result<()>;

    /// Download a file from the remote machine to a local path.
    async fn download(&self, src: &str, dst: &Path) -> Result<()>;

    /// Execute a command on the remote machine.
    async fn exec(&self, command: &str) -> Result<CommandOutput>;
}

// ---------------------------------------------------------------------------
// Builder — machine lifecycle
// ---------------------------------------------------------------------------

/// Builds a machine and returns a communicator to interact with it.
#[async_trait::async_trait]
pub trait Builder: Send + Sync {
    /// Validate the builder configuration.
    fn prepare(&self, config: &HashMap<String, Value>) -> Result<()>;

    /// Start the machine and return a communicator.
    async fn run(
        &self,
        config: &HashMap<String, Value>,
    ) -> Result<Option<Box<dyn Communicator>>>;

    /// Retrieve the artifact produced by this build.
    async fn artifact(&self) -> Result<Artifact>;

    /// Clean up resources (stop VM, remove container, etc.).
    async fn cleanup(&self) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Provisioner — configure the machine
// ---------------------------------------------------------------------------

/// Provisions (configures) a running machine.
#[async_trait::async_trait]
pub trait Provisioner: Send + Sync {
    /// Run the provisioner against the machine.
    async fn provision(
        &self,
        config: &HashMap<String, Value>,
        comm: Option<&dyn Communicator>,
    ) -> Result<()>;
}

// ---------------------------------------------------------------------------
// PostProcessor — transform artifacts
// ---------------------------------------------------------------------------

/// Transforms an artifact after the build completes.
#[async_trait::async_trait]
pub trait PostProcessor: Send + Sync {
    /// Process an artifact and return a (possibly transformed) artifact.
    async fn process(
        &self,
        config: &HashMap<String, Value>,
        artifact: Artifact,
    ) -> Result<Artifact>;
}

// ---------------------------------------------------------------------------
// Registries — factory functions for creating instances
// ---------------------------------------------------------------------------

/// Registry of all known builders, provisioners, and post-processors.
pub struct Registry {
    builders: HashMap<String, Box<dyn Fn() -> Box<dyn Builder>>>,
    provisioners: HashMap<String, Box<dyn Fn() -> Box<dyn Provisioner>>>,
    post_processors: HashMap<String, Box<dyn Fn() -> Box<dyn PostProcessor>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            builders: HashMap::new(),
            provisioners: HashMap::new(),
            post_processors: HashMap::new(),
        }
    }

    pub fn register_builder(
        &mut self,
        name: &str,
        factory: impl Fn() -> Box<dyn Builder> + 'static,
    ) {
        self.builders.insert(name.to_string(), Box::new(factory));
    }

    pub fn register_provisioner(
        &mut self,
        name: &str,
        factory: impl Fn() -> Box<dyn Provisioner> + 'static,
    ) {
        self.provisioners
            .insert(name.to_string(), Box::new(factory));
    }

    pub fn register_post_processor(
        &mut self,
        name: &str,
        factory: impl Fn() -> Box<dyn PostProcessor> + 'static,
    ) {
        self.post_processors
            .insert(name.to_string(), Box::new(factory));
    }

    pub fn create_builder(&self, name: &str) -> Option<Box<dyn Builder>> {
        self.builders.get(name).map(|f| f())
    }

    pub fn create_provisioner(&self, name: &str) -> Option<Box<dyn Provisioner>> {
        self.provisioners.get(name).map(|f| f())
    }

    pub fn create_post_processor(&self, name: &str) -> Option<Box<dyn PostProcessor>> {
        self.post_processors.get(name).map(|f| f())
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_output_default() {
        let out = CommandOutput::default();
        assert_eq!(out.stdout, "");
        assert_eq!(out.stderr, "");
        assert_eq!(out.exit_code, 0);
        assert!(out.success());
    }

    #[test]
    fn test_command_output_success() {
        let out = CommandOutput {
            exit_code: 0,
            ..Default::default()
        };
        assert!(out.success());
    }

    #[test]
    fn test_command_output_failure() {
        let out = CommandOutput {
            exit_code: 1,
            stderr: "error".into(),
            ..Default::default()
        };
        assert!(!out.success());
    }

    #[test]
    fn test_command_output_negative_code() {
        let out = CommandOutput {
            exit_code: -1,
            ..Default::default()
        };
        assert!(!out.success());
    }

    #[test]
    fn test_artifact_empty() {
        let a = Artifact::empty("null", "my-null");
        assert_eq!(a.builder_type, "null");
        assert_eq!(a.builder_name, "my-null");
        assert!(a.id.is_empty());
        assert!(a.files.is_empty());
        assert!(a.metadata.is_empty());
    }

    #[test]
    fn test_artifact_clone() {
        let a = Artifact {
            id: "ami-123".into(),
            description: "test ami".into(),
            files: vec!["file.qcow2".into()],
            builder_type: "qemu".into(),
            builder_name: "my-qemu".into(),
            metadata: HashMap::from([("key".into(), "val".into())]),
        };
        let b = a.clone();
        assert_eq!(a.id, b.id);
        assert_eq!(a.files, b.files);
        assert_eq!(a.metadata, b.metadata);
    }

    #[test]
    fn test_registry_new_empty() {
        let reg = Registry::new();
        assert!(reg.create_builder("null").is_none());
        assert!(reg.create_provisioner("shell").is_none());
        assert!(reg.create_post_processor("manifest").is_none());
    }

    #[test]
    fn test_registry_default_equals_new() {
        let reg = Registry::default();
        assert!(reg.create_builder("null").is_none());
    }

    #[test]
    fn test_registry_register_and_create_builder() {
        let mut reg = Registry::new();
        reg.register_builder("null", || {
            Box::new(crate::builder::null::NullBuilder::new())
        });
        assert!(reg.create_builder("null").is_some());
        assert!(reg.create_builder("docker").is_none());
    }

    #[test]
    fn test_registry_register_and_create_provisioner() {
        let mut reg = Registry::new();
        reg.register_provisioner("breakpoint", || {
            Box::new(crate::provisioner::breakpoint::BreakpointProvisioner)
        });
        assert!(reg.create_provisioner("breakpoint").is_some());
        assert!(reg.create_provisioner("shell").is_none());
    }

    #[test]
    fn test_registry_register_and_create_post_processor() {
        let mut reg = Registry::new();
        reg.register_post_processor("manifest", || {
            Box::new(crate::post_processor::manifest::ManifestPostProcessor)
        });
        assert!(reg.create_post_processor("manifest").is_some());
        assert!(reg.create_post_processor("checksum").is_none());
    }

    #[test]
    fn test_registry_overwrite_registration() {
        let mut reg = Registry::new();
        reg.register_builder("test", || {
            Box::new(crate::builder::null::NullBuilder::new())
        });
        // Re-register should overwrite without error
        reg.register_builder("test", || {
            Box::new(crate::builder::null::NullBuilder::new())
        });
        assert!(reg.create_builder("test").is_some());
    }

    #[test]
    fn test_registry_full_registration() {
        let mut reg = Registry::new();
        crate::builder::register_all(&mut reg);
        crate::provisioner::register_all(&mut reg);
        crate::post_processor::register_all(&mut reg);

        assert!(reg.create_builder("null").is_some());
        assert!(reg.create_builder("docker").is_some());
        assert!(reg.create_builder("qemu").is_some());
        assert!(reg.create_builder("amazon-ebs").is_some());

        assert!(reg.create_provisioner("shell").is_some());
        assert!(reg.create_provisioner("file").is_some());
        assert!(reg.create_provisioner("shell-local").is_some());
        assert!(reg.create_provisioner("breakpoint").is_some());

        assert!(reg.create_post_processor("manifest").is_some());
        assert!(reg.create_post_processor("checksum").is_some());
        assert!(reg.create_post_processor("shell-local").is_some());
        assert!(reg.create_post_processor("compress").is_some());
    }
}
