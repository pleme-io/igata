use crate::error::Result;
use std::collections::BTreeMap;
use std::path::Path;

/// Resolves a single variable to its concrete value.
///
/// Implement this trait to add custom variable sources beyond
/// literal/file/env (e.g., secret vaults, HTTP endpoints, databases).
pub trait VariableResolver: std::fmt::Debug + Send + Sync {
    /// Resolve to a string value.
    fn resolve(&self) -> Result<String>;

    /// Human-readable description of the source (for error messages and logging).
    fn describe(&self) -> String;
}

/// Resolves a complete set of named variables.
///
/// The default implementation iterates resolvers, but you can override
/// for batch resolution (e.g., fetching multiple secrets in a single API call).
pub trait ContextResolver: std::fmt::Debug + Send + Sync {
    /// Resolve all variables to their concrete values.
    fn resolve_all(&self) -> Result<BTreeMap<String, String>>;
}

/// Renders a template string with resolved variables.
///
/// Swap the template backend (MiniJinja, Tera, Handlebars, or plain
/// string replacement) by implementing this trait.
pub trait TemplateRenderer: std::fmt::Debug + Send + Sync {
    /// Render a template string with the given variable map.
    fn render(&self, template: &str, variables: &BTreeMap<String, String>) -> Result<String>;
}

/// Reads template source content.
///
/// Default implementations read from the filesystem, but you can
/// mock this for testing or source templates from other locations
/// (e.g., Nix store, HTTP, embedded resources).
pub trait TemplateLoader: std::fmt::Debug + Send + Sync {
    /// Load template content from the given path.
    fn load(&self, path: &Path) -> Result<String>;
}

/// Writes rendered output to a target.
///
/// Separate from rendering so you can mock file I/O in tests,
/// add logging, or write to non-filesystem targets.
pub trait OutputWriter: std::fmt::Debug + Send + Sync {
    /// Write content to the target path with the given permissions.
    fn write(&self, path: &Path, content: &str, mode: u32) -> Result<()>;
}

/// Loads and deserializes a manifest.
///
/// Override to load manifests from sources other than local JSON files.
pub trait ManifestLoader: std::fmt::Debug + Send + Sync {
    /// Load a manifest from the given path.
    fn load(&self, path: &Path) -> Result<crate::manifest::Manifest>;
}

/// Observes the rendering pipeline for logging, metrics, or auditing.
///
/// All methods have default no-op implementations so you only override
/// the events you care about.
pub trait RenderObserver: std::fmt::Debug + Send + Sync {
    /// Called before rendering a template entry.
    fn on_render_start(&self, _name: &str, _source: &Path, _target: &Path) {}

    /// Called after successfully rendering a template entry.
    fn on_render_complete(&self, _name: &str, _target: &Path) {}

    /// Called when a rendering error occurs.
    fn on_render_error(&self, _name: &str, _error: &crate::error::Error) {}

}
