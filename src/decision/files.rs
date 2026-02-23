use crate::config::Config;
use crate::domain::rule::files::FileRule;
use crate::domain::Decision;
use crate::domain::FileOperation;
use crate::domain::FileTarget;
use crate::domain::PermissionMode;

use super::aggregation::{aggregate_decisions, apply_mode_modifier};
use super::reason::build_file_reason;

/// Look up a normalized path and operation against file rules.
///
/// Returns the most restrictive (highest severity) decision among all matching rules.
/// Returns `None` if no rule matches.
fn lookup(
    rules: &[FileRule],
    normalized_path: &str,
    operation: FileOperation,
    cwd: &str,
) -> Option<Decision> {
    rules
        .iter()
        .filter(|r| r.matches(normalized_path, operation, cwd))
        .map(|r| r.decision.clone())
        .max_by_key(|d| d.severity())
}

/// Evaluate a file tool invocation against file config rules.
///
/// Receives already-resolved file targets with normalized absolute paths.
/// Only performs config matching — no validation, parsing, or normalization.
/// Returns the final decision and a human-readable reason string.
pub(super) fn evaluate_file_tool(
    operation: FileOperation,
    targets: &[FileTarget],
    cwd: &str,
    permission_mode: &PermissionMode,
    config: &Config,
) -> Option<(Decision, String)> {
    if config.files.is_empty() {
        return None;
    }
    let files_config = &config.files;

    let per_path: Vec<Option<Decision>> = targets
        .iter()
        .map(|target| {
            lookup(
                files_config,
                &target.normalized_path.to_string_lossy(),
                operation,
                cwd,
            )
        })
        .collect();

    let aggregated = aggregate_decisions(&per_path);

    aggregated.map(|decision| {
        let modified = apply_mode_modifier(decision.clone(), permission_mode);
        let op_str = &operation.to_string();
        let raw_paths: Vec<&str> = targets.iter().map(|t| t.raw_path.as_str()).collect();
        let reason = build_file_reason(&modified, &raw_paths, &per_path, &decision, op_str);
        (modified, reason)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(source: &str) -> Vec<FileRule> {
        let wrapped = format!("files {{\n{source}\n}}");
        let config = crate::config::Config::parse(&wrapped).expect("parse should succeed");
        config.files
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
