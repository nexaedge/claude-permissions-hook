//! File tool configuration types.
//!
//! Struct definitions and public lookup delegation. Parsing is in
//! [`crate::config::parse::files`]. Matching internals are in
//! [`crate::config::match_rule::files`].

use std::collections::HashSet;

use crate::domain::PathError;
use crate::protocol::Decision;

pub use crate::domain::FileOperation;

/// File configuration: a flat ordered list of rules.
///
/// Rules are evaluated in order by severity (deny > ask > allow).
pub type FilesConfig = Vec<FileRule>;

/// A single file rule binding a path pattern and decision to a set of operations.
#[derive(Debug)]
pub struct FileRule {
    /// The decision to apply when this rule matches.
    pub decision: Decision,
    /// The path pattern (raw and home-expanded).
    pub path: PathPattern,
    /// Which file operations this rule applies to.
    /// Empty means the rule applies to all operations.
    pub operations: HashSet<FileOperation>,
}

/// A file path pattern with raw and home-expanded forms.
#[derive(Debug)]
pub struct PathPattern {
    /// Original pattern string (may contain `<cwd>`, `<home>`, `~`).
    ///
    /// Used in tests and error messages; the expanded form is used for matching.
    #[allow(dead_code)]
    pub raw: String,
    /// Pattern with `~` and `<home>` expanded at load time.
    ///
    /// `Err` when `$HOME` is not set and the pattern requires it.
    /// `<cwd>` is **not** expanded here — that happens at match time.
    pub expanded: Result<String, PathError>,
}

/// Look up a normalized path and operation against file rules.
///
/// Uses severity ordering: deny > ask > allow. Returns the most restrictive
/// decision among all matching rules. Returns `None` if no rule matches.
///
/// If any rule for the given operation has a pattern that failed `$HOME`
/// expansion (e.g., `$HOME` is not set), the decision is fail-closed `Ask`
/// regardless of tier, preventing silent `deny` from an unresolvable pattern.
pub fn lookup(
    rules: &[FileRule],
    normalized_path: &str,
    operation: FileOperation,
    cwd: &str,
) -> Option<Decision> {
    super::match_rule::files::lookup(rules, normalized_path, operation, cwd)
}
