//! File tool rule types and matching logic.

use std::collections::HashSet;

use crate::domain::Decision;
use crate::domain::FileOperation;

/// File configuration: a flat ordered list of rules.
///
/// Rules are evaluated in order by severity (deny > ask > allow).
pub(crate) type FilesConfig = Vec<FileRule>;

/// A single file rule binding a path pattern and decision to a set of operations.
///
/// Invariant: all fields are valid — expansion errors are caught at config load time.
#[derive(Debug)]
pub(crate) struct FileRule {
    /// The decision to apply when this rule matches.
    pub(crate) decision: Decision,
    /// The path pattern (raw and home-expanded).
    pub(crate) path: PathPattern,
    /// Which file operations this rule applies to.
    /// Empty means the rule applies to all operations.
    pub(crate) operations: HashSet<FileOperation>,
}

impl FileRule {
    /// Check whether a normalized path and operation satisfy this rule.
    ///
    /// Operation check: empty operations set means all operations match.
    /// Pattern expansion: `<cwd>` is replaced at match time.
    /// Glob errors fail toward the more restrictive outcome:
    /// deny/ask tiers treat errors as matching, allow tier treats errors as non-matching.
    pub(crate) fn matches(
        &self,
        normalized_path: &str,
        operation: FileOperation,
        cwd: &str,
    ) -> bool {
        // Check operation: empty set means all operations match
        if !self.operations.is_empty() && !self.operations.contains(&operation) {
            return false;
        }
        let expanded = self.path.expanded.replace("<cwd>", cwd);
        // For deny/ask decisions: fail-closed (error means match)
        // For allow: fail-open (error means no match)
        let error_means_match = self.decision != Decision::Allow;
        crate::domain::path::matches(normalized_path, &expanded).unwrap_or(error_means_match)
    }
}

/// A file path pattern with raw and home-expanded forms.
///
/// Invariant: `expanded` is always valid — `$HOME`-dependent patterns are
/// expanded at config load time. If expansion fails, config loading returns
/// an error rather than storing a deferred failure.
#[derive(Debug)]
pub(crate) struct PathPattern {
    /// Original pattern string (may contain `<cwd>`, `<home>`, `~`).
    ///
    /// Used in tests and error messages; the expanded form is used for matching.
    #[allow(dead_code)]
    pub(crate) raw: String,
    /// Pattern with `~` and `<home>` expanded at load time.
    ///
    /// `<cwd>` is **not** expanded here — that happens at match time.
    pub(crate) expanded: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(decision: Decision, expanded: &str, operations: &[FileOperation]) -> FileRule {
        FileRule {
            decision,
            path: PathPattern {
                raw: expanded.to_string(),
                expanded: expanded.to_string(),
            },
            operations: operations.iter().cloned().collect(),
        }
    }

    // --- FileRule::matches() tests ---

    #[test]
    fn matches_path_and_operation() {
        let rule = make_rule(Decision::Deny, "/etc/**", &[FileOperation::Read]);
        assert!(rule.matches("/etc/passwd", FileOperation::Read, "/"));
    }

    #[test]
    fn no_match_wrong_operation() {
        let rule = make_rule(Decision::Deny, "/etc/**", &[FileOperation::Read]);
        assert!(!rule.matches("/etc/passwd", FileOperation::Write, "/"));
    }

    #[test]
    fn no_match_wrong_path() {
        let rule = make_rule(Decision::Deny, "/etc/**", &[FileOperation::Read]);
        assert!(!rule.matches("/tmp/foo", FileOperation::Read, "/"));
    }

    #[test]
    fn empty_operations_matches_any() {
        let rule = make_rule(Decision::Deny, "/etc/**", &[]);
        assert!(rule.matches("/etc/passwd", FileOperation::Read, "/"));
        assert!(rule.matches("/etc/passwd", FileOperation::Write, "/"));
    }

    #[test]
    fn cwd_expansion_at_match_time() {
        let rule = make_rule(Decision::Allow, "<cwd>/**", &[FileOperation::Read]);
        assert!(rule.matches("/project/src/main.rs", FileOperation::Read, "/project"));
        assert!(!rule.matches("/other/file.rs", FileOperation::Read, "/project"));
    }
}
