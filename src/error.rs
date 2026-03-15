use std::path::PathBuf;

/// All errors that can occur during template operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("template error: {0}")]
    Template(#[from] minijinja::Error),

    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write {path}: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse manifest {path}: {source}")]
    ParseManifest {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("environment variable {name} not set")]
    EnvNotSet { name: String },

    #[error("invalid syntax config: {0}")]
    Syntax(String),
}

pub type Result<T> = std::result::Result<T, Error>;
