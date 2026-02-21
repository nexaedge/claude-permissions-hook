//! File tool configuration types.
//!
//! Struct definitions and public lookup delegation. Parsing is in
//! [`crate::config::parse::files`]. Matching internals are in
//! [`crate::config::match_rule::files`].

use std::collections::HashSet;

use crate::domain::PathError;
use crate::protocol::Decision;
use crate::protocol::FileOperation;

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
    #[allow(dead_code)]
    pub raw_pattern: String,
    /// Pattern with `~` and `<home>` expanded at load time.
    ///
    /// `Err` when `$HOME` is not set and the pattern requires it.
    /// `<cwd>` is **not** expanded here — that happens at match time.
    pub home_expanded_pattern: Result<String, PathError>,
    /// Which file operations this rule applies to.
    pub operations: HashSet<FileOperation>,
    /// 1-based line number in the source file.
    #[allow(dead_code)]
    pub line: usize,
}

impl FilesConfig {
    /// Look up a normalized path and operation against file rules.
    ///
    /// Delegates to [`super::match_rule::files::lookup`].
    /// Precedence: deny > ask > allow. Returns `None` if no rule matches.
    pub fn lookup(
        &self,
        normalized_path: &str,
        operation: FileOperation,
        cwd: &str,
    ) -> Option<Decision> {
        super::match_rule::files::lookup(self, normalized_path, operation, cwd)
    }
}
