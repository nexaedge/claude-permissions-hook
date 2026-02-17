pub(crate) mod bash;
mod kdl;
pub mod rule;
pub(crate) mod section;

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use bash::BashConfig;

use kdl::KdlParse;
use section::ToolConfig;

type ToolParser = Box<dyn Fn(&KdlParse) -> Result<(TypeId, Box<dyn Any>), ConfigError>>;

/// Top-level configuration holding registered tool configs.
///
/// Built via [`ConfigBuilder`]. Access individual tool configs with [`tool`](Self::tool).
pub struct Config {
    tools: HashMap<TypeId, Box<dyn Any>>,
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
    /// Create a new builder for registering tool parsers.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder {
            parsers: Vec::new(),
            prebuilt: HashMap::new(),
        }
    }

    /// Get a registered tool's config by type.
    ///
    /// Returns `None` if the tool was not registered or its section was absent.
    pub(crate) fn tool<T: ToolConfig>(&self) -> Option<&T> {
        self.tools
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref())
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("registered_tools", &self.tools.len())
            .finish()
    }
}

/// Builder for constructing a [`Config`] with registered tool parsers.
///
/// Two modes of construction:
/// - **Parsing**: `register::<T>()` then `parse()`/`load()` — parses tool sections from KDL
/// - **Direct**: `set(tool_config)` then `build()` — inserts pre-built configs (useful for tests)
pub struct ConfigBuilder {
    parsers: Vec<ToolParser>,
    prebuilt: HashMap<TypeId, Box<dyn Any>>,
}

impl ConfigBuilder {
    /// Register a tool to be parsed from its KDL section during `parse()`/`load()`.
    pub(crate) fn register<T: ToolConfig>(mut self) -> Self {
        self.parsers.push(Box::new(|kdl| {
            let config: T = section::parse_tool(kdl)?;
            Ok((TypeId::of::<T>(), Box::new(config)))
        }));
        self
    }

    /// Insert a pre-built tool config directly (skips KDL parsing for this tool).
    #[cfg(test)]
    pub(crate) fn set<T: ToolConfig>(mut self, config: T) -> Self {
        self.prebuilt.insert(TypeId::of::<T>(), Box::new(config));
        self
    }

    /// Parse a KDL string, running all registered parsers.
    pub fn parse(self, content: &str) -> Result<Config, ConfigError> {
        let kdl = KdlParse::parse(content)?;

        let mut tools = self.prebuilt;
        for parser in &self.parsers {
            let (type_id, config) = parser(&kdl)?;
            tools.insert(type_id, config);
        }
        Ok(Config { tools })
    }

    /// Load and parse a KDL file, running all registered parsers.
    pub fn load(self, path: &Path) -> Result<Config, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound(path.to_path_buf())
            } else {
                ConfigError::ReadError(e)
            }
        })?;
        self.parse(&content)
    }

    /// Build a Config from pre-built tools only (no KDL parsing).
    #[cfg(test)]
    pub(crate) fn build(self) -> Config {
        Config {
            tools: self.prebuilt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandSegment;
    use crate::protocol::Decision;
    use std::collections::HashSet;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // --- Helpers ---

    fn set_of(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn rules_of(programs: &[&str]) -> Vec<rule::BashRule> {
        programs
            .iter()
            .map(|p| rule::BashRule {
                program: p.to_string(),
                conditions: rule::RuleConditions::default(),
            })
            .collect()
    }

    fn seg(program: &str) -> CommandSegment {
        CommandSegment {
            program: program.to_string(),
            args: vec![],
        }
    }

    fn program_names(rules: &[rule::BashRule]) -> Vec<&str> {
        rules.iter().map(|r| r.program.as_str()).collect()
    }

    /// Parse KDL with bash tool registered.
    fn parse_config(content: &str) -> Result<Config, ConfigError> {
        Config::builder().register::<BashConfig>().parse(content)
    }

    /// Shorthand to get BashConfig from a Config.
    fn bash(config: &Config) -> &BashConfig {
        config.tool::<BashConfig>().expect("bash not registered")
    }

    /// Parse raw KDL and collect bash rules for a single tier.
    fn rules_from_kdl(source: &str, tier: &str) -> Vec<rule::BashRule> {
        let ts = section::parse_from_source(source).unwrap();
        let entries = match tier {
            "allow" => ts.allow,
            "deny" => ts.deny,
            "ask" => ts.ask,
            _ => panic!("unknown tier: {tier}"),
        };
        bash::parse_rules(entries).unwrap()
    }

    /// Parse raw KDL, attempt to collect bash rules, return the error string.
    fn rules_err(source: &str, tier: &str) -> String {
        let ts = match section::parse_from_source(source) {
            Err(e) => return e.to_string(),
            Ok(ts) => ts,
        };
        let entries = match tier {
            "allow" => ts.allow,
            "deny" => ts.deny,
            "ask" => ts.ask,
            _ => panic!("unknown tier: {tier}"),
        };
        bash::parse_rules(entries).unwrap_err().to_string()
    }

    // --- Config-level parsing tests ---

    #[test]
    fn parse_valid_kdl_with_all_sections() {
        let config = parse_config(
            r#"
            bash {
                allow "git" "cargo" "npm"
                deny "rm" "shutdown"
                ask "docker" "kubectl"
            }
            "#,
        )
        .unwrap();

        let b = bash(&config);
        let mut allow_names: Vec<&str> = program_names(&b.allow);
        allow_names.sort();
        assert_eq!(allow_names, vec!["cargo", "git", "npm"]);
        let mut deny_names: Vec<&str> = program_names(&b.deny);
        deny_names.sort();
        assert_eq!(deny_names, vec!["rm", "shutdown"]);
        let mut ask_names: Vec<&str> = program_names(&b.ask);
        ask_names.sort();
        assert_eq!(ask_names, vec!["docker", "kubectl"]);
    }

    #[test]
    fn parse_kdl_with_missing_sections() {
        let config = parse_config(
            r#"
            bash {
                allow "git"
            }
            "#,
        )
        .unwrap();

        let b = bash(&config);
        assert_eq!(program_names(&b.allow), vec!["git"]);
        assert!(b.deny.is_empty());
        assert!(b.ask.is_empty());
    }

    #[test]
    fn parse_empty_kdl_file() {
        let config = parse_config("").unwrap();
        let b = bash(&config);
        assert!(b.allow.is_empty());
        assert!(b.deny.is_empty());
        assert!(b.ask.is_empty());
    }

    #[test]
    fn merge_multiple_allow_nodes() {
        let config = parse_config(
            r#"
            bash {
                allow "git"
                allow "cargo" "npm"
            }
            "#,
        )
        .unwrap();
        assert_eq!(
            program_names(&bash(&config).allow),
            vec!["git", "cargo", "npm"]
        );
    }

    #[test]
    fn invalid_kdl_returns_parse_error() {
        let result = parse_config("this is { not valid { kdl");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    #[test]
    fn unregistered_tool_returns_none() {
        let config = Config::builder().build();
        assert!(config.tool::<BashConfig>().is_none());
    }

    // --- Lookup tests ---

    #[test]
    fn lookup_program_in_allow_list() {
        let config = Config::builder()
            .set(BashConfig {
                allow: rules_of(&["git"]),
                ..Default::default()
            })
            .build();
        assert_eq!(
            bash(&config).lookup(&seg("git")),
            Some(Decision::Allow)
        );
    }

    #[test]
    fn lookup_program_in_deny_list() {
        let config = Config::builder()
            .set(BashConfig {
                deny: rules_of(&["rm"]),
                ..Default::default()
            })
            .build();
        assert_eq!(bash(&config).lookup(&seg("rm")), Some(Decision::Deny));
    }

    #[test]
    fn lookup_program_in_ask_list() {
        let config = Config::builder()
            .set(BashConfig {
                ask: rules_of(&["docker"]),
                ..Default::default()
            })
            .build();
        assert_eq!(
            bash(&config).lookup(&seg("docker")),
            Some(Decision::Ask)
        );
    }

    #[test]
    fn lookup_unlisted_program_returns_none() {
        let config = Config::builder()
            .set(BashConfig {
                allow: rules_of(&["git"]),
                deny: rules_of(&["rm"]),
                ask: rules_of(&["docker"]),
            })
            .build();
        assert_eq!(bash(&config).lookup(&seg("python")), None);
    }

    #[test]
    fn lookup_program_in_both_allow_and_deny_returns_deny() {
        let config = Config::builder()
            .set(BashConfig {
                allow: rules_of(&["rm"]),
                deny: rules_of(&["rm"]),
                ..Default::default()
            })
            .build();
        assert_eq!(bash(&config).lookup(&seg("rm")), Some(Decision::Deny));
    }

    #[test]
    fn lookup_program_in_both_allow_and_ask_returns_ask() {
        let config = Config::builder()
            .set(BashConfig {
                allow: rules_of(&["docker"]),
                ask: rules_of(&["docker"]),
                ..Default::default()
            })
            .build();
        assert_eq!(
            bash(&config).lookup(&seg("docker")),
            Some(Decision::Ask)
        );
    }

    // --- File loading tests ---

    #[test]
    fn load_nonexistent_file_returns_not_found() {
        let result = Config::builder()
            .register::<BashConfig>()
            .load(Path::new("/tmp/does-not-exist-12345.kdl"));
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

        let config = Config::builder()
            .register::<BashConfig>()
            .load(tmpfile.path())
            .unwrap();
        let b = bash(&config);
        assert_eq!(program_names(&b.allow), vec!["git", "cargo"]);
        assert_eq!(program_names(&b.deny), vec!["rm"]);
    }

    #[test]
    fn load_file_with_invalid_kdl_returns_parse_error() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "invalid {{ kdl {{ syntax").unwrap();

        let result = Config::builder()
            .register::<BashConfig>()
            .load(tmpfile.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    // --- Basename normalization tests ---

    #[test]
    fn lookup_absolute_path_matches_basename() {
        let config = Config::builder()
            .set(BashConfig {
                deny: rules_of(&["rm"]),
                ..Default::default()
            })
            .build();
        let b = bash(&config);
        assert_eq!(b.lookup(&seg("/bin/rm")), Some(Decision::Deny));
        assert_eq!(b.lookup(&seg("/usr/bin/rm")), Some(Decision::Deny));
    }

    #[test]
    fn lookup_relative_path_matches_basename() {
        let config = Config::builder()
            .set(BashConfig {
                allow: rules_of(&["deploy"]),
                ..Default::default()
            })
            .build();
        assert_eq!(
            bash(&config).lookup(&seg("./scripts/deploy")),
            Some(Decision::Allow)
        );
    }

    // --- Inline rule parsing tests ---

    #[test]
    fn rule_simple_program_name() {
        let rules = rules_from_kdl(r#"deny "rm""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert!(rules[0].is_unconditional());
    }

    #[test]
    fn rule_multiple_programs_on_one_node() {
        let rules = rules_from_kdl(r#"allow "git" "cargo""#, "allow");
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].program, "git");
        assert!(rules[0].is_unconditional());
        assert_eq!(rules[1].program, "cargo");
        assert!(rules[1].is_unconditional());
    }

    #[test]
    fn rule_inline_with_flags_and_positional() {
        let rules = rules_from_kdl(r#"deny "rm -rf /""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["-r", "-f"]));
        assert_eq!(rules[0].conditions.subcommand, vec!["/"]);
    }

    #[test]
    fn rule_inline_flags_only() {
        let rules = rules_from_kdl(r#"deny "rm -rf""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["-r", "-f"]));
        assert!(rules[0].conditions.subcommand.is_empty());
    }

    #[test]
    fn rule_inline_with_subcommand_and_flag() {
        let rules = rules_from_kdl(r#"deny "git push --force""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "git");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["--force"]));
        assert_eq!(rules[0].conditions.subcommand, vec!["push"]);
    }

    #[test]
    fn rule_inline_with_tab_whitespace_parses_args() {
        let rules = rules_from_kdl("deny \"rm\t-rf\"", "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["-r", "-f"]));
    }

    #[test]
    fn rule_double_dash_goes_to_required_flags() {
        let rules = rules_from_kdl(r#"deny "git --""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "git");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["--"]));
        assert!(rules[0].conditions.subcommand.is_empty());
    }

    // --- Children block parsing tests ---

    #[test]
    fn rule_children_required_flags() {
        let rules = rules_from_kdl(
            r#"deny "rm" {
                required-flags "r" "f"
            }"#,
            "deny",
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["-r", "-f"]));
    }

    #[test]
    fn rule_children_optional_flags() {
        let rules = rules_from_kdl(
            r#"deny "rm" {
                optional-flags "force"
            }"#,
            "deny",
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].conditions.optional_flags, set_of(&["--force"]));
    }

    #[test]
    fn rule_children_required_arguments() {
        let rules = rules_from_kdl(
            r#"ask "curl" {
                required-arguments "--upload-file *"
            }"#,
            "ask",
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].conditions.required_arguments.len(), 1);
        assert_eq!(
            rules[0].conditions.required_arguments[0].flag,
            "--upload-file"
        );
        assert_eq!(rules[0].conditions.required_arguments[0].value.raw, "*");
    }

    #[test]
    fn rule_children_subcommands() {
        let rules = rules_from_kdl(
            r#"allow "git" {
                subcommands "status" "push origin"
            }"#,
            "allow",
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].conditions.subcommands.len(), 2);
        assert_eq!(rules[0].conditions.subcommands[0], vec!["status"]);
        assert_eq!(rules[0].conditions.subcommands[1], vec!["push", "origin"]);
    }

    #[test]
    fn rule_children_named_positional_matcher() {
        let rules = rules_from_kdl(
            r#"deny "rm" {
                files "/*"
            }"#,
            "deny",
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].conditions.positionals.len(), 1);
        assert_eq!(rules[0].conditions.positionals[0].raw, "/*");
    }

    #[test]
    fn rule_inline_with_children_extends_conditions() {
        let rules = rules_from_kdl(
            r#"allow "claude mcp add" {
                remotes "linear"
            }"#,
            "allow",
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "claude");
        assert_eq!(rules[0].conditions.subcommand, vec!["mcp", "add"]);
        assert_eq!(rules[0].conditions.positionals.len(), 1);
        assert_eq!(rules[0].conditions.positionals[0].raw, "linear");
    }

    #[test]
    fn rule_children_positionals_glob() {
        let rules = rules_from_kdl(
            r#"deny "rm" {
                positionals "/*" "/home/*"
            }"#,
            "deny",
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].conditions.positionals.len(), 2);
        assert_eq!(rules[0].conditions.positionals[0].raw, "/*");
        assert_eq!(rules[0].conditions.positionals[1].raw, "/home/*");
    }

    #[test]
    fn rule_children_flags_normalization() {
        let rules = rules_from_kdl(
            r#"deny "rm" {
                required-flags "r" "force" "-f" "--verbose"
            }"#,
            "deny",
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(
            rules[0].conditions.required_flags,
            set_of(&["-r", "--force", "-f", "--verbose"])
        );
    }

    // --- Error cases ---

    #[test]
    fn rule_invalid_inline_rule_returns_error() {
        let err = rules_err(r#"deny "git &&""#, "deny");
        assert!(
            err.contains("line 2"),
            "should include line number, got: {err}"
        );
    }

    #[test]
    fn rule_invalid_glob_returns_error() {
        let err = rules_err(
            r#"deny "rm" {
            positionals "[invalid"
        }"#,
            "deny",
        );
        assert!(
            err.contains("line 3"),
            "should include line number, got: {err}"
        );
    }

    #[test]
    fn rule_invalid_required_arguments_format() {
        let err = rules_err(
            r#"deny "curl" {
            required-arguments "--upload-file"
        }"#,
            "deny",
        );
        assert!(
            err.contains("line 3"),
            "should include line number, got: {err}"
        );
    }

    #[test]
    fn error_multi_segment_inline_rule() {
        let err = rules_err(r#"deny "git status && rm -rf /""#, "deny");
        assert!(err.contains("multiple commands"), "got: {err}");
        assert!(
            err.contains("line 2"),
            "should include line number, got: {err}"
        );
    }

    #[test]
    fn error_children_without_entry() {
        let err = rules_err(
            r#"deny {
            required-flags "r"
        }"#,
            "deny",
        );
        assert!(err.contains("no program entry"), "got: {err}");
        assert!(
            err.contains("line 2"),
            "should include line number, got: {err}"
        );
    }

    #[test]
    fn error_multi_entry_with_children() {
        let err = rules_err(
            r#"deny "rm" "mv" {
            required-flags "f"
        }"#,
            "deny",
        );
        assert!(err.contains("multiple entries"), "got: {err}");
        assert!(
            err.contains("line 2"),
            "should include line number, got: {err}"
        );
    }

    #[test]
    fn error_includes_correct_line_number() {
        let err = rules_err(
            "allow \"git\"\nallow \"cargo\"\ndeny {\n    required-flags \"r\"\n}",
            "deny",
        );
        assert!(err.contains("line 4"), "should report line 4, got: {err}");
    }

    #[test]
    fn error_propagates_through_config_parse() {
        let result = parse_config(
            r#"
            bash {
                deny "git &&"
            }
            "#,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    #[test]
    fn error_invalid_glob_propagates_through_config_parse() {
        let result = parse_config(
            r#"
            bash {
                deny "rm" {
                    positionals "[invalid"
                }
            }
            "#,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }
}
