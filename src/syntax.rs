use crate::error::{Error, Result};
use minijinja::syntax::SyntaxConfig;

/// Delimiter configuration for templates.
///
/// Defaults to `[= =]` for variables, `[% %]` for blocks, `[# #]` for comments.
/// These defaults avoid conflicts with Nix `${}` interpolation, shell `$VAR`,
/// and common config file formats.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Syntax {
    /// Variable delimiters, e.g. `["[=", "=]"]`.
    #[serde(default = "Syntax::default_variable")]
    pub variable: (String, String),

    /// Block delimiters, e.g. `["[%", "%]"]`.
    #[serde(default = "Syntax::default_block")]
    pub block: (String, String),

    /// Comment delimiters, e.g. `["[#", "#]"]`.
    #[serde(default = "Syntax::default_comment")]
    pub comment: (String, String),
}

impl Default for Syntax {
    fn default() -> Self {
        Self {
            variable: Self::default_variable(),
            block: Self::default_block(),
            comment: Self::default_comment(),
        }
    }
}

impl Syntax {
    fn default_variable() -> (String, String) {
        ("[=".into(), "=]".into())
    }

    fn default_block() -> (String, String) {
        ("[%".into(), "%]".into())
    }

    fn default_comment() -> (String, String) {
        ("[#".into(), "#]".into())
    }

    /// Convert to a MiniJinja `SyntaxConfig`.
    ///
    /// MiniJinja's builder requires `&'static str` for delimiters, so we leak
    /// the strings. This is fine — syntax configs are created once per engine
    /// and live for the program's duration.
    pub fn to_config(&self) -> Result<SyntaxConfig> {
        let var_open: &'static str = self.variable.0.clone().leak();
        let var_close: &'static str = self.variable.1.clone().leak();
        let blk_open: &'static str = self.block.0.clone().leak();
        let blk_close: &'static str = self.block.1.clone().leak();
        let cmt_open: &'static str = self.comment.0.clone().leak();
        let cmt_close: &'static str = self.comment.1.clone().leak();

        SyntaxConfig::builder()
            .variable_delimiters(var_open, var_close)
            .block_delimiters(blk_open, blk_close)
            .comment_delimiters(cmt_open, cmt_close)
            .build()
            .map_err(|e| Error::Syntax(e.to_string()))
    }
}
