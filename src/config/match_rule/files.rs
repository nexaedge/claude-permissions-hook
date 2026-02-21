use crate::config::files::{FileRule, FilesConfig};
use crate::protocol::Decision;
use crate::protocol::FileOperation;

/// Look up a normalized path and operation against file rules.
///
/// Checks tiers in order: deny → ask → allow. First matching tier wins.
/// Returns `None` if no rule in any tier matches.
///
/// If any rule for the given operation has a pattern that failed `$HOME`
/// expansion (e.g., `$HOME` is not set), the decision is fail-closed `Ask`
/// regardless of tier, preventing silent `deny` from an unresolvable pattern.
///
/// Invalid glob patterns fail toward the more restrictive outcome:
/// deny/ask tiers treat errors as matching, allow tier treats errors as non-matching.
pub fn lookup(
    config: &FilesConfig,
    normalized_path: &str,
    operation: FileOperation,
    cwd: &str,
) -> Option<Decision> {
    // Fail-closed: if any rule for this operation has an expansion error
    // (e.g., $HOME not set), return Ask unconditionally to avoid silent deny.
    if has_expansion_error(&config.deny, operation)
        || has_expansion_error(&config.ask, operation)
        || has_expansion_error(&config.allow, operation)
    {
        return Some(Decision::Ask);
    }
    if matches_any_rule(&config.deny, normalized_path, operation, cwd, true) {
        return Some(Decision::Deny);
    }
    if matches_any_rule(&config.ask, normalized_path, operation, cwd, true) {
        return Some(Decision::Ask);
    }
    if matches_any_rule(&config.allow, normalized_path, operation, cwd, false) {
        return Some(Decision::Allow);
    }
    None
}

/// Returns `true` if any rule in the tier for the given operation has a
/// pattern that failed home expansion.
fn has_expansion_error(rules: &[FileRule], operation: FileOperation) -> bool {
    rules
        .iter()
        .any(|rule| rule.operations.contains(&operation) && rule.home_expanded_pattern.is_err())
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
        let home_expanded = match &rule.home_expanded_pattern {
            Ok(p) => p,
            Err(_) => return error_means_match,
        };
        let expanded = home_expanded.replace("<cwd>", cwd);
        crate::path::matches(normalized_path, &expanded).unwrap_or(error_means_match)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::document::ConfigDocument;
    use crate::config::files::FilesConfig;
    use crate::config::parse::files::parse_files;
    use crate::config::ConfigError;

    fn parse_files_from_source(source: &str) -> Result<Option<FilesConfig>, ConfigError> {
        let wrapped = format!("files {{\n{source}\n}}");
        let doc = ConfigDocument::parse(&wrapped)?;
        parse_files(&doc)
    }

    fn files(source: &str) -> FilesConfig {
        parse_files_from_source(source)
            .expect("parse should succeed")
            .expect("files section should be present")
    }

    // --- Lookup tests ---

    #[test]
    fn lookup_deny_matches() {
        let config = files(r#"deny "/etc/**" "read""#);
        let result = lookup(&config, "/etc/passwd", FileOperation::Read, "/");
        assert_eq!(result, Some(Decision::Deny));
    }

    #[test]
    fn lookup_allow_matches() {
        let config = files(r#"allow "/tmp/**" "read""#);
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Read, "/");
        assert_eq!(result, Some(Decision::Allow));
    }

    #[test]
    fn lookup_ask_matches() {
        let config = files(r#"ask "/etc/**" "write""#);
        let result = lookup(&config, "/etc/hosts", FileOperation::Write, "/");
        assert_eq!(result, Some(Decision::Ask));
    }

    #[test]
    fn lookup_deny_wins_over_allow() {
        let config = files(
            r#"
            deny "/etc/**" "read"
            allow "/etc/**" "read"
            "#,
        );
        let result = lookup(&config, "/etc/hosts", FileOperation::Read, "/");
        assert_eq!(result, Some(Decision::Deny));
    }

    #[test]
    fn lookup_no_match_returns_none() {
        let config = files(r#"deny "~/.ssh/**" "read""#);
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Read, "/");
        assert_eq!(result, None);
    }

    #[test]
    fn lookup_wrong_operation_returns_none() {
        let config = files(r#"deny "/tmp/**" "read""#);
        // write is not denied
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Write, "/");
        assert_eq!(result, None);
    }

    #[test]
    fn lookup_cwd_expansion() {
        let config = files(r#"allow "<cwd>/**" "read""#);
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
        let config = files(r#"allow "<cwd>/**" "read""#);
        let result = lookup(&config, "/other/file.rs", FileOperation::Read, "/project");
        assert_eq!(result, None);
    }

    // --- Expansion error → fail-closed Ask ---

    fn rule_with_expansion_error(operations: &[FileOperation]) -> FileRule {
        FileRule {
            raw_pattern: "~/.ssh/**".to_string(),
            home_expanded_pattern: Err(crate::domain::PathError::HomeNotSet("$HOME".to_string())),
            operations: operations.iter().cloned().collect(),
            line: 1,
        }
    }

    #[test]
    fn lookup_expansion_error_in_deny_tier_returns_ask() {
        let config = FilesConfig {
            deny: vec![rule_with_expansion_error(&[FileOperation::Read])],
            ask: vec![],
            allow: vec![],
        };
        // Even for an unrelated path, expansion error forces Ask (fail-closed).
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Read, "/tmp");
        assert_eq!(result, Some(Decision::Ask));
    }

    #[test]
    fn lookup_expansion_error_only_for_other_operation_does_not_affect_lookup() {
        let config = FilesConfig {
            // Error rule is for Write, but we're looking up Read — no interference.
            deny: vec![rule_with_expansion_error(&[FileOperation::Write])],
            ask: vec![],
            allow: vec![],
        };
        let result = lookup(&config, "/tmp/foo.txt", FileOperation::Read, "/tmp");
        assert_eq!(result, None);
    }
}
