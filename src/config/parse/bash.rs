use crate::config::normalize::bash::normalize_subcommand_chains;
use crate::domain::rule::bash::{compile_glob, BashConditions, BashRule};
use crate::domain::Decision;
use crate::error::ConfigError;

use super::{parse_tier, ConfigNode};

/// Parse the `bash` section out of a top-level config node slice.
///
/// Returns `None` when the section is absent or empty.
pub(in crate::config) fn parse_bash(
    config_nodes: &[ConfigNode],
) -> Result<Option<Vec<BashRule>>, ConfigError> {
    match ConfigNode::body_of(config_nodes, "bash") {
        Some(rule_nodes) => parse_bash_nodes(rule_nodes).map(Some),
        None => Ok(None),
    }
}

/// Parse a list of `ConfigNode`s into `BashRule`s.
///
/// Caller guarantees `nodes` is non-empty.
pub(in crate::config) fn parse_bash_nodes(
    nodes: &[ConfigNode],
) -> Result<Vec<BashRule>, ConfigError> {
    let mut rules = Vec::new();
    for node in nodes {
        let decision = parse_tier(&node.name, node.line)?;

        if node.arguments.is_empty() {
            return Err(ConfigError::InvalidSyntax(format!(
                "line {}: {} node has no program entries",
                node.line, node.name
            )));
        }

        let conditions = parse_conditions_from_body(node)?;

        for value in &node.arguments {
            let bash_rule =
                parse_single_rule(value, decision.clone(), conditions.clone(), node.line)?;
            rules.push(bash_rule);
        }
    }
    Ok(rules)
}

/// Parse a single rule string into a `BashRule`.
///
/// Simple program name (no whitespace) → BashRule with empty conditions.
/// Rule with args → parse with command::parse(), classify into conditions.
/// The provided conditions are then merged in (children override inline).
fn parse_single_rule(
    value: &str,
    decision: Decision,
    mut extra_conditions: BashConditions,
    line: usize,
) -> Result<BashRule, ConfigError> {
    let at_line = |e: ConfigError| {
        let ConfigError::InvalidSyntax(msg) = e;
        ConfigError::InvalidSyntax(format!("line {line}: {msg}"))
    };

    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidSyntax(format!(
            "line {line}: empty rule string"
        )));
    }

    // Simple program name: no whitespace → empty inline conditions
    if trimmed.split_whitespace().nth(1).is_none() {
        // Apply body conditions directly (no inline subcommand to normalize)
        let program = crate::domain::ProgramName::parse(trimmed)
            .map_err(|_| ConfigError::InvalidSyntax(format!("line {line}: empty program name")))?;
        return Ok(BashRule {
            decision,
            program,
            conditions: extra_conditions,
        });
    }

    // Parse with command::parse() to get program + args
    let segments = crate::command::parse(trimmed)
        .map_err(|e| ConfigError::InvalidSyntax(format!("invalid rule '{trimmed}': {e}")))
        .map_err(&at_line)?;

    // Require exactly one command segment
    if segments.len() > 1 {
        return Err(ConfigError::InvalidSyntax(format!(
            "line {line}: rule '{trimmed}' contains multiple commands; use separate rules instead"
        )));
    }

    let segment = segments.into_iter().next().ok_or_else(|| {
        ConfigError::InvalidSyntax(format!("line {line}: no program found in rule '{trimmed}'"))
    })?;

    // Parse inline args into a base conditions, then merge extra_conditions on top
    let mut inline_conditions = BashConditions::default();
    for arg in &segment.args {
        if arg.starts_with('-') {
            inline_conditions
                .required_flags
                .insert(crate::domain::Flag::new(arg));
        } else {
            inline_conditions.subcommand.push(arg.clone());
        }
    }

    // Merge body conditions into inline conditions
    inline_conditions
        .required_flags
        .extend(extra_conditions.required_flags.drain());
    inline_conditions
        .optional_flags
        .extend(extra_conditions.optional_flags.drain());
    inline_conditions
        .positionals
        .append(&mut extra_conditions.positionals);
    inline_conditions
        .required_arguments
        .append(&mut extra_conditions.required_arguments);
    inline_conditions
        .subcommands
        .append(&mut extra_conditions.subcommands);

    // Normalize inline subcommand with children subcommand chains
    normalize_subcommand_chains(&mut inline_conditions);

    Ok(BashRule {
        decision,
        program: segment.program,
        conditions: inline_conditions,
    })
}

/// Parse condition children nodes from a rule body.
fn parse_conditions_from_body(node: &ConfigNode) -> Result<BashConditions, ConfigError> {
    let mut conditions = BashConditions::default();
    for child in node.body_nodes() {
        let line = child.line;
        let glob_at_line = |msg: String| ConfigError::InvalidSyntax(format!("line {line}: {msg}"));
        let err_at_line = |e: ConfigError| {
            let ConfigError::InvalidSyntax(msg) = e;
            ConfigError::InvalidSyntax(format!("line {line}: {msg}"))
        };

        match child.name.as_str() {
            "required-flags" => {
                for v in &child.arguments {
                    conditions
                        .required_flags
                        .insert(crate::domain::Flag::new(v));
                }
            }
            "optional-flags" => {
                for v in &child.arguments {
                    conditions
                        .optional_flags
                        .insert(crate::domain::Flag::new(v));
                }
            }
            "positionals" => {
                for v in &child.arguments {
                    let pattern = compile_glob(v).map_err(&glob_at_line)?;
                    conditions.positionals.push(pattern);
                }
            }
            "required-arguments" => {
                for v in &child.arguments {
                    let pattern = parse_argument_pattern(v).map_err(&err_at_line)?;
                    conditions.required_arguments.push(pattern);
                }
            }
            "subcommands" => {
                for v in &child.arguments {
                    let chain: Vec<String> = v.split_whitespace().map(String::from).collect();
                    conditions.subcommands.push(chain);
                }
            }
            _ => {
                // Named positional matcher (e.g., `files "/*"`, `remotes "linear"`)
                for v in &child.arguments {
                    let pattern = compile_glob(v).map_err(&glob_at_line)?;
                    conditions.positionals.push(pattern);
                }
            }
        }
    }
    Ok(conditions)
}

/// Parse a `required-arguments` entry: `"--upload-file *"` -> ArgumentPattern.
fn parse_argument_pattern(
    value: &str,
) -> Result<crate::domain::rule::bash::ArgumentPattern, ConfigError> {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(ConfigError::InvalidSyntax(format!(
            "required-arguments entry must have flag and value pattern: '{value}'"
        )));
    }
    let flag = parts[0].to_string();
    let pattern = compile_glob(parts[1]).map_err(ConfigError::InvalidSyntax)?;
    Ok(crate::domain::rule::bash::ArgumentPattern {
        flag,
        value: pattern,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn flag_set(items: &[&str]) -> HashSet<crate::domain::Flag> {
        items.iter().map(|s| crate::domain::Flag::new(s)).collect()
    }

    /// Parse raw KDL source into BashRules.
    fn rules_from_kdl(source: &str) -> Vec<BashRule> {
        let nodes = super::super::parse_section_from_source(source).unwrap();
        parse_bash_nodes(&nodes).unwrap()
    }

    /// Parse raw KDL and return the error string.
    fn rules_err(source: &str) -> String {
        let nodes = match super::super::parse_section_from_source(source) {
            Err(e) => return e.to_string(),
            Ok(nodes) => nodes,
        };
        parse_bash_nodes(&nodes).unwrap_err().to_string()
    }

    #[test]
    fn rule_simple_program_name() {
        let rules = rules_from_kdl(r#"deny "rm""#);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].decision, Decision::Deny);
        assert!(rules[0].is_unconditional());
    }

    #[test]
    fn rule_multiple_programs_on_one_node() {
        let rules = rules_from_kdl(r#"allow "git" "cargo""#);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].program, "git");
        assert_eq!(rules[0].decision, Decision::Allow);
        assert!(rules[0].is_unconditional());
        assert_eq!(rules[1].program, "cargo");
        assert_eq!(rules[1].decision, Decision::Allow);
        assert!(rules[1].is_unconditional());
    }

    #[test]
    fn rule_inline_with_flags_and_positional() {
        let rules = rules_from_kdl(r#"deny "rm -rf /""#);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["-r", "-f"]));
        assert_eq!(rules[0].conditions.subcommand, vec!["/"]);
    }

    #[test]
    fn rule_inline_flags_only() {
        let rules = rules_from_kdl(r#"deny "rm -rf""#);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["-r", "-f"]));
        assert!(rules[0].conditions.subcommand.is_empty());
    }

    #[test]
    fn rule_inline_with_subcommand_and_flag() {
        let rules = rules_from_kdl(r#"deny "git push --force""#);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "git");
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["--force"]));
        assert_eq!(rules[0].conditions.subcommand, vec!["push"]);
    }

    #[test]
    fn rule_inline_with_tab_whitespace_parses_args() {
        let rules = rules_from_kdl("deny \"rm\t-rf\"");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["-r", "-f"]));
    }

    #[test]
    fn rule_double_dash_goes_to_required_flags() {
        let rules = rules_from_kdl(r#"deny "git --""#);
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
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(
            rules[0].conditions.required_flags,
            flag_set(&["-r", "--force", "-f", "--verbose"])
        );
    }

    #[test]
    fn rule_invalid_inline_rule_returns_error() {
        let err = rules_err(r#"deny "git &&""#);
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
        );
        assert!(
            err.contains("line 3"),
            "should include line number, got: {err}"
        );
    }

    #[test]
    fn error_multi_segment_inline_rule() {
        let err = rules_err(r#"deny "git status && rm -rf /""#);
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
        );
        assert!(err.contains("no program entries"), "got: {err}");
        assert!(
            err.contains("line 2"),
            "should include line number, got: {err}"
        );
    }

    /// Previously this was an error ("multiple entries with children").
    /// Now it's valid: multiple args each become separate rules with the same conditions.
    #[test]
    fn multi_entry_with_children_creates_multiple_rules() {
        let rules = rules_from_kdl(
            r#"deny "rm" "mv" {
            required-flags "f"
        }"#,
        );
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].program, "rm");
        assert_eq!(rules[0].decision, Decision::Deny);
        assert_eq!(rules[0].conditions.required_flags, flag_set(&["-f"]));
        assert_eq!(rules[1].program, "mv");
        assert_eq!(rules[1].decision, Decision::Deny);
        assert_eq!(rules[1].conditions.required_flags, flag_set(&["-f"]));
    }

    #[test]
    fn error_includes_correct_line_number() {
        let err = rules_err("allow \"git\"\nallow \"cargo\"\ndeny {\n    required-flags \"r\"\n}");
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
        );
        assert_eq!(rules.len(), 1);
        assert!(rules[0].conditions.subcommand.is_empty());
        assert_eq!(
            rules[0].conditions.subcommands,
            vec![vec!["push", "origin"], vec!["push", "upstream"]]
        );
    }
}
