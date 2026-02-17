use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::protocol::Decision;

/// Top-level configuration loaded from a KDL file.
#[derive(Debug)]
pub struct Config {
    pub bash: BashConfig,
}

/// Bash-specific configuration: sets of programs to allow, deny, or ask about.
#[derive(Debug, Default)]
pub struct BashConfig {
    pub allow: HashSet<String>,
    pub deny: HashSet<String>,
    pub ask: HashSet<String>,
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
    /// Load a config from a KDL file at the given path.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound(path.to_path_buf())
            } else {
                ConfigError::ReadError(e)
            }
        })?;
        Self::parse(&content)
    }

    /// Parse a KDL string into a Config.
    pub fn parse(content: &str) -> Result<Self, ConfigError> {
        let doc: kdl::KdlDocument = content
            .parse()
            .map_err(|e: kdl::KdlError| ConfigError::ParseError(e.to_string()))?;
        Self::from_document(&doc)
    }

    fn from_document(doc: &kdl::KdlDocument) -> Result<Self, ConfigError> {
        let bash = match doc.get("bash").and_then(|n| n.children()) {
            Some(children) => BashConfig::from_children(children)?,
            None => BashConfig::default(),
        };
        Ok(Config { bash })
    }
}

impl BashConfig {
    fn from_children(children: &kdl::KdlDocument) -> Result<Self, ConfigError> {
        Ok(BashConfig {
            allow: collect_programs(children, "allow"),
            deny: collect_programs(children, "deny"),
            ask: collect_programs(children, "ask"),
        })
    }

    /// Look up a program name and return its configured decision.
    ///
    /// Precedence: deny > ask > allow. Returns `None` for unlisted programs.
    pub fn lookup(&self, program: &str) -> Option<Decision> {
        if self.deny.contains(program) {
            Some(Decision::Deny)
        } else if self.ask.contains(program) {
            Some(Decision::Ask)
        } else if self.allow.contains(program) {
            Some(Decision::Allow)
        } else {
            None
        }
    }
}

/// Collect all string arguments from nodes with the given name.
/// Handles multiple nodes: `allow "git"` + `allow "cargo"` merges into one set.
fn collect_programs(doc: &kdl::KdlDocument, node_name: &str) -> HashSet<String> {
    doc.nodes()
        .iter()
        .filter(|n| n.name().value() == node_name)
        .flat_map(|n| n.entries())
        .filter_map(|e| e.value().as_string().map(String::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // --- KDL Parsing Tests ---

    fn set_of(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_valid_kdl_with_all_sections() {
        let config = Config::parse(
            r#"
            bash {
                allow "git" "cargo" "npm"
                deny "rm" "shutdown"
                ask "docker" "kubectl"
            }
            "#,
        )
        .unwrap();

        assert_eq!(config.bash.allow, set_of(&["git", "cargo", "npm"]));
        assert_eq!(config.bash.deny, set_of(&["rm", "shutdown"]));
        assert_eq!(config.bash.ask, set_of(&["docker", "kubectl"]));
    }

    #[test]
    fn parse_kdl_with_missing_sections() {
        let config = Config::parse(
            r#"
            bash {
                allow "git"
            }
            "#,
        )
        .unwrap();

        assert_eq!(config.bash.allow, set_of(&["git"]));
        assert!(config.bash.deny.is_empty());
        assert!(config.bash.ask.is_empty());
    }

    #[test]
    fn parse_empty_kdl_file() {
        let config = Config::parse("").unwrap();

        assert!(config.bash.allow.is_empty());
        assert!(config.bash.deny.is_empty());
        assert!(config.bash.ask.is_empty());
    }

    #[test]
    fn merge_multiple_allow_nodes() {
        let config = Config::parse(
            r#"
            bash {
                allow "git"
                allow "cargo" "npm"
            }
            "#,
        )
        .unwrap();

        assert_eq!(config.bash.allow, set_of(&["git", "cargo", "npm"]));
    }

    #[test]
    fn invalid_kdl_returns_parse_error() {
        let result = Config::parse("this is { not valid { kdl");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    // --- Lookup Tests ---

    #[test]
    fn lookup_program_in_allow_list() {
        let bash = BashConfig {
            allow: set_of(&["git"]),
            deny: HashSet::new(),
            ask: HashSet::new(),
        };
        assert_eq!(bash.lookup("git"), Some(Decision::Allow));
    }

    #[test]
    fn lookup_program_in_deny_list() {
        let bash = BashConfig {
            allow: HashSet::new(),
            deny: set_of(&["rm"]),
            ask: HashSet::new(),
        };
        assert_eq!(bash.lookup("rm"), Some(Decision::Deny));
    }

    #[test]
    fn lookup_program_in_ask_list() {
        let bash = BashConfig {
            allow: HashSet::new(),
            deny: HashSet::new(),
            ask: set_of(&["docker"]),
        };
        assert_eq!(bash.lookup("docker"), Some(Decision::Ask));
    }

    #[test]
    fn lookup_unlisted_program_returns_none() {
        let bash = BashConfig {
            allow: set_of(&["git"]),
            deny: set_of(&["rm"]),
            ask: set_of(&["docker"]),
        };
        assert_eq!(bash.lookup("python"), None);
    }

    #[test]
    fn lookup_program_in_both_allow_and_deny_returns_deny() {
        let bash = BashConfig {
            allow: set_of(&["rm"]),
            deny: set_of(&["rm"]),
            ask: HashSet::new(),
        };
        assert_eq!(bash.lookup("rm"), Some(Decision::Deny));
    }

    #[test]
    fn lookup_program_in_both_allow_and_ask_returns_ask() {
        let bash = BashConfig {
            allow: set_of(&["docker"]),
            deny: HashSet::new(),
            ask: set_of(&["docker"]),
        };
        assert_eq!(bash.lookup("docker"), Some(Decision::Ask));
    }

    // --- File Loading Tests ---

    #[test]
    fn load_nonexistent_file_returns_not_found() {
        let result = Config::load(Path::new("/tmp/does-not-exist-12345.kdl"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::NotFound(_)));
    }

    #[test]
    fn load_valid_file_from_disk() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(
            tmpfile,
            r#"bash {{
    allow "git" "cargo"
    deny "rm"
}}"#
        )
        .unwrap();

        let config = Config::load(tmpfile.path()).unwrap();
        assert_eq!(config.bash.allow, set_of(&["git", "cargo"]));
        assert_eq!(config.bash.deny, set_of(&["rm"]));
    }

    #[test]
    fn load_file_with_invalid_kdl_returns_parse_error() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "invalid {{ kdl {{ syntax").unwrap();

        let result = Config::load(tmpfile.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }
}
