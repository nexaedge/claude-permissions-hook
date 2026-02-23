use std::collections::HashSet;

use crate::config::ConfigError;
use crate::domain::rule::files::{FileRule, PathPattern};
use crate::domain::FileOperation;

use super::{parse_tier, ConfigNode};

/// Parse the `files` section out of a top-level config node slice.
///
/// Returns `None` when the section is absent or empty.
///
/// Grammar:
/// ```kdl
/// files {
///     allow "~/.config/**"                           // empty operations = all operations
///     deny "~/.ssh/**" { operations "read" "write" } // specific operations in body
///     deny "path1" "path2" { operations "read" }     // multiple paths, shared operations
/// }
/// ```
pub(in crate::config) fn parse_files(
    config_nodes: &[ConfigNode],
) -> Result<Option<Vec<FileRule>>, ConfigError> {
    match ConfigNode::body_of(config_nodes, "files") {
        Some(rule_nodes) => parse_file_nodes(rule_nodes).map(Some),
        None => Ok(None),
    }
}

fn parse_file_nodes(nodes: &[ConfigNode]) -> Result<Vec<FileRule>, ConfigError> {
    let mut rules = Vec::new();
    for node in nodes {
        let decision = parse_tier(&node.name, node.line)?;

        if node.arguments.is_empty() {
            return Err(ConfigError::ParseError(format!(
                "line {}: {} node has no path entries",
                node.line, node.name
            )));
        }

        let operations = parse_operations_from_body(node)?;

        for raw_path in &node.arguments {
            let expanded = crate::config::normalize::files::expand_home(raw_path).map_err(|e| {
                ConfigError::ParseError(format!(
                    "line {}: failed to expand path \"{raw_path}\": {e}",
                    node.line
                ))
            })?;
            let path = PathPattern {
                raw: raw_path.clone(),
                expanded,
            };
            rules.push(FileRule {
                decision: decision.clone(),
                path,
                operations: operations.clone(),
            });
        }
    }
    Ok(rules)
}

fn parse_operations_from_body(node: &ConfigNode) -> Result<HashSet<FileOperation>, ConfigError> {
    let mut ops = HashSet::new();
    for child in node.body_nodes() {
        match child.name.as_str() {
            "operations" => {
                for v in &child.arguments {
                    let op = match v.as_str() {
                        "read" => FileOperation::Read,
                        "write" => FileOperation::Write,
                        "edit" => FileOperation::Edit,
                        "glob" => FileOperation::Glob,
                        "grep" => FileOperation::Grep,
                        unknown => {
                            return Err(ConfigError::ParseError(format!(
                                "line {}: unknown file operation \"{unknown}\"; expected read, write, edit, glob, or grep",
                                child.line
                            )));
                        }
                    };
                    ops.insert(op);
                }
            }
            other => {
                return Err(ConfigError::ParseError(format!(
                    "line {}: unexpected node \"{other}\" in files rule; expected operations",
                    child.line
                )));
            }
        }
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FileOperation;

    fn parse_files_from_source(source: &str) -> Result<Option<Vec<FileRule>>, ConfigError> {
        let wrapped = format!("files {{\n{source}\n}}");
        let doc = crate::config::document::ConfigDocument::parse(&wrapped)?;
        parse_files(&crate::config::parse::section_to_config_nodes(&doc))
    }

    fn files(source: &str) -> Vec<FileRule> {
        parse_files_from_source(source)
            .expect("parse should succeed")
            .expect("nodes should produce rules")
    }

    fn files_err(source: &str) -> String {
        parse_files_from_source(source).unwrap_err().to_string()
    }

    fn ops_set(ops: &[FileOperation]) -> HashSet<FileOperation> {
        ops.iter().copied().collect()
    }

    // --- Simple deny/allow/ask rules ---

    #[test]
    fn deny_rule_with_operations_in_body() {
        let config = files(r#"deny "~/.ssh/**" { operations "read" "write" }"#);
        assert_eq!(config.len(), 1);
        assert_eq!(config[0].path.raw, "~/.ssh/**");
        assert_eq!(
            config[0].operations,
            ops_set(&[FileOperation::Read, FileOperation::Write])
        );
    }

    #[test]
    fn allow_rule_no_body_empty_operations() {
        let config = files(r#"allow "~/.config/**""#);
        assert_eq!(config.len(), 1);
        assert_eq!(config[0].path.raw, "~/.config/**");
        assert!(config[0].operations.is_empty());
    }

    #[test]
    fn ask_rule_with_operations() {
        let config = files(r#"ask "/**" { operations "write" "edit" }"#);
        assert_eq!(config.len(), 1);
        assert_eq!(config[0].path.raw, "/**");
        assert_eq!(
            config[0].operations,
            ops_set(&[FileOperation::Write, FileOperation::Edit])
        );
    }

    // --- Multiple paths in one node ---

    #[test]
    fn multiple_paths_same_node_creates_multiple_rules() {
        let config = files(r#"deny "path1" "path2" { operations "read" }"#);
        assert_eq!(config.len(), 2);
        assert_eq!(config[0].path.raw, "path1");
        assert_eq!(config[1].path.raw, "path2");
        assert_eq!(config[0].operations, ops_set(&[FileOperation::Read]));
        assert_eq!(config[1].operations, ops_set(&[FileOperation::Read]));
    }

    // --- Mixed rules ---

    #[test]
    fn mixed_rules() {
        let config = files(
            r#"
            deny "~/.ssh/**" { operations "read" "write" }
            allow "<cwd>/**" { operations "read" "write" "edit" }
            ask "/etc/**" { operations "write" }
            "#,
        );
        assert_eq!(config.len(), 3);
    }

    // --- All five operations parse correctly ---

    #[test]
    fn all_operations_parse() {
        let config = files(r#"allow "/**" { operations "read" "write" "edit" "glob" "grep" }"#);
        assert_eq!(config.len(), 1);
        assert_eq!(
            config[0].operations,
            ops_set(&[
                FileOperation::Read,
                FileOperation::Write,
                FileOperation::Edit,
                FileOperation::Glob,
                FileOperation::Grep,
            ])
        );
    }

    // --- Error: node with no paths ---

    #[test]
    fn error_node_no_paths() {
        let err = files_err("deny");
        assert!(err.contains("no path entries"), "got: {err}");
        assert!(err.contains("line"), "should include line info, got: {err}");
    }

    // --- Error: unknown operation name ---

    #[test]
    fn error_unknown_operation() {
        let err = files_err(r#"deny "~/.ssh/**" { operations "read" "delete" }"#);
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

    // --- Error: unknown body node ---

    #[test]
    fn error_unexpected_body_node() {
        let err = files_err(r#"deny "~/.ssh/**" { required-flags "read" }"#);
        assert!(err.contains("unexpected node"), "got: {err}");
        assert!(
            err.contains("required-flags"),
            "should mention the bad node, got: {err}"
        );
    }

    // --- Error: unknown tier ---

    #[test]
    fn error_unknown_tier() {
        let err = files_err(r#"permit "~/.ssh/**""#);
        assert!(err.contains("unknown tier"), "got: {err}");
    }

    // --- Case-sensitive operation matching ---

    #[test]
    fn error_operation_case_sensitive() {
        let err = files_err(r#"allow "/tmp/**" { operations "Read" }"#);
        assert!(err.contains("unknown file operation"), "got: {err}");
        assert!(
            err.contains("Read"),
            "should mention the bad op, got: {err}"
        );
    }

    // --- Multiple rules of the same tier ---

    #[test]
    fn multiple_rules_same_tier() {
        let config = files(
            r#"
            deny "~/.ssh/**" { operations "read" }
            deny "/etc/shadow" { operations "read" "write" }
            "#,
        );
        assert_eq!(config.len(), 2);
        assert_eq!(config[0].path.raw, "~/.ssh/**");
        assert_eq!(config[0].operations, ops_set(&[FileOperation::Read]));
        assert_eq!(config[1].path.raw, "/etc/shadow");
        assert_eq!(
            config[1].operations,
            ops_set(&[FileOperation::Read, FileOperation::Write])
        );
    }

    // --- Home expansion ---

    #[test]
    fn tilde_home_expansion() {
        let home = std::env::var("HOME").unwrap();
        let config = files(r#"deny "~/.ssh/**""#);
        assert_eq!(config[0].path.expanded, format!("{home}/.ssh/**"));
    }
}
