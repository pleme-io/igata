use crate::context::{Context, Source};
use crate::defaults::{
    EnvResolver, FileResolver, FsOutputWriter, FsTemplateLoader, LiteralResolver,
    MapContextResolver, MiniJinjaRenderer, NoopObserver, StderrObserver,
};
use crate::error::Result;
use crate::manifest::Manifest;
use crate::syntax::Syntax;
use crate::traits::{
    ContextResolver, OutputWriter, RenderObserver, TemplateLoader, TemplateRenderer,
};
use std::collections::BTreeMap;
use std::path::Path;

/// The igata template engine — fully trait-composed.
///
/// Every I/O boundary is behind a trait: rendering, loading, writing,
/// observing. Swap any component for testing or alternative backends.
///
/// # Examples
///
/// ```
/// use igata::{Engine, Context};
///
/// let engine = Engine::new();
/// let result = engine.render_str(
///     "Hello [= name =]!",
///     &Context::from([("name", "world")]),
/// ).unwrap();
/// assert_eq!(result, "Hello world!");
/// ```
pub struct Engine {
    renderer: Box<dyn TemplateRenderer>,
    loader: Box<dyn TemplateLoader>,
    writer: Box<dyn OutputWriter>,
    observer: Box<dyn RenderObserver>,
}

// Manual Debug since we compose trait objects.
impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("renderer", &self.renderer)
            .field("loader", &self.loader)
            .field("writer", &self.writer)
            .field("observer", &self.observer)
            .finish()
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Create an engine with all default implementations.
    #[must_use]
    pub fn new() -> Self {
        Self {
            renderer: Box::new(MiniJinjaRenderer::default()),
            loader: Box::new(FsTemplateLoader),
            writer: Box::new(FsOutputWriter),
            observer: Box::new(StderrObserver),
        }
    }

    /// Create an engine with custom syntax.
    pub fn with_syntax(syntax: Syntax) -> Result<Self> {
        syntax.to_config()?; // validate eagerly
        Ok(Self {
            renderer: Box::new(MiniJinjaRenderer::new(syntax)),
            loader: Box::new(FsTemplateLoader),
            writer: Box::new(FsOutputWriter),
            observer: Box::new(StderrObserver),
        })
    }

    /// Start building an engine with custom components.
    #[must_use]
    pub fn builder() -> EngineBuilder {
        EngineBuilder::default()
    }

    /// Render a template string with the given context.
    pub fn render_str(&self, template: &str, ctx: &Context) -> Result<String> {
        let resolved = Self::resolve_context(ctx)?;
        self.renderer.render(template, &resolved)
    }

    /// Render a template file, returning the rendered string.
    pub fn render_file(&self, template_path: &Path, ctx: &Context) -> Result<String> {
        let content = self.loader.load(template_path)?;
        let resolved = Self::resolve_context(ctx)?;
        self.renderer.render(&content, &resolved)
    }

    /// Render a template file and write the output to a target path.
    pub fn render_to_file(
        &self,
        template_path: &Path,
        target_path: &Path,
        ctx: &Context,
        mode: u32,
    ) -> Result<()> {
        let rendered = self.render_file(template_path, ctx)?;
        self.writer.write(target_path, &rendered, mode)
    }

    /// Render all templates in a manifest.
    ///
    /// Note: this intentionally uses the manifest's declared syntax (not the
    /// engine's renderer) because manifest-driven rendering is self-describing
    /// — the manifest specifies which delimiters its templates use.
    pub fn render_manifest(&self, manifest: &Manifest) -> Result<RenderReport> {
        // Build a renderer with the manifest's syntax — intentionally overrides
        // the engine's default renderer so manifests are self-describing.
        let renderer: Box<dyn TemplateRenderer> =
            Box::new(MiniJinjaRenderer::new(manifest.syntax.clone()));

        let mut report = RenderReport::default();

        for (name, entry) in &manifest.templates {
            self.observer
                .on_render_start(name, &entry.source, &entry.target);

            match self.render_manifest_entry(&*renderer, name, entry) {
                Ok(()) => {
                    self.observer.on_render_complete(name, &entry.target);
                    report.rendered.push(name.clone());
                }
                Err(e) => {
                    self.observer.on_render_error(name, &e);
                    return Err(e);
                }
            }
        }

        Ok(report)
    }

    fn render_manifest_entry(
        &self,
        renderer: &dyn TemplateRenderer,
        name: &str,
        entry: &crate::manifest::TemplateEntry,
    ) -> Result<()> {
        let content = self.loader.load(&entry.source).map_err(|e| {
            eprintln!("[igata] failed to load template '{name}': {e}");
            e
        })?;
        let ctx_resolver = Self::build_context_resolver(&entry.context);
        let variables = ctx_resolver.resolve_all().map_err(|e| {
            eprintln!("[igata] failed to resolve context for '{name}': {e}");
            e
        })?;
        let rendered = renderer.render(&content, &variables)?;
        let mode = parse_mode(&entry.mode);
        self.writer.write(&entry.target, &rendered, mode)
    }

    fn resolve_context(ctx: &Context) -> Result<BTreeMap<String, String>> {
        Self::build_context_resolver(ctx).resolve_all()
    }

    fn build_context_resolver(ctx: &Context) -> MapContextResolver {
        let mut resolver = MapContextResolver::new();
        for (name, source) in &ctx.variables {
            let var_resolver: Box<dyn crate::traits::VariableResolver> = match source {
                Source::Literal { value } => Box::new(LiteralResolver {
                    value: value.clone(),
                }),
                Source::File { path } => Box::new(FileResolver {
                    path: path.clone(),
                }),
                Source::Env { name } => Box::new(EnvResolver {
                    name: name.clone(),
                }),
            };
            resolver.insert(name, var_resolver);
        }
        resolver
    }
}

/// Fluent builder for [`Engine`] — inject any combination of trait implementations.
pub struct EngineBuilder {
    syntax: Option<Syntax>,
    renderer: Option<Box<dyn TemplateRenderer>>,
    loader: Option<Box<dyn TemplateLoader>>,
    writer: Option<Box<dyn OutputWriter>>,
    observer: Option<Box<dyn RenderObserver>>,
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self {
            syntax: None,
            renderer: None,
            loader: None,
            writer: None,
            observer: None,
        }
    }
}

impl EngineBuilder {
    /// Set custom variable delimiters.
    #[must_use]
    pub fn variable_delimiters(mut self, open: &str, close: &str) -> Self {
        let syntax = self.syntax.get_or_insert_with(Syntax::default);
        syntax.variable = (open.into(), close.into());
        self
    }

    /// Set custom block delimiters.
    #[must_use]
    pub fn block_delimiters(mut self, open: &str, close: &str) -> Self {
        let syntax = self.syntax.get_or_insert_with(Syntax::default);
        syntax.block = (open.into(), close.into());
        self
    }

    /// Set custom comment delimiters.
    #[must_use]
    pub fn comment_delimiters(mut self, open: &str, close: &str) -> Self {
        let syntax = self.syntax.get_or_insert_with(Syntax::default);
        syntax.comment = (open.into(), close.into());
        self
    }

    /// Inject a custom template renderer.
    #[must_use]
    pub fn renderer(mut self, renderer: Box<dyn TemplateRenderer>) -> Self {
        self.renderer = Some(renderer);
        self
    }

    /// Inject a custom template loader.
    #[must_use]
    pub fn loader(mut self, loader: Box<dyn TemplateLoader>) -> Self {
        self.loader = Some(loader);
        self
    }

    /// Inject a custom output writer.
    #[must_use]
    pub fn writer(mut self, writer: Box<dyn OutputWriter>) -> Self {
        self.writer = Some(writer);
        self
    }

    /// Inject a custom render observer.
    #[must_use]
    pub fn observer(mut self, observer: Box<dyn RenderObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// Disable all output (no stderr logging).
    #[must_use]
    pub fn silent(mut self) -> Self {
        self.observer = Some(Box::new(NoopObserver));
        self
    }

    /// Build the engine.
    pub fn build(self) -> Result<Engine> {
        let syntax = self.syntax.unwrap_or_default();
        let renderer = self
            .renderer
            .unwrap_or_else(|| Box::new(MiniJinjaRenderer::new(syntax)));
        let loader = self
            .loader
            .unwrap_or_else(|| Box::new(FsTemplateLoader));
        let writer = self
            .writer
            .unwrap_or_else(|| Box::new(FsOutputWriter));
        let observer = self
            .observer
            .unwrap_or_else(|| Box::new(StderrObserver));

        Ok(Engine {
            renderer,
            loader,
            writer,
            observer,
        })
    }
}

/// Report of a manifest render operation.
#[derive(Debug, Default)]
pub struct RenderReport {
    /// Names of successfully rendered templates.
    pub rendered: Vec<String>,
}

fn parse_mode(mode: &str) -> u32 {
    u32::from_str_radix(mode, 8).unwrap_or(0o600)
}
