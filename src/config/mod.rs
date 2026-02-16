use std::path::{Path, PathBuf};

use crate::protocol::Decision;

/// Top-level configuration loaded from a KDL file.
#[derive(Debug)]
pub struct Config {
    pub bash: BashConfig,
}

/// Bash-specific configuration: lists of programs to allow, deny, or ask about.
#[derive(Debug, Default)]
pub struct BashConfig {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub ask: Vec<String>,
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
    #[error("invalid config: {0}")]
    ValidationError(String),
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
        if self.deny.iter().any(|p| p == program) {
            Some(Decision::Deny)
        } else if self.ask.iter().any(|p| p == program) {
            Some(Decision::Ask)
        } else if self.allow.iter().any(|p| p == program) {
            Some(Decision::Allow)
        } else {
            None
        }
    }
}

/// Collect all string arguments from nodes with the given name.
/// Handles multiple nodes: `allow "git"` + `allow "cargo"` merges into one list.
fn collect_programs(doc: &kdl::KdlDocument, node_name: &str) -> Vec<String> {
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

        assert_eq!(config.bash.allow, vec!["git", "cargo", "npm"]);
        assert_eq!(config.bash.deny, vec!["rm", "shutdown"]);
        assert_eq!(config.bash.ask, vec!["docker", "kubectl"]);
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

        assert_eq!(config.bash.allow, vec!["git"]);
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

        assert_eq!(config.bash.allow, vec!["git", "cargo", "npm"]);
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
            allow: vec!["git".into()],
            deny: vec![],
            ask: vec![],
        };
        assert_eq!(bash.lookup("git"), Some(Decision::Allow));
    }

    #[test]
    fn lookup_program_in_deny_list() {
        let bash = BashConfig {
            allow: vec![],
            deny: vec!["rm".into()],
            ask: vec![],
        };
        assert_eq!(bash.lookup("rm"), Some(Decision::Deny));
    }

    #[test]
    fn lookup_program_in_ask_list() {
        let bash = BashConfig {
            allow: vec![],
            deny: vec![],
            ask: vec!["docker".into()],
        };
        assert_eq!(bash.lookup("docker"), Some(Decision::Ask));
    }

    #[test]
    fn lookup_unlisted_program_returns_none() {
        let bash = BashConfig {
            allow: vec!["git".into()],
            deny: vec!["rm".into()],
            ask: vec!["docker".into()],
        };
        assert_eq!(bash.lookup("python"), None);
    }

    #[test]
    fn lookup_program_in_both_allow_and_deny_returns_deny() {
        let bash = BashConfig {
            allow: vec!["rm".into()],
            deny: vec!["rm".into()],
            ask: vec![],
        };
        assert_eq!(bash.lookup("rm"), Some(Decision::Deny));
    }

    #[test]
    fn lookup_program_in_both_allow_and_ask_returns_ask() {
        let bash = BashConfig {
            allow: vec!["docker".into()],
            deny: vec![],
            ask: vec!["docker".into()],
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
        assert_eq!(config.bash.allow, vec!["git", "cargo"]);
        assert_eq!(config.bash.deny, vec!["rm"]);
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
