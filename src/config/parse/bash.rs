use crate::config::normalize::bash::normalize_subcommand_chains;
use crate::config::rule::{self, compile_glob};
use crate::config::section::{ChildNode, RuleEntry};
use crate::config::ConfigError;

/// Parse a tier's rule entries into BashRules.
pub(crate) fn parse_rules(entries: Vec<RuleEntry>) -> Result<Vec<rule::BashRule>, ConfigError> {
    let mut rules = Vec::new();
    for entry in entries {
        let at_line = |e: ConfigError| match e {
            ConfigError::ParseError(msg) => {
                ConfigError::ParseError(format!("line {}: {msg}", entry.line))
            }
            other => other,
        };

        for value in &entry.values {
            let bash_rule = parse_rule_entry(value).map_err(&at_line)?;
            rules.push(bash_rule);
        }

        // Children extend the last parsed rule.
        if let Some(children) = &entry.children {
            if let Some(last_rule) = rules.last_mut() {
                parse_children(children, &mut last_rule.conditions)?;
                // When both inline subcommand and children subcommands exist,
                // children chains are relative to the inline subcommand position.
                // Prepend the inline subcommand to each chain, then clear it.
                // e.g., "git push" { subcommands "origin" } → chains [["push","origin"]]
                normalize_subcommand_chains(&mut last_rule.conditions);
            }
        }
    }
    Ok(rules)
}

/// Parse a single rule entry string into a BashRule.
///
/// Simple program name (no whitespace) -> BashRule with empty conditions.
/// Rule with args -> parse with command::parse(), classify args into conditions.
fn parse_rule_entry(value: &str) -> Result<rule::BashRule, ConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::ParseError("empty rule string".to_string()));
    }

    // Simple program name: no whitespace -> empty conditions
    if trimmed.split_whitespace().nth(1).is_none() {
        return Ok(rule::BashRule {
            program: crate::domain::ProgramName::new(trimmed),
            conditions: rule::RuleConditions::default(),
        });
    }

    // Parse with command::parse() to get program + args
    let segments = crate::command::parse(trimmed)
        .map_err(|e| ConfigError::ParseError(format!("invalid rule '{trimmed}': {e}")))?;

    // Require exactly one command segment
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
            conditions
                .required_flags
                .insert(crate::domain::Flag::new(arg));
        } else {
            conditions.subcommand.push(arg.clone());
        }
    }

    Ok(rule::BashRule {
        program: segment.program,
        conditions,
    })
}

/// Parse children nodes to extend rule conditions.
///
/// Operates on the intermediate [`ChildNode`] representation — no KDL dependency.
fn parse_children(
    children: &[ChildNode],
    conditions: &mut rule::RuleConditions,
) -> Result<(), ConfigError> {
    for child in children {
        let line = child.line;
        let glob_at_line = |msg: String| ConfigError::ParseError(format!("line {line}: {msg}"));
        let err_at_line = |e: ConfigError| match e {
            ConfigError::ParseError(msg) => ConfigError::ParseError(format!("line {line}: {msg}")),
            other => other,
        };

        match child.name.as_str() {
            "required-flags" => {
                for v in &child.values {
                    conditions
                        .required_flags
                        .insert(crate::domain::Flag::new(v));
                }
            }
            "optional-flags" => {
                for v in &child.values {
                    conditions
                        .optional_flags
                        .insert(crate::domain::Flag::new(v));
                }
            }
            "positionals" => {
                for v in &child.values {
                    let pattern = compile_glob(v).map_err(&glob_at_line)?;
                    conditions.positionals.push(pattern);
                }
            }
            "required-arguments" => {
                for v in &child.values {
                    let pattern = parse_argument_pattern(v).map_err(&err_at_line)?;
                    conditions.required_arguments.push(pattern);
                }
            }
            "subcommands" => {
                for v in &child.values {
                    let chain: Vec<String> = v.split_whitespace().map(String::from).collect();
                    conditions.subcommands.push(chain);
                }
            }
            _ => {
                // Named positional matcher (e.g., `files "/*"`, `remotes "linear"`)
                for v in &child.values {
                    let pattern = compile_glob(v).map_err(&glob_at_line)?;
                    conditions.positionals.push(pattern);
                }
            }
        }
    }
    Ok(())
}

/// Parse a `required-arguments` entry: `"--upload-file *"` -> ArgumentPattern.
fn parse_argument_pattern(value: &str) -> Result<rule::ArgumentPattern, ConfigError> {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(ConfigError::ParseError(format!(
            "required-arguments entry must have flag and value pattern: '{value}'"
        )));
    }
    let flag = parts[0].to_string();
    let pattern = compile_glob(parts[1]).map_err(ConfigError::ParseError)?;
    Ok(rule::ArgumentPattern {
        flag,
        value: pattern,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::section;
    use std::collections::HashSet;

    fn flag_set(items: &[&str]) -> HashSet<crate::domain::Flag> {
        items.iter().map(|s| crate::domain::Flag::new(s)).collect()
    }

    /// Parse raw KDL source into a ToolSection and extract rules for the given tier.
    fn rules_from_kdl(source: &str, tier: &str) -> Vec<rule::BashRule> {
        let ts = section::parse_from_source(source).unwrap();
        let entries = match tier {
            "allow" => ts.allow,
            "deny" => ts.deny,
            "ask" => ts.ask,
            _ => panic!("unknown tier: {tier}"),
        };
        parse_rules(entries).unwrap()
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
        parse_rules(entries).unwrap_err().to_string()
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
        let rules = rules_from_kdl(r#"deny "rm -rf /""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["-r", "-f"]));
        assert_eq!(rules[0].conditions.subcommand, vec!["/"]);
    }

    #[test]
    fn rule_inline_flags_only() {
        let rules = rules_from_kdl(r#"deny "rm -rf""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["-r", "-f"]));
        assert!(rules[0].conditions.subcommand.is_empty());
    }

    #[test]
    fn rule_inline_with_subcommand_and_flag() {
        let rules = rules_from_kdl(r#"deny "git push --force""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "git");
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["--force"]));
        assert_eq!(rules[0].conditions.subcommand, vec!["push"]);
    }

    #[test]
    fn rule_inline_with_tab_whitespace_parses_args() {
        let rules = rules_from_kdl("deny \"rm\t-rf\"", "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["-r", "-f"]));
    }

    #[test]
    fn rule_double_dash_goes_to_required_flags() {
        let rules = rules_from_kdl(r#"deny "git --""#, "deny");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "git");
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["--"]));
        assert!(rules[0].conditions.subcommand.is_empty());
    }

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
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["-r", "-f"]));
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
        assert_eq!(rules[0].conditions.optional_flags, flag_set(&["--force"]));
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
            flag_set(&["-r", "--force", "-f", "--verbose"])
        );
    }

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

    // --- Subcommand normalization via parse ---

    #[test]
    fn inline_subcommand_with_children_subcommands_normalized() {
        // "git push" { subcommands "origin" } → subcommands=[["push","origin"]], subcommand=[]
        let rules = rules_from_kdl(
            r#"allow "git push" {
                subcommands "origin"
            }"#,
            "allow",
        );
        assert_eq!(rules.len(), 1);
        assert!(rules[0].conditions.subcommand.is_empty());
        assert_eq!(
            rules[0].conditions.subcommands,
            vec![vec!["push", "origin"]]
        );
    }

    #[test]
    fn inline_subcommand_with_multiple_children_chains() {
        // "git push" { subcommands "origin" "upstream" }
        // → [["push","origin"], ["push","upstream"]]
        let rules = rules_from_kdl(
            r#"allow "git push" {
                subcommands "origin" "upstream"
            }"#,
            "allow",
        );
        assert_eq!(rules.len(), 1);
        assert!(rules[0].conditions.subcommand.is_empty());
        assert_eq!(
            rules[0].conditions.subcommands,
            vec![vec!["push", "origin"], vec!["push", "upstream"]]
        );
    }
}
