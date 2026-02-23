pub(crate) mod document;
pub(crate) mod normalize;
pub(crate) mod parse;

use std::path::{Path, PathBuf};

use crate::domain::rule::bash::BashConfig;
use crate::domain::rule::files::FilesConfig;

use document::ConfigDocument;

/// Top-level configuration — facade for the rest of the codebase.
///
/// Other modules access tool configs through this struct without
/// needing to know about parsing, KDL, or tool-specific modules.
#[derive(Debug, Default)]
pub struct Config {
    pub(crate) bash: Option<BashConfig>,
    pub(crate) files: Option<FilesConfig>,
}

/// Errors that can occur when loading or parsing a config file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(PathBuf),
    #[error("failed to read config: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("invalid KDL syntax: {0}")]
    ParseError(String),
}

impl Config {
    /// Load config from a file path.
    ///
    /// Returns `ConfigError::NotFound` if the file does not exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use claude_permissions_hook::config::Config;
    ///
    /// let config = Config::load(Path::new("/path/to/config.kdl")).unwrap();
    /// ```
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let doc = ConfigDocument::load(path)?;
        Self::from_document(&doc)
    }

    /// Parse config from a KDL string.
    ///
    /// # Examples
    ///
    /// ```
    /// use claude_permissions_hook::config::Config;
    ///
    /// let config = Config::parse(r#"bash { allow "git" }"#).unwrap();
    /// ```
    pub fn parse(content: &str) -> Result<Self, ConfigError> {
        let doc = ConfigDocument::parse(content)?;
        Self::from_document(&doc)
    }

    fn from_document(doc: &ConfigDocument) -> Result<Self, ConfigError> {
        let config_nodes = parse::section_to_config_nodes(doc);
        Ok(Config {
            bash: parse::bash::parse_bash(&config_nodes)?,
            files: parse::files::parse_files(&config_nodes)?,
        })
    }
}
