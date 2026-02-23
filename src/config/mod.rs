pub(crate) mod document;
pub(crate) mod normalize;
pub(crate) mod parse;

use std::path::{Path, PathBuf};

use crate::domain::rule::bash::BashRule;
use crate::domain::rule::files::FileRule;

use document::ConfigDocument;

/// Top-level configuration — facade for the rest of the codebase.
///
/// Other modules access tool configs through this struct without
/// needing to know about parsing, KDL, or tool-specific modules.
///
/// Fields use `Vec` (not `Option<Vec>`) — an absent config section
/// is represented as an empty vec.
#[derive(Debug, Default)]
pub struct Config {
    pub(crate) bash: Vec<BashRule>,
    pub(crate) files: Vec<FileRule>,
    /// Whether the bash section was present in the config file.
    pub(crate) has_bash: bool,
    /// Whether the files section was present in the config file.
    pub(crate) has_files: bool,
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
        let bash = parse::bash::parse_bash(&config_nodes)?;
        let files = parse::files::parse_files(&config_nodes)?;
        Ok(Config {
            has_bash: bash.is_some(),
            has_files: files.is_some(),
            bash: bash.unwrap_or_default(),
            files: files.unwrap_or_default(),
        })
    }
}
