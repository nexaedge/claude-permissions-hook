//! File tool configuration.
//!
//! Parses file-specific rules from the `files` section using custom parsing
//! via `ConfigDocument`. Handles flat one-liner rules (`deny "~/.ssh/**" "read"`)
//! and path-first blocks (`"<cwd>/**" { allow "read" "write" }`).

use std::collections::HashSet;

use crate::file_tools::FileOperation;
use crate::protocol::Decision;

use super::document::ConfigDocument;
use super::ConfigError;

/// File tool configuration: rules for allow, deny, or ask decisions by path.
#[derive(Debug, Default)]
pub struct FilesConfig {
    pub deny: Vec<FileRule>,
    pub ask: Vec<FileRule>,
    pub allow: Vec<FileRule>,
}

/// A single file rule binding a path pattern to a set of operations.
#[derive(Debug)]
pub struct FileRule {
    /// Raw glob pattern (may contain `<cwd>`, `<home>`, `~`).
    pub raw_pattern: String,
    /// Which file operations this rule applies to.
    pub operations: HashSet<FileOperation>,
    /// 1-based line number in the source file.
    pub line: usize,
}

impl FilesConfig {
    /// Look up a normalized path and operation against file rules.
    ///
    /// Checks tiers in order: deny → ask → allow. First matching tier wins.
    /// Returns `None` if no rule in any tier matches.
    ///
    /// Invalid glob patterns fail toward the more restrictive outcome:
    /// deny/ask tiers treat errors as matching, allow tier treats errors as non-matching.
    pub fn lookup(
        &self,
        normalized_path: &str,
        operation: FileOperation,
        cwd: &str,
        _home: &str,
    ) -> Option<Decision> {
        if matches_any_rule(&self.deny, normalized_path, operation, cwd, true) {
            return Some(Decision::Deny);
        }
        if matches_any_rule(&self.ask, normalized_path, operation, cwd, true) {
            return Some(Decision::Ask);
        }
        if matches_any_rule(&self.allow, normalized_path, operation, cwd, false) {
            return Some(Decision::Allow);
        }
        None
    }
}

/// Check if any rule in a tier matches the given path and operation.
///
/// `error_means_match`: when `true`, invalid glob patterns are treated as
/// matching (fail-closed for deny/ask); when `false`, treated as non-matching
/// (fail-closed for allow).
fn matches_any_rule(
    rules: &[FileRule],
    normalized_path: &str,
    operation: FileOperation,
    cwd: &str,
    error_means_match: bool,
) -> bool {
    rules.iter().any(|rule| {
        if !rule.operations.contains(&operation) {
            return false;
        }
        let expanded = crate::path::expand_pattern(&rule.raw_pattern, cwd);
        crate::path::matches(normalized_path, &expanded).unwrap_or(error_means_match)
    })
}

/// Parse the `files` section from a config document.
///
/// Returns `None` when the `files` section is absent.
pub(super) fn parse_files(doc: &ConfigDocument) -> Result<Option<FilesConfig>, ConfigError> {
    let section = match doc.section("files") {
        Some(s) => s,
        None => return Ok(None),
    };

    let mut config = FilesConfig::default();

    for node in section.nodes() {
        match node.name() {
            "allow" | "deny" | "ask" => {
                parse_flat_rule(&node, &mut config)?;
            }
            _ => {
                parse_path_block(&node, &mut config)?;
            }
        }
    }

    Ok(Some(config))
}

/// Parse a flat one-liner rule: `deny "~/.ssh/**" "read" "write"`.
///
/// Node name determines the tier. First string value is the path pattern,
/// remaining values are operations.
fn parse_flat_rule(
    node: &super::document::ParseNode<'_>,
    config: &mut FilesConfig,
) -> Result<(), ConfigError> {
    let tier = node.name();
    let line = node.line();
    let values = node.string_values();

    if node.entry_count() != values.len() {
        return Err(ConfigError::ParseError(format!(
            "line {line}: {tier} node contains non-string values; \
             all entries must be quoted strings"
        )));
    }

    if values.is_empty() {
        return Err(ConfigError::ParseError(format!(
            "line {line}: {tier} node requires a path pattern and at least one operation"
        )));
    }

    let raw_pattern = values[0].to_string();
    let op_strings = &values[1..];

    if op_strings.is_empty() {
        return Err(ConfigError::ParseError(format!(
            "line {line}: {tier} node for pattern \"{raw_pattern}\" requires at least one operation"
        )));
    }

    let operations = parse_operations(op_strings, line)?;
    let rule = FileRule {
        raw_pattern,
        operations,
        line,
    };

    push_rule(config, tier, rule);
    Ok(())
}

/// Parse a path-first block: `"<cwd>/**" { allow "read" "write" }`.
///
/// Node name is the path pattern. Children nodes named `allow`/`deny`/`ask`
/// define the tier, with their string values parsed as operations.
fn parse_path_block(
    node: &super::document::ParseNode<'_>,
    config: &mut FilesConfig,
) -> Result<(), ConfigError> {
    let raw_pattern = node.name().to_string();
    let line = node.line();

    if !node.string_values().is_empty() || node.entry_count() > 0 {
        return Err(ConfigError::ParseError(format!(
            "line {line}: path block \"{raw_pattern}\" must not have inline values; \
             use allow/deny/ask children instead"
        )));
    }

    if !node.has_children() {
        return Err(ConfigError::ParseError(format!(
            "line {line}: path block \"{raw_pattern}\" requires a children block with \
             allow/deny/ask nodes"
        )));
    }

    let children = node.children().expect("has_children was true");
    let child_nodes = children.nodes();

    if child_nodes.is_empty() {
        return Err(ConfigError::ParseError(format!(
            "line {line}: path block \"{raw_pattern}\" has an empty children block"
        )));
    }

    let mut found_tier = false;
    for child in &child_nodes {
        let child_tier = child.name();
        match child_tier {
            "allow" | "deny" | "ask" => {
                let op_strings = child.string_values();
                if child.entry_count() != op_strings.len() {
                    return Err(ConfigError::ParseError(format!(
                        "line {}: {child_tier} node in path block \"{raw_pattern}\" \
                         contains non-string values; all entries must be quoted strings",
                        child.line()
                    )));
                }
                if op_strings.is_empty() {
                    return Err(ConfigError::ParseError(format!(
                        "line {}: {child_tier} node in path block \"{raw_pattern}\" \
                         requires at least one operation",
                        child.line()
                    )));
                }
                let operations = parse_operations(&op_strings, child.line())?;
                let rule = FileRule {
                    raw_pattern: raw_pattern.clone(),
                    operations,
                    line: child.line(),
                };
                push_rule(config, child_tier, rule);
                found_tier = true;
            }
            other => {
                return Err(ConfigError::ParseError(format!(
                    "line {}: unexpected node \"{other}\" in path block \"{raw_pattern}\"; \
                     expected allow, deny, or ask",
                    child.line()
                )));
            }
        }
    }

    if !found_tier {
        return Err(ConfigError::ParseError(format!(
            "line {line}: path block \"{raw_pattern}\" has no allow/deny/ask children"
        )));
    }

    Ok(())
}

/// Parse operation strings into a `HashSet<FileOperation>`.
fn parse_operations(ops: &[&str], line: usize) -> Result<HashSet<FileOperation>, ConfigError> {
    let mut set = HashSet::new();
    for op in ops {
        let file_op = match *op {
            "read" => FileOperation::Read,
            "write" => FileOperation::Write,
            "edit" => FileOperation::Edit,
            "glob" => FileOperation::Glob,
            "grep" => FileOperation::Grep,
            unknown => {
                return Err(ConfigError::ParseError(format!(
                    "line {line}: unknown file operation \"{unknown}\"; \
                     expected read, write, edit, glob, or grep"
                )));
            }
        };
        set.insert(file_op);
    }
    Ok(set)
}

/// Push a rule into the correct tier vector.
fn push_rule(config: &mut FilesConfig, tier: &str, rule: FileRule) {
    match tier {
        "allow" => config.allow.push(rule),
        "deny" => config.deny.push(rule),
        "ask" => config.ask.push(rule),
        _ => unreachable!("tier validated before calling push_rule"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Helpers ---

    /// Parse raw KDL source into a FilesConfig, wrapping in a `files { }` section.
    fn parse_files_from_source(source: &str) -> Result<Option<FilesConfig>, ConfigError> {
        let wrapped = format!("files {{\n{source}\n}}");
        let doc = super::super::document::ConfigDocument::parse(&wrapped)?;
        parse_files(&doc)
    }

    /// Unwrap a successful parse that should return Some.
    fn files(source: &str) -> FilesConfig {
        parse_files_from_source(source)
            .expect("parse should succeed")
            .expect("files section should be present")
    }

    /// Parse and return the error string.
    fn files_err(source: &str) -> String {
        parse_files_from_source(source).unwrap_err().to_string()
    }

    fn ops_set(ops: &[FileOperation]) -> HashSet<FileOperation> {
        ops.iter().copied().collect()
    }

    // --- Flat one-liner tests ---

    #[test]
    fn flat_deny_rule() {
        let config = files(r#"deny "~/.ssh/**" "read" "write""#);
        assert_eq!(config.deny.len(), 1);
        assert_eq!(config.deny[0].raw_pattern, "~/.ssh/**");
        assert_eq!(
            config.deny[0].operations,
            ops_set(&[FileOperation::Read, FileOperation::Write])
        );
        assert!(config.allow.is_empty());
        assert!(config.ask.is_empty());
    }

    #[test]
    fn flat_allow_rule() {
        let config = files(r#"allow "/tmp/**" "read""#);
        assert_eq!(config.allow.len(), 1);
        assert_eq!(config.allow[0].raw_pattern, "/tmp/**");
        assert_eq!(config.allow[0].operations, ops_set(&[FileOperation::Read]));
        assert!(config.deny.is_empty());
        assert!(config.ask.is_empty());
    }

    #[test]
    fn flat_ask_rule() {
        let config = files(r#"ask "/**" "write" "edit""#);
        assert_eq!(config.ask.len(), 1);
        assert_eq!(config.ask[0].raw_pattern, "/**");
        assert_eq!(
            config.ask[0].operations,
            ops_set(&[FileOperation::Write, FileOperation::Edit])
        );
    }

    // --- Path-first block tests ---

    #[test]
    fn path_first_block_allow() {
        let config = files(r#""<cwd>/**" { allow "read" "write" }"#);
        assert_eq!(config.allow.len(), 1);
        assert_eq!(config.allow[0].raw_pattern, "<cwd>/**");
        assert_eq!(
            config.allow[0].operations,
            ops_set(&[FileOperation::Read, FileOperation::Write])
        );
        assert!(config.deny.is_empty());
        assert!(config.ask.is_empty());
    }

    // --- Mixed syntax ---

    #[test]
    fn mixed_flat_and_path_block() {
        let config = files(
            r#"
            deny "~/.ssh/**" "read" "write"
            "<cwd>/**" {
                allow "read" "write" "edit"
            }
            ask "/etc/**" "write"
            "#,
        );
        assert_eq!(config.deny.len(), 1);
        assert_eq!(config.deny[0].raw_pattern, "~/.ssh/**");
        assert_eq!(config.allow.len(), 1);
        assert_eq!(config.allow[0].raw_pattern, "<cwd>/**");
        assert_eq!(config.ask.len(), 1);
        assert_eq!(config.ask[0].raw_pattern, "/etc/**");
    }

    // --- Multiple tiers in one path block ---

    #[test]
    fn path_block_multiple_tiers() {
        let config = files(
            r#"
            "/path" {
                allow "read"
                deny "write"
            }
            "#,
        );
        assert_eq!(config.allow.len(), 1);
        assert_eq!(config.allow[0].raw_pattern, "/path");
        assert_eq!(config.allow[0].operations, ops_set(&[FileOperation::Read]));
        assert_eq!(config.deny.len(), 1);
        assert_eq!(config.deny[0].raw_pattern, "/path");
        assert_eq!(config.deny[0].operations, ops_set(&[FileOperation::Write]));
    }

    // --- Config without files section ---

    #[test]
    fn no_files_section_returns_none() {
        let doc = super::super::document::ConfigDocument::parse(
            r#"
            bash {
                allow "git" "cargo"
            }
            "#,
        )
        .unwrap();
        let result = parse_files(&doc).unwrap();
        assert!(result.is_none());
    }

    // --- Error: flat rule with no values ---

    #[test]
    fn error_flat_rule_no_values() {
        let err = files_err("deny");
        assert!(err.contains("requires a path pattern"), "got: {err}");
        assert!(err.contains("line"), "should include line info, got: {err}");
    }

    // --- Error: flat rule with only path, no operations ---

    #[test]
    fn error_flat_rule_path_only_no_operations() {
        let err = files_err(r#"deny "~/.ssh/**""#);
        assert!(
            err.contains("requires at least one operation"),
            "got: {err}"
        );
        assert!(
            err.contains("~/.ssh/**"),
            "should mention pattern, got: {err}"
        );
    }

    // --- Error: unknown operation name ---

    #[test]
    fn error_unknown_operation() {
        let err = files_err(r#"deny "~/.ssh/**" "read" "delete""#);
        assert!(err.contains("unknown file operation"), "got: {err}");
        assert!(
            err.contains("delete"),
            "should mention the bad op, got: {err}"
        );
        assert!(
            err.contains("line"),
            "should include line number, got: {err}"
        );
    }

    // --- Error: path-first block with no children ---

    #[test]
    fn error_path_block_no_children() {
        // KDL node without children block — parsed as bare node
        let err = files_err(r#""<cwd>/**""#);
        assert!(err.contains("requires a children block"), "got: {err}");
    }

    // --- Error: path-first block with empty tier children ---

    #[test]
    fn error_path_block_empty_children() {
        let err = files_err(r#""<cwd>/**" { }"#);
        assert!(err.contains("empty children block"), "got: {err}");
    }

    // --- Error: path block child tier with no operations ---

    #[test]
    fn error_path_block_tier_no_operations() {
        let err = files_err(
            r#"
            "<cwd>/**" {
                allow
            }
            "#,
        );
        assert!(
            err.contains("requires at least one operation"),
            "got: {err}"
        );
    }

    // --- Backwards compat: bash-only config, files is None ---

    #[test]
    fn backwards_compat_bash_only_config() {
        let doc = super::super::document::ConfigDocument::parse(
            r#"
            bash {
                allow "git"
                deny "rm"
            }
            "#,
        )
        .unwrap();
        let result = parse_files(&doc).unwrap();
        assert!(result.is_none());
    }

    // --- Multiple flat rules of the same tier merge ---

    #[test]
    fn multiple_flat_rules_same_tier_merge() {
        let config = files(
            r#"
            deny "~/.ssh/**" "read"
            deny "/etc/shadow" "read" "write"
            "#,
        );
        assert_eq!(config.deny.len(), 2);
        assert_eq!(config.deny[0].raw_pattern, "~/.ssh/**");
        assert_eq!(config.deny[0].operations, ops_set(&[FileOperation::Read]));
        assert_eq!(config.deny[1].raw_pattern, "/etc/shadow");
        assert_eq!(
            config.deny[1].operations,
            ops_set(&[FileOperation::Read, FileOperation::Write])
        );
    }

    // --- All five operations parse correctly ---

    #[test]
    fn all_operations_parse() {
        let config = files(r#"allow "/**" "read" "write" "edit" "glob" "grep""#);
        assert_eq!(config.allow.len(), 1);
        assert_eq!(
            config.allow[0].operations,
            ops_set(&[
                FileOperation::Read,
                FileOperation::Write,
                FileOperation::Edit,
                FileOperation::Glob,
                FileOperation::Grep,
            ])
        );
    }

    // --- Case-sensitive operation matching ---

    #[test]
    fn error_operation_case_sensitive() {
        let err = files_err(r#"allow "/tmp/**" "Read""#);
        assert!(err.contains("unknown file operation"), "got: {err}");
        assert!(
            err.contains("Read"),
            "should mention the bad op, got: {err}"
        );
    }

    // --- Line numbers in errors ---

    #[test]
    fn error_unknown_operation_includes_line_number() {
        let err = files_err(
            r#"
            allow "/tmp/**" "read"
            deny "~/.ssh/**" "badop"
            "#,
        );
        assert!(
            err.contains("line"),
            "should include line number, got: {err}"
        );
        assert!(
            err.contains("badop"),
            "should mention the bad op, got: {err}"
        );
    }

    // --- Path block with unexpected child node ---

    #[test]
    fn error_path_block_unexpected_child() {
        let err = files_err(
            r#"
            "<cwd>/**" {
                required-flags "read"
            }
            "#,
        );
        assert!(err.contains("unexpected node"), "got: {err}");
        assert!(
            err.contains("required-flags"),
            "should mention the bad node, got: {err}"
        );
    }

    // --- Non-string entry validation (fail-closed) ---

    #[test]
    fn error_flat_rule_non_string_entry() {
        let err = files_err(r#"deny 123 "read" "write""#);
        assert!(err.contains("non-string"), "got: {err}");
    }

    #[test]
    fn error_path_block_tier_non_string_entry() {
        let err = files_err(
            r#"
            "<cwd>/**" {
                allow 42
            }
            "#,
        );
        assert!(err.contains("non-string"), "got: {err}");
    }

    // --- Path block with inline values rejected ---

    #[test]
    fn error_path_block_with_inline_values() {
        let err = files_err(
            r#"
            "<cwd>/**" "read" {
                allow "write"
            }
            "#,
        );
        assert!(err.contains("inline values"), "got: {err}");
    }
}
