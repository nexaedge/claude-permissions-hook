pub mod rule;

use std::path::{Path, PathBuf};

use crate::command::CommandSegment;
use crate::protocol::Decision;

/// Top-level configuration loaded from a KDL file.
#[derive(Debug)]
pub struct Config {
    pub bash: BashConfig,
}

/// Bash-specific configuration: rules for allow, deny, or ask decisions.
#[derive(Debug, Default)]
pub struct BashConfig {
    pub allow: Vec<rule::BashRule>,
    pub deny: Vec<rule::BashRule>,
    pub ask: Vec<rule::BashRule>,
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
            allow: collect_rules(children, "allow")?,
            deny: collect_rules(children, "deny")?,
            ask: collect_rules(children, "ask")?,
        })
    }

    /// Look up a command segment and return its configured decision.
    ///
    /// Normalizes paths to basenames before lookup: `/bin/rm` matches a `deny "rm"` rule.
    /// Precedence: deny > ask > allow. Returns `None` for unlisted programs.
    ///
    /// Currently matches on program name only. Full condition matching (flags,
    /// positionals, subcommands) is implemented in Step 03.
    pub fn lookup(&self, segment: &CommandSegment) -> Option<Decision> {
        let normalized = std::path::Path::new(&segment.program)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(&segment.program);
        if self.deny.iter().any(|r| r.program == normalized) {
            Some(Decision::Deny)
        } else if self.ask.iter().any(|r| r.program == normalized) {
            Some(Decision::Ask)
        } else if self.allow.iter().any(|r| r.program == normalized) {
            Some(Decision::Allow)
        } else {
            None
        }
    }
}

/// Collect all rules from nodes with the given name.
///
/// Handles both simple program entries (`deny "rm"`) and inline argument rules
/// (`deny "rm -rf /"`), plus optional children blocks for extended conditions.
fn collect_rules(
    doc: &kdl::KdlDocument,
    node_name: &str,
) -> Result<Vec<rule::BashRule>, ConfigError> {
    let mut rules = Vec::new();
    for node in doc.nodes().iter().filter(|n| n.name().value() == node_name) {
        let string_entries: Vec<&str> = node
            .entries()
            .iter()
            .filter_map(|e| e.value().as_string())
            .collect();
        let has_children = node.children().is_some();

        // Reject children block without any string entry (fail-closed)
        if has_children && string_entries.is_empty() {
            return Err(ConfigError::ParseError(format!(
                "{node_name} node has children block but no program entry"
            )));
        }

        // Reject multiple entries combined with children (ambiguous semantics)
        if has_children && string_entries.len() > 1 {
            return Err(ConfigError::ParseError(format!(
                "{node_name} node has children block with multiple entries; \
                 use separate nodes instead"
            )));
        }

        for value in &string_entries {
            let bash_rule = parse_rule_entry(value)?;
            rules.push(bash_rule);
        }

        // Children extend the single entry when present.
        if let Some(children) = node.children() {
            if let Some(last_rule) = rules.last_mut() {
                parse_children_block(children, &mut last_rule.conditions)?;
            }
        }
    }
    Ok(rules)
}

/// Parse a single rule entry string into a BashRule.
///
/// Simple program name (no spaces) → BashRule with empty conditions.
/// Rule with args → parse with command::parse(), classify args into conditions.
fn parse_rule_entry(value: &str) -> Result<rule::BashRule, ConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::ParseError("empty rule string".to_string()));
    }

    // Simple program name: no spaces → empty conditions
    if !trimmed.contains(' ') {
        return Ok(rule::BashRule {
            program: trimmed.to_string(),
            conditions: rule::RuleConditions::default(),
        });
    }

    // Parse with command::parse() to get program + args
    let segments = crate::command::parse(trimmed)
        .map_err(|e| ConfigError::ParseError(format!("invalid rule '{trimmed}': {e}")))?;

    // Require exactly one command segment — reject multi-command rules
    // (e.g., "git status && rm -rf /" would parse to 2 segments)
    if segments.len() > 1 {
        return Err(ConfigError::ParseError(format!(
            "rule '{trimmed}' contains multiple commands; use separate rules instead"
        )));
    }

    let segment = segments
        .into_iter()
        .next()
        .ok_or_else(|| ConfigError::ParseError(format!("no program found in rule '{trimmed}'")))?;

    let mut conditions = rule::RuleConditions::default();
    for arg in &segment.args {
        if arg.starts_with('-') {
            conditions.required_flags.insert(arg.clone());
        } else {
            conditions.subcommand.push(arg.clone());
        }
    }

    Ok(rule::BashRule {
        program: segment.program,
        conditions,
    })
}

/// Parse a children block to extend rule conditions.
///
/// Supported children nodes:
/// - `required-flags "r" "f"` → normalize and add to required_flags
/// - `optional-flags "r" "f"` → normalize and set optional_flags
/// - `positionals "/*" "/home/*"` → compile as globs
/// - `required-arguments "--upload-file *"` → split to ArgumentPattern
/// - `subcommands "status" "push origin"` → split to subcommand chains
/// - Any other name → named positional matcher (add to positionals)
fn parse_children_block(
    children: &kdl::KdlDocument,
    conditions: &mut rule::RuleConditions,
) -> Result<(), ConfigError> {
    for node in children.nodes() {
        let name = node.name().value();
        let values: Vec<String> = node
            .entries()
            .iter()
            .filter_map(|e| e.value().as_string().map(String::from))
            .collect();

        match name {
            "required-flags" => {
                for v in &values {
                    conditions.required_flags.insert(rule::normalize_flag(v));
                }
            }
            "optional-flags" => {
                for v in &values {
                    conditions.optional_flags.insert(rule::normalize_flag(v));
                }
            }
            "positionals" => {
                for v in &values {
                    let pattern = rule::compile_glob(v).map_err(ConfigError::ParseError)?;
                    conditions.positionals.push(pattern);
                }
            }
            "required-arguments" => {
                for v in &values {
                    let pattern = parse_argument_pattern(v)?;
                    conditions.required_arguments.push(pattern);
                }
            }
            "subcommands" => {
                for v in &values {
                    let chain: Vec<String> = v.split_whitespace().map(String::from).collect();
                    conditions.subcommands.push(chain);
                }
            }
            _ => {
                // Named positional matcher (e.g., `files "/*"`, `remotes "linear"`)
                for v in &values {
                    let pattern = rule::compile_glob(v).map_err(ConfigError::ParseError)?;
                    conditions.positionals.push(pattern);
                }
            }
        }
    }
    Ok(())
}

/// Parse a `required-arguments` entry: `"--upload-file *"` → ArgumentPattern.
fn parse_argument_pattern(value: &str) -> Result<rule::ArgumentPattern, ConfigError> {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(ConfigError::ParseError(format!(
            "required-arguments entry must have flag and value pattern: '{value}'"
        )));
    }
    let flag = parts[0].to_string();
    let pattern = rule::compile_glob(parts[1]).map_err(ConfigError::ParseError)?;
    Ok(rule::ArgumentPattern {
        flag,
        value: pattern,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // --- Helpers ---

    fn set_of(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Create simple unconditional rules from program names.
    fn rules_of(programs: &[&str]) -> Vec<rule::BashRule> {
        programs
            .iter()
            .map(|p| rule::BashRule {
                program: p.to_string(),
                conditions: rule::RuleConditions::default(),
            })
            .collect()
    }

    /// Helper to create a CommandSegment for lookup tests.
    fn seg(program: &str) -> CommandSegment {
        CommandSegment {
            program: program.to_string(),
            args: vec![],
        }
    }

    /// Extract program names from rules for assertion.
    fn program_names(rules: &[rule::BashRule]) -> Vec<&str> {
        rules.iter().map(|r| r.program.as_str()).collect()
    }

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

        let mut allow_names: Vec<&str> = program_names(&config.bash.allow);
        allow_names.sort();
        assert_eq!(allow_names, vec!["cargo", "git", "npm"]);
        let mut deny_names: Vec<&str> = program_names(&config.bash.deny);
        deny_names.sort();
        assert_eq!(deny_names, vec!["rm", "shutdown"]);
        let mut ask_names: Vec<&str> = program_names(&config.bash.ask);
        ask_names.sort();
        assert_eq!(ask_names, vec!["docker", "kubectl"]);
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

        assert_eq!(program_names(&config.bash.allow), vec!["git"]);
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

        assert_eq!(
            program_names(&config.bash.allow),
            vec!["git", "cargo", "npm"]
        );
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
            allow: rules_of(&["git"]),
            deny: vec![],
            ask: vec![],
        };
        assert_eq!(bash.lookup(&seg("git")), Some(Decision::Allow));
    }

    #[test]
    fn lookup_program_in_deny_list() {
        let bash = BashConfig {
            allow: vec![],
            deny: rules_of(&["rm"]),
            ask: vec![],
        };
        assert_eq!(bash.lookup(&seg("rm")), Some(Decision::Deny));
    }

    #[test]
    fn lookup_program_in_ask_list() {
        let bash = BashConfig {
            allow: vec![],
            deny: vec![],
            ask: rules_of(&["docker"]),
        };
        assert_eq!(bash.lookup(&seg("docker")), Some(Decision::Ask));
    }

    #[test]
    fn lookup_unlisted_program_returns_none() {
        let bash = BashConfig {
            allow: rules_of(&["git"]),
            deny: rules_of(&["rm"]),
            ask: rules_of(&["docker"]),
        };
        assert_eq!(bash.lookup(&seg("python")), None);
    }

    #[test]
    fn lookup_program_in_both_allow_and_deny_returns_deny() {
        let bash = BashConfig {
            allow: rules_of(&["rm"]),
            deny: rules_of(&["rm"]),
            ask: vec![],
        };
        assert_eq!(bash.lookup(&seg("rm")), Some(Decision::Deny));
    }

    #[test]
    fn lookup_program_in_both_allow_and_ask_returns_ask() {
        let bash = BashConfig {
            allow: rules_of(&["docker"]),
            deny: vec![],
            ask: rules_of(&["docker"]),
        };
        assert_eq!(bash.lookup(&seg("docker")), Some(Decision::Ask));
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
        assert_eq!(program_names(&config.bash.allow), vec!["git", "cargo"]);
        assert_eq!(program_names(&config.bash.deny), vec!["rm"]);
    }

    // --- Basename normalization tests ---

    #[test]
    fn lookup_absolute_path_matches_basename() {
        let bash = BashConfig {
            allow: vec![],
            deny: rules_of(&["rm"]),
            ask: vec![],
        };
        assert_eq!(bash.lookup(&seg("/bin/rm")), Some(Decision::Deny));
        assert_eq!(bash.lookup(&seg("/usr/bin/rm")), Some(Decision::Deny));
    }

    #[test]
    fn lookup_relative_path_matches_basename() {
        let bash = BashConfig {
            allow: rules_of(&["deploy"]),
            deny: vec![],
            ask: vec![],
        };
        assert_eq!(bash.lookup(&seg("./scripts/deploy")), Some(Decision::Allow));
    }

    #[test]
    fn load_file_with_invalid_kdl_returns_parse_error() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "invalid {{ kdl {{ syntax").unwrap();

        let result = Config::load(tmpfile.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    // --- collect_rules: inline rule parsing ---

    /// Helper to parse KDL and collect rules from a specific node name.
    fn rules_from_kdl(kdl: &str, node_name: &str) -> Vec<rule::BashRule> {
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        collect_rules(&doc, node_name).unwrap()
    }

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
        // deny "rm -rf /" → program="rm", required_flags={"-r","-f"}, subcommand=["/"]
        let rules = rules_from_kdl(r#"deny "rm -rf /""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["-r", "-f"]));
        assert_eq!(rules[0].conditions.subcommand, vec!["/"]);
    }

    #[test]
    fn rule_inline_flags_only() {
        // deny "rm -rf" → program="rm", required_flags={"-r","-f"}, NO subcommand
        let rules = rules_from_kdl(r#"deny "rm -rf""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["-r", "-f"]));
        assert!(rules[0].conditions.subcommand.is_empty());
    }

    #[test]
    fn rule_inline_with_subcommand_and_flag() {
        // deny "git push --force" → program="git", required_flags={"--force"}, subcommand=["push"]
        let rules = rules_from_kdl(r#"deny "git push --force""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "git");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["--force"]));
        assert_eq!(rules[0].conditions.subcommand, vec!["push"]);
    }

    // --- collect_rules: children block parsing ---

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
        // Named matcher `files "/*"` → same as positionals "/*"
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
        // allow "claude mcp add" { remotes "linear" }
        // → subcommand=["mcp","add"] + positionals=["linear"]
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
        let doc: kdl::KdlDocument = r#"deny "git &&""#.parse().unwrap();
        let result = collect_rules(&doc, "deny");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    #[test]
    fn rule_invalid_glob_returns_error() {
        let doc: kdl::KdlDocument = r#"deny "rm" {
            positionals "[invalid"
        }"#
        .parse()
        .unwrap();
        let result = collect_rules(&doc, "deny");
        assert!(result.is_err());
    }

    #[test]
    fn rule_invalid_required_arguments_format() {
        let doc: kdl::KdlDocument = r#"deny "curl" {
            required-arguments "--upload-file"
        }"#
        .parse()
        .unwrap();
        let result = collect_rules(&doc, "deny");
        assert!(result.is_err());
    }

    #[test]
    fn rule_double_dash_goes_to_required_flags() {
        // "git --" is valid shell — `--` starts with `-` so it classifies as a flag.
        // Unusual rule but consistent behavior.
        let rules = rules_from_kdl(r#"deny "git --""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "git");
        assert_eq!(rules[0].conditions.required_flags, set_of(&["--"]));
        assert!(rules[0].conditions.subcommand.is_empty());
    }

    #[test]
    fn error_propagates_through_config_parse() {
        // Invalid inline rule should cause Config::parse to fail
        let result = Config::parse(
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
    fn error_multi_segment_inline_rule() {
        // "git status && rm -rf /" contains operators → multiple segments → error
        let doc: kdl::KdlDocument = r#"deny "git status && rm -rf /""#.parse().unwrap();
        let result = collect_rules(&doc, "deny");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("multiple commands"), "got: {err}");
    }

    #[test]
    fn error_children_without_entry() {
        // deny { required-flags "r" } → no program entry → error
        let doc: kdl::KdlDocument = r#"deny {
            required-flags "r"
        }"#
        .parse()
        .unwrap();
        let result = collect_rules(&doc, "deny");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no program entry"), "got: {err}");
    }

    #[test]
    fn error_multi_entry_with_children() {
        // deny "rm" "mv" { required-flags "f" } → ambiguous → error
        let doc: kdl::KdlDocument = r#"deny "rm" "mv" {
            required-flags "f"
        }"#
        .parse()
        .unwrap();
        let result = collect_rules(&doc, "deny");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("multiple entries"), "got: {err}");
    }

    #[test]
    fn error_invalid_glob_propagates_through_config_parse() {
        let result = Config::parse(
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
