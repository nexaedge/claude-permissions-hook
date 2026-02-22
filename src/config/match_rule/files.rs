use crate::config::files::{FileOperation, FileRule};
use crate::protocol::Decision;

/// Look up a normalized path and operation against file rules.
///
/// Returns the most restrictive (highest severity) decision among all matching rules.
/// Returns `None` if no rule matches.
///
/// If any rule for the given operation has a pattern that failed `$HOME`
/// expansion (e.g., `$HOME` is not set), the decision is fail-closed `Ask`
/// regardless of tier, preventing silent `deny` from an unresolvable pattern.
///
/// Invalid glob patterns fail toward the more restrictive outcome:
/// deny/ask tiers treat errors as matching, allow tier treats errors as non-matching.
pub fn lookup(
    rules: &[FileRule],
    normalized_path: &str,
    operation: FileOperation,
    cwd: &str,
) -> Option<Decision> {
    // Fail-closed: if any rule for this operation has an expansion error
    // (e.g., $HOME not set), return Ask unconditionally to avoid silent deny.
    if has_expansion_error(rules, operation) {
        return Some(Decision::Ask);
    }

    rules
        .iter()
        .filter(|r| rule_matches(r, normalized_path, operation, cwd))
        .map(|r| r.decision.clone())
        .max_by_key(|d| d.severity())
}

/// Returns `true` if any rule for the given operation has a
/// pattern that failed home expansion.
fn has_expansion_error(rules: &[FileRule], operation: FileOperation) -> bool {
    rules.iter().any(|rule| {
        (rule.operations.is_empty() || rule.operations.contains(&operation))
            && rule.path.expanded.is_err()
    })
}

/// Check if a single rule matches the given path and operation.
fn rule_matches(
    rule: &FileRule,
    normalized_path: &str,
    operation: FileOperation,
    cwd: &str,
) -> bool {
    // Check operation: empty set means all operations match
    if !rule.operations.is_empty() && !rule.operations.contains(&operation) {
        return false;
    }
    let home_expanded = match &rule.path.expanded {
        Ok(p) => p,
        Err(_) => return false, // expansion error already handled above
    };
    let expanded = home_expanded.replace("<cwd>", cwd);
    // For deny/ask decisions: fail-closed (error means match)
    // For allow: fail-open (error means no match)
    let error_means_match = rule.decision != Decision::Allow;
    crate::path::matches(normalized_path, &expanded).unwrap_or(error_means_match)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::files::FilesConfig;
    use crate::config::parse::files::parse_files;
    use crate::config::ConfigError;

    fn parse_files_from_source(source: &str) -> Result<Option<FilesConfig>, ConfigError> {
        let wrapped = format!("files {{\n{source}\n}}");
        let doc = crate::config::document::ConfigDocument::parse(&wrapped)?;
        parse_files(&crate::config::parse::section_to_config_nodes(&doc))
    }

    fn files(source: &str) -> FilesConfig {
        parse_files_from_source(source)
            .expect("parse should succeed")
            .expect("nodes should produce rules")
    }

    // --- Lookup tests ---

    #[test]
    fn lookup_deny_matches() {
        let config = files(r#"deny "/etc/**" { operations "read" }"#);
        let result = lookup(&config, "/etc/passwd", FileOperation::Read, "/");
        assert_eq!(result, Some(Decision::Deny));
    }

    #[test]
    fn lookup_allow_matches() {
        let config = files(r#"allow "/tmp/**" { operations "read" }"#);
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Read, "/");
        assert_eq!(result, Some(Decision::Allow));
    }

    #[test]
    fn lookup_ask_matches() {
        let config = files(r#"ask "/etc/**" { operations "write" }"#);
        let result = lookup(&config, "/etc/hosts", FileOperation::Write, "/");
        assert_eq!(result, Some(Decision::Ask));
    }

    #[test]
    fn lookup_deny_wins_over_allow() {
        let config = files(
            r#"
            deny "/etc/**" { operations "read" }
            allow "/etc/**" { operations "read" }
            "#,
        );
        let result = lookup(&config, "/etc/hosts", FileOperation::Read, "/");
        assert_eq!(result, Some(Decision::Deny));
    }

    #[test]
    fn lookup_no_match_returns_none() {
        let config = files(r#"deny "~/.ssh/**" { operations "read" }"#);
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Read, "/");
        assert_eq!(result, None);
    }

    #[test]
    fn lookup_wrong_operation_returns_none() {
        let config = files(r#"deny "/tmp/**" { operations "read" }"#);
        // write is not denied
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Write, "/");
        assert_eq!(result, None);
    }

    #[test]
    fn lookup_cwd_expansion() {
        let config = files(r#"allow "<cwd>/**" { operations "read" }"#);
        let result = lookup(
            &config,
            "/project/src/main.rs",
            FileOperation::Read,
            "/project",
        );
        assert_eq!(result, Some(Decision::Allow));
    }

    #[test]
    fn lookup_cwd_expansion_outside_cwd() {
        let config = files(r#"allow "<cwd>/**" { operations "read" }"#);
        let result = lookup(&config, "/other/file.rs", FileOperation::Read, "/project");
        assert_eq!(result, None);
    }

    // --- Expansion error → fail-closed Ask ---

    fn rule_with_expansion_error(decision: Decision, operations: &[FileOperation]) -> FileRule {
        FileRule {
            decision,
            path: crate::config::files::PathPattern {
                raw: "~/.ssh/**".to_string(),
                expanded: Err(crate::domain::PathError::HomeNotSet("$HOME".to_string())),
            },
            operations: operations.iter().cloned().collect(),
        }
    }

    #[test]
    fn lookup_expansion_error_in_deny_tier_returns_ask() {
        let config: FilesConfig = vec![rule_with_expansion_error(
            Decision::Deny,
            &[FileOperation::Read],
        )];
        // Even for an unrelated path, expansion error forces Ask (fail-closed).
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Read, "/tmp");
        assert_eq!(result, Some(Decision::Ask));
    }

    #[test]
    fn lookup_expansion_error_only_for_other_operation_does_not_affect_lookup() {
        // Error rule is for Write, but we're looking up Read — no interference.
        let config: FilesConfig = vec![rule_with_expansion_error(
            Decision::Deny,
            &[FileOperation::Write],
        )];
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Read, "/tmp");
        assert_eq!(result, None);
    }

    // --- All operations (empty set) ---

    #[test]
    fn lookup_empty_operations_matches_any_operation() {
        let config = files(r#"deny "/etc/**""#);
        // Empty operations = all operations
        let result = lookup(&config, "/etc/passwd", FileOperation::Read, "/");
        assert_eq!(result, Some(Decision::Deny));
        let result2 = lookup(&config, "/etc/passwd", FileOperation::Write, "/");
        assert_eq!(result2, Some(Decision::Deny));
    }
}
