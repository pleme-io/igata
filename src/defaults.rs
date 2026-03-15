use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::syntax::Syntax;
use crate::traits::{
    ContextResolver, ManifestLoader, OutputWriter, RenderObserver, TemplateLoader,
    TemplateRenderer, VariableResolver,
};
use minijinja::syntax::SyntaxConfig;
use minijinja::Environment;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::OnceLock;

// ── Variable resolvers ─────────────────────────────────────────────────

/// Resolves to a literal string value.
#[derive(Debug, Clone)]
pub struct LiteralResolver {
    pub value: String,
}

impl VariableResolver for LiteralResolver {
    fn resolve(&self) -> Result<String> {
        Ok(self.value.clone())
    }

    fn describe(&self) -> String {
        "literal".into()
    }
}

/// Reads a value from a file, trimming trailing whitespace.
#[derive(Debug, Clone)]
pub struct FileResolver {
    pub path: String,
}

impl VariableResolver for FileResolver {
    fn resolve(&self) -> Result<String> {
        let p = Path::new(&self.path);
        let content = std::fs::read_to_string(p).map_err(|e| Error::ReadFile {
            path: p.to_path_buf(),
            source: e,
        })?;
        Ok(content.trim_end().to_owned())
    }

    fn describe(&self) -> String {
        format!("file:{}", self.path)
    }
}

/// Reads a value from an environment variable.
#[derive(Debug, Clone)]
pub struct EnvResolver {
    pub name: String,
}

impl VariableResolver for EnvResolver {
    fn resolve(&self) -> Result<String> {
        std::env::var(&self.name).map_err(|_| Error::EnvNotSet {
            name: self.name.clone(),
        })
    }

    fn describe(&self) -> String {
        format!("env:{}", self.name)
    }
}

// ── Context resolver ───────────────────────────────────────────────────

/// Default context resolver: iterates a map of named `VariableResolver`s.
#[derive(Debug, Default)]
pub struct MapContextResolver {
    pub resolvers: BTreeMap<String, Box<dyn VariableResolver>>,
}

impl MapContextResolver {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, resolver: Box<dyn VariableResolver>) {
        self.resolvers.insert(name.into(), resolver);
    }
}

impl ContextResolver for MapContextResolver {
    fn resolve_all(&self) -> Result<BTreeMap<String, String>> {
        self.resolvers
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.resolve()?)))
            .collect()
    }
}

// ── Template renderer ──────────────────────────────────────────────────

/// MiniJinja-backed template renderer.
///
/// Caches the `SyntaxConfig` so it is computed (and leaked) only once,
/// regardless of how many times `render` is called.
#[derive(Debug)]
pub struct MiniJinjaRenderer {
    syntax: Syntax,
    cached_config: OnceLock<SyntaxConfig>,
}

impl MiniJinjaRenderer {
    #[must_use]
    pub fn new(syntax: Syntax) -> Self {
        Self {
            syntax,
            cached_config: OnceLock::new(),
        }
    }

    fn syntax_config(&self) -> Result<&SyntaxConfig> {
        if let Some(config) = self.cached_config.get() {
            return Ok(config);
        }
        let config = self.syntax.to_config()?;
        // If another thread raced us, that's fine — we just discard ours.
        let _ = self.cached_config.set(config);
        Ok(self.cached_config.get().expect("just set"))
    }

    /// Eagerly validate syntax by building and caching the config.
    /// Call this once at construction time to surface config errors early
    /// without leaking strings a second time via `Syntax::to_config()`.
    pub fn validate(&self) -> Result<()> {
        self.syntax_config()?;
        Ok(())
    }
}

impl Default for MiniJinjaRenderer {
    fn default() -> Self {
        Self::new(Syntax::default())
    }
}

impl TemplateRenderer for MiniJinjaRenderer {
    fn render(&self, template: &str, variables: &BTreeMap<String, String>) -> Result<String> {
        let config = self.syntax_config()?;
        let mut env = Environment::new();
        env.set_syntax(config.clone());
        let tmpl = env.template_from_str(template)?;
        Ok(tmpl.render(variables)?)
    }
}

// ── Template loader ────────────────────────────────────────────────────

/// Loads templates from the local filesystem.
#[derive(Debug, Default)]
pub struct FsTemplateLoader;

impl TemplateLoader for FsTemplateLoader {
    fn load(&self, path: &Path) -> Result<String> {
        std::fs::read_to_string(path).map_err(|e| Error::ReadFile {
            path: path.to_path_buf(),
            source: e,
        })
    }
}

// ── Output writer ──────────────────────────────────────────────────────

/// Writes rendered content to the local filesystem.
#[derive(Debug, Default)]
pub struct FsOutputWriter;

impl OutputWriter for FsOutputWriter {
    fn write(&self, path: &Path, content: &str, mode: u32) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::WriteFile {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        std::fs::write(path, content).map_err(|e| Error::WriteFile {
            path: path.to_path_buf(),
            source: e,
        })?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| {
            Error::WriteFile {
                path: path.to_path_buf(),
                source: e,
            }
        })?;
        Ok(())
    }
}

// ── Manifest loader ────────────────────────────────────────────────────

/// Loads manifests from local JSON files.
#[derive(Debug, Default)]
pub struct JsonManifestLoader;

impl ManifestLoader for JsonManifestLoader {
    fn load(&self, path: &Path) -> Result<Manifest> {
        Manifest::load(path)
    }
}

// ── Render observer ────────────────────────────────────────────────────

/// Default observer that logs to stderr.
#[derive(Debug, Default)]
pub struct StderrObserver;

impl RenderObserver for StderrObserver {
    fn on_render_complete(&self, name: &str, target: &Path) {
        eprintln!("[igata] rendered {name} → {}", target.display());
    }

    fn on_render_error(&self, name: &str, error: &crate::error::Error) {
        eprintln!("[igata] error rendering {name}: {error}");
    }
}

/// Observer that does nothing (for testing or silent operation).
#[derive(Debug, Default)]
pub struct NoopObserver;

impl RenderObserver for NoopObserver {}
