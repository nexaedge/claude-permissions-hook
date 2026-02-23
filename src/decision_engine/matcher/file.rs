use crate::domain::rule::files::FileRule;
use crate::domain::{Decision, FileOperation};

/// Check whether a normalized path and operation satisfy a file rule.
///
/// Operation check: empty operations set means all operations match.
/// Pattern expansion: `<cwd>` is replaced at match time.
/// Glob errors fail toward the more restrictive outcome:
/// deny/ask tiers treat errors as matching, allow tier treats errors as non-matching.
pub(crate) fn matches(
    rule: &FileRule,
    normalized_path: &str,
    operation: FileOperation,
    cwd: &str,
) -> bool {
    // Check operation: empty set means all operations match
    if !rule.operations.is_empty() && !rule.operations.contains(&operation) {
        return false;
    }
    let expanded = rule.path.expanded.replace("<cwd>", cwd);
    // For deny/ask decisions: fail-closed (error means match)
    // For allow: fail-open (error means no match)
    let error_means_match = rule.decision != Decision::Allow;
    crate::domain::path::matches(normalized_path, &expanded).unwrap_or(error_means_match)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::rule::files::PathPattern;

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

    #[test]
    fn matches_path_and_operation() {
        let rule = make_rule(Decision::Deny, "/etc/**", &[FileOperation::Read]);
        assert!(matches(&rule, "/etc/passwd", FileOperation::Read, "/"));
    }

    #[test]
    fn no_match_wrong_operation() {
        let rule = make_rule(Decision::Deny, "/etc/**", &[FileOperation::Read]);
        assert!(!matches(&rule, "/etc/passwd", FileOperation::Write, "/"));
    }

    #[test]
    fn no_match_wrong_path() {
        let rule = make_rule(Decision::Deny, "/etc/**", &[FileOperation::Read]);
        assert!(!matches(&rule, "/tmp/foo", FileOperation::Read, "/"));
    }

    #[test]
    fn empty_operations_matches_any() {
        let rule = make_rule(Decision::Deny, "/etc/**", &[]);
        assert!(matches(&rule, "/etc/passwd", FileOperation::Read, "/"));
        assert!(matches(&rule, "/etc/passwd", FileOperation::Write, "/"));
    }

    #[test]
    fn cwd_expansion_at_match_time() {
        let rule = make_rule(Decision::Allow, "<cwd>/**", &[FileOperation::Read]);
        assert!(matches(
            &rule,
            "/project/src/main.rs",
            FileOperation::Read,
            "/project"
        ));
        assert!(!matches(
            &rule,
            "/other/file.rs",
            FileOperation::Read,
            "/project"
        ));
    }
}
