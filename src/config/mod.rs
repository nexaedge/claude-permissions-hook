pub(crate) mod document;
pub(crate) mod normalize;
pub(crate) mod parse;

use std::path::Path;

use crate::domain::rule::bash::BashRule;
use crate::domain::rule::files::FileRule;
use crate::domain::Environment;
use crate::error::ConfigError;

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

/// Parse KDL config content into a Config.
///
/// CLI reads the file from disk; this function only parses the string content.
/// Returns `Err(ConfigError)` if the content contains invalid KDL.
/// Missing or empty sections produce an empty `Vec` in the resulting Config.
///
/// The `_env` parameter is reserved for future use (home directory expansion).
/// Currently home expansion uses `std::env` directly; a future step will
/// thread `Environment` through the normalize layer.
///
/// # Examples
///
/// ```
/// use claude_permissions_hook::config::parse_policy;
/// use claude_permissions_hook::domain::Environment;
/// use std::path::PathBuf;
///
/// let env = Environment { home: PathBuf::from("/home/user"), cwd: PathBuf::from("/tmp") };
/// let config = parse_policy(r#"bash { allow "git" }"#, &env).unwrap();
/// ```
pub fn parse_policy(content: &str, _env: &Environment) -> Result<Config, ConfigError> {
    Config::parse(content)
}

impl Config {
    /// Load config from a file path.
    ///
    /// I/O errors (file not found, permission denied) propagate as `io::Error`.
    /// KDL parse errors propagate as `ConfigError::InvalidSyntax`.
    ///
    /// Kept temporarily — will be removed in Step 5 when CLI takes over file I/O.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use claude_permissions_hook::config::Config;
    ///
    /// let config = Config::load(Path::new("/path/to/config.kdl")).unwrap();
    /// ```
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::parse(&content)?)
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
