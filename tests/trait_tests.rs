use igata::defaults::{LiteralResolver, MapContextResolver};
use igata::error::Result;
use igata::traits::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ── Mock implementations ───────────────────────────────────────────────

/// Mock variable resolver that returns a fixed value.
#[derive(Debug)]
struct MockResolver(String);

impl VariableResolver for MockResolver {
    fn resolve(&self) -> Result<String> {
        Ok(self.0.clone())
    }
    fn describe(&self) -> String {
        format!("mock:{}", self.0)
    }
}

/// Mock resolver that always fails.
#[derive(Debug)]
struct FailingResolver(String);

impl VariableResolver for FailingResolver {
    fn resolve(&self) -> Result<String> {
        Err(igata::Error::EnvNotSet {
            name: self.0.clone(),
        })
    }
    fn describe(&self) -> String {
        "failing".into()
    }
}

/// Mock template loader that returns content from a map.
#[derive(Debug)]
struct InMemoryLoader {
    templates: BTreeMap<PathBuf, String>,
}

impl InMemoryLoader {
    fn new() -> Self {
        Self {
            templates: BTreeMap::new(),
        }
    }
    fn add(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.templates.insert(path.into(), content.into());
    }
}

impl TemplateLoader for InMemoryLoader {
    fn load(&self, path: &Path) -> Result<String> {
        self.templates
            .get(path)
            .cloned()
            .ok_or_else(|| igata::Error::ReadFile {
                path: path.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "not in memory"),
            })
    }
}

/// Mock writer that captures writes instead of touching the filesystem.
#[derive(Debug, Default, Clone)]
struct CaptureWriter {
    writes: Arc<Mutex<Vec<(PathBuf, String, u32)>>>,
}

impl OutputWriter for CaptureWriter {
    fn write(&self, path: &Path, content: &str, mode: u32) -> Result<()> {
        self.writes
            .lock()
            .unwrap()
            .push((path.to_path_buf(), content.to_owned(), mode));
        Ok(())
    }
}

/// Mock observer that records events.
#[derive(Debug, Default, Clone)]
struct RecordingObserver {
    events: Arc<Mutex<Vec<String>>>,
}

impl RenderObserver for RecordingObserver {
    fn on_render_start(&self, name: &str, _source: &Path, _target: &Path) {
        self.events
            .lock()
            .unwrap()
            .push(format!("start:{name}"));
    }
    fn on_render_complete(&self, name: &str, _target: &Path) {
        self.events
            .lock()
            .unwrap()
            .push(format!("complete:{name}"));
    }
    fn on_render_error(&self, name: &str, error: &igata::Error) {
        self.events
            .lock()
            .unwrap()
            .push(format!("error:{name}:{error}"));
    }
}

/// Mock renderer that wraps output in markers (proves renderer is swappable).
#[derive(Debug)]
struct MarkerRenderer;

impl TemplateRenderer for MarkerRenderer {
    fn render(&self, template: &str, variables: &BTreeMap<String, String>) -> Result<String> {
        let mut result = template.to_string();
        for (k, v) in variables {
            result = result.replace(&format!("[= {k} =]"), v);
        }
        Ok(format!("<<RENDERED>>{result}<</RENDERED>>"))
    }
}

// ── Variable resolver tests ────────────────────────────────────────────

#[test]
fn mock_resolver_returns_value() {
    let r = MockResolver("hello".into());
    assert_eq!(r.resolve().unwrap(), "hello");
    assert_eq!(r.describe(), "mock:hello");
}

#[test]
fn failing_resolver_errors() {
    let r = FailingResolver("oops".into());
    assert!(r.resolve().is_err());
}

#[test]
fn literal_resolver_implements_trait() {
    let r = LiteralResolver {
        value: "test".into(),
    };
    assert_eq!(r.resolve().unwrap(), "test");
    assert_eq!(r.describe(), "literal");
}

// ── Context resolver tests ─────────────────────────────────────────────

#[test]
fn map_context_resolver_resolves_all() {
    let mut ctx = MapContextResolver::new();
    ctx.insert("a", Box::new(MockResolver("1".into())));
    ctx.insert("b", Box::new(MockResolver("2".into())));

    let resolved = ctx.resolve_all().unwrap();
    assert_eq!(resolved["a"], "1");
    assert_eq!(resolved["b"], "2");
}

#[test]
fn map_context_resolver_propagates_errors() {
    let mut ctx = MapContextResolver::new();
    ctx.insert("good", Box::new(MockResolver("ok".into())));
    ctx.insert("bad", Box::new(FailingResolver("fail".into())));

    assert!(ctx.resolve_all().is_err());
}

// ── Template loader tests ──────────────────────────────────────────────

#[test]
fn in_memory_loader_returns_content() {
    let mut loader = InMemoryLoader::new();
    loader.add("/templates/app.conf", "host = [= host =]");

    let content = loader.load(Path::new("/templates/app.conf")).unwrap();
    assert_eq!(content, "host = [= host =]");
}

#[test]
fn in_memory_loader_missing_returns_error() {
    let loader = InMemoryLoader::new();
    assert!(loader.load(Path::new("/nonexistent")).is_err());
}

// ── Output writer tests ────────────────────────────────────────────────

#[test]
fn capture_writer_records_writes() {
    let writer = CaptureWriter::default();
    writer
        .write(Path::new("/out/app.conf"), "rendered content", 0o600)
        .unwrap();

    let writes = writer.writes.lock().unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].0, PathBuf::from("/out/app.conf"));
    assert_eq!(writes[0].1, "rendered content");
    assert_eq!(writes[0].2, 0o600);
}

// ── Engine with injected mocks ─────────────────────────────────────────

#[test]
fn engine_with_mock_writer_no_filesystem() {
    let writer = CaptureWriter::default();
    let engine = igata::Engine::builder()
        .writer(Box::new(writer.clone()))
        .silent()
        .build()
        .unwrap();

    let ctx = igata::Context::from([("val", "42")]);
    let mut loader = InMemoryLoader::new();
    loader.add("/tmpl", "x = [= val =]");

    // Use render_str (doesn't need loader/writer) to verify renderer works.
    let result = engine.render_str("x = [= val =]", &ctx).unwrap();
    assert_eq!(result, "x = 42");
}

#[test]
fn engine_with_custom_renderer() {
    let engine = igata::Engine::builder()
        .renderer(Box::new(MarkerRenderer))
        .silent()
        .build()
        .unwrap();

    let ctx = igata::Context::from([("name", "test")]);
    let result = engine.render_str("hello [= name =]", &ctx).unwrap();
    assert_eq!(result, "<<RENDERED>>hello test<</RENDERED>>");
}

#[test]
fn engine_observer_receives_events() {
    use igata::manifest::{Manifest, TemplateEntry};

    let observer = RecordingObserver::default();
    let writer = CaptureWriter::default();

    let mut loader = InMemoryLoader::new();
    loader.add("/src/tmpl", "val = [= x =]");

    let engine = igata::Engine::builder()
        .loader(Box::new(loader))
        .writer(Box::new(writer.clone()))
        .observer(Box::new(observer.clone()))
        .build()
        .unwrap();

    let manifest = Manifest {
        syntax: igata::Syntax::default(),
        templates: BTreeMap::from([(
            "test-entry".into(),
            TemplateEntry {
                source: PathBuf::from("/src/tmpl"),
                target: PathBuf::from("/out/result"),
                mode: "0644".into(),
                owner: String::new(),
                group: String::new(),
                context: igata::Context::from([("x", "hello")]),
            },
        )]),
    };

    let report = engine.render_manifest(&manifest).unwrap();
    assert_eq!(report.rendered, vec!["test-entry"]);

    let events = observer.events.lock().unwrap();
    assert_eq!(events[0], "start:test-entry");
    assert_eq!(events[1], "complete:test-entry");

    let writes = writer.writes.lock().unwrap();
    assert_eq!(writes[0].1, "val = hello");
    assert_eq!(writes[0].2, 0o644);
}

// ── Composition patterns ───────────────────────────────────────────────

#[test]
fn custom_variable_resolver_integration() {
    /// A "vault" resolver that simulates fetching from a secret store.
    #[derive(Debug)]
    struct VaultResolver {
        secret_path: String,
        // In reality, this would hold a vault client.
        mock_secrets: BTreeMap<String, String>,
    }

    impl VariableResolver for VaultResolver {
        fn resolve(&self) -> Result<String> {
            self.mock_secrets
                .get(&self.secret_path)
                .cloned()
                .ok_or_else(|| igata::Error::EnvNotSet {
                    name: format!("vault:{}", self.secret_path),
                })
        }
        fn describe(&self) -> String {
            format!("vault:{}", self.secret_path)
        }
    }

    let mock_vault: BTreeMap<String, String> = BTreeMap::from([
        ("/pleme/db-password".into(), "s3cr3t".into()),
        ("/pleme/api-key".into(), "ak-12345".into()),
    ]);

    let mut ctx = MapContextResolver::new();
    ctx.insert(
        "db_password",
        Box::new(VaultResolver {
            secret_path: "/pleme/db-password".into(),
            mock_secrets: mock_vault.clone(),
        }),
    );
    ctx.insert(
        "api_key",
        Box::new(VaultResolver {
            secret_path: "/pleme/api-key".into(),
            mock_secrets: mock_vault,
        }),
    );

    let resolved = ctx.resolve_all().unwrap();
    assert_eq!(resolved["db_password"], "s3cr3t");
    assert_eq!(resolved["api_key"], "ak-12345");
}

#[test]
fn dry_run_writer() {
    /// Writer that validates but doesn't write.
    #[derive(Debug)]
    struct DryRunWriter {
        log: Arc<Mutex<Vec<String>>>,
    }

    impl OutputWriter for DryRunWriter {
        fn write(&self, path: &Path, content: &str, mode: u32) -> Result<()> {
            self.log.lock().unwrap().push(format!(
                "would write {} bytes to {} (mode {:o})",
                content.len(),
                path.display(),
                mode
            ));
            Ok(())
        }
    }

    let log = Arc::new(Mutex::new(Vec::new()));
    let writer = DryRunWriter { log: log.clone() };

    let engine = igata::Engine::builder()
        .writer(Box::new(writer))
        .silent()
        .build()
        .unwrap();

    let result = engine
        .render_str("output = [= x =]", &igata::Context::from([("x", "1")]))
        .unwrap();
    assert_eq!(result, "output = 1");

    // Verify dry-run writer was not invoked for render_str (no file output).
    assert!(log.lock().unwrap().is_empty());
}
