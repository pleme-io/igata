//! Igata (鋳型) — general-purpose template engine for Nix activation-time rendering.
//!
//! Templates use Nix-safe delimiters by default: `[= var =]` for variables,
//! `[% block %]` for logic, `[# comment #]` for comments. This avoids conflicts
//! with Nix's `${}` string interpolation.
//!
//! # Architecture
//!
//! Every I/O boundary is behind a trait. Swap any component:
//!
//! | Trait | Default | Purpose |
//! |-------|---------|---------|
//! | [`TemplateRenderer`] | [`MiniJinjaRenderer`] | Renders template strings |
//! | [`TemplateLoader`] | [`FsTemplateLoader`] | Reads template source files |
//! | [`OutputWriter`] | [`FsOutputWriter`] | Writes rendered output |
//! | [`VariableResolver`] | Literal/File/Env | Resolves a single variable |
//! | [`ContextResolver`] | [`MapContextResolver`] | Resolves a set of variables |
//! | [`ManifestLoader`] | [`JsonManifestLoader`] | Loads the rendering manifest |
//! | [`RenderObserver`] | [`StderrObserver`] | Observes render events |
//!
//! # Quick Start
//!
//! ```
//! use igata::{Engine, Context};
//!
//! let engine = Engine::new();
//! let result = engine.render_str(
//!     "db_url = [= db_url =]\npool = [= pool_size =]",
//!     &Context::from([("db_url", "postgres://localhost/app"), ("pool_size", "10")]),
//! ).unwrap();
//! ```
//!
//! # Custom Components
//!
//! ```
//! use igata::{Engine, traits::OutputWriter, error::Result};
//! use std::path::Path;
//!
//! #[derive(Debug)]
//! struct DryRunWriter;
//!
//! impl OutputWriter for DryRunWriter {
//!     fn write(&self, path: &Path, content: &str, _mode: u32) -> Result<()> {
//!         eprintln!("[dry-run] would write {} bytes to {}", content.len(), path.display());
//!         Ok(())
//!     }
//! }
//!
//! let engine = Engine::builder()
//!     .writer(Box::new(DryRunWriter))
//!     .silent()
//!     .build()
//!     .unwrap();
//! ```

pub mod context;
pub mod defaults;
pub mod engine;
pub mod error;
pub mod manifest;
pub mod syntax;
pub mod traits;

// Core types
pub use context::{Context, ContextBuilder, Source};
pub use engine::{Engine, EngineBuilder, RenderReport};
pub use error::{Error, Result};
pub use manifest::{Manifest, TemplateEntry};
pub use syntax::Syntax;

// Default implementations (for direct use and as reference implementations)
pub use defaults::{
    EnvResolver, FileResolver, FsOutputWriter, FsTemplateLoader, JsonManifestLoader,
    LiteralResolver, MapContextResolver, MiniJinjaRenderer, NoopObserver, StderrObserver,
};
