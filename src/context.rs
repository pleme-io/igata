use crate::error::{Error, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// Source of a template variable value.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// Literal string value.
    Literal { value: String },

    /// Read value from a file (trimming trailing whitespace).
    File { path: String },

    /// Read value from an environment variable.
    Env { name: String },
}

impl Source {
    /// Resolve this source to its concrete string value.
    pub fn resolve(&self) -> Result<String> {
        match self {
            Self::Literal { value } => Ok(value.clone()),
            Self::File { path } => {
                let p = Path::new(path);
                let content = std::fs::read_to_string(p).map_err(|e| Error::ReadFile {
                    path: p.to_path_buf(),
                    source: e,
                })?;
                Ok(content.trim_end().to_owned())
            }
            Self::Env { name } => std::env::var(name).map_err(|_| Error::EnvNotSet {
                name: name.clone(),
            }),
        }
    }
}

/// Template context: a set of named variables resolved from various sources.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct Context {
    /// Variable name → source mapping.
    #[serde(default)]
    pub variables: BTreeMap<String, Source>,
}

impl Context {
    /// Create an empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start building a context.
    #[must_use]
    pub fn builder() -> ContextBuilder {
        ContextBuilder::default()
    }

    /// Resolve all variable sources into concrete string values.
    pub fn resolve(&self) -> Result<BTreeMap<String, String>> {
        self.variables
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.resolve()?)))
            .collect()
    }
}

/// Fluent builder for [`Context`].
#[derive(Debug, Default)]
pub struct ContextBuilder {
    variables: BTreeMap<String, Source>,
}

impl ContextBuilder {
    /// Add a literal string variable.
    #[must_use]
    pub fn literal(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(
            name.into(),
            Source::Literal {
                value: value.into(),
            },
        );
        self
    }

    /// Add a variable read from a file at render time.
    #[must_use]
    pub fn file(mut self, name: impl Into<String>, path: impl Into<String>) -> Self {
        self.variables.insert(
            name.into(),
            Source::File {
                path: path.into(),
            },
        );
        self
    }

    /// Add a variable read from an environment variable.
    #[must_use]
    pub fn env(mut self, name: impl Into<String>, env_name: impl Into<String>) -> Self {
        self.variables.insert(
            name.into(),
            Source::Env {
                name: env_name.into(),
            },
        );
        self
    }

    /// Build the context.
    #[must_use]
    pub fn build(self) -> Context {
        Context {
            variables: self.variables,
        }
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Context {
    fn from(pairs: [(&str, &str); N]) -> Self {
        let variables = pairs
            .into_iter()
            .map(|(k, v)| {
                (
                    k.to_owned(),
                    Source::Literal {
                        value: v.to_owned(),
                    },
                )
            })
            .collect();
        Self { variables }
    }
}
