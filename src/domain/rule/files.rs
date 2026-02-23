//! File tool rule types.

use std::collections::HashSet;

use crate::domain::Decision;
use crate::domain::FileOperation;

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
