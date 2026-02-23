use std::path::PathBuf;

/// A resolved file path target for rule matching.
///
/// Replaces the old `ResolvedPath` with richer context and `PathBuf` types
/// per ARCHITECTURE.md convention. `raw_path` stays as `String` since it's
/// the display form, not a filesystem path.
#[derive(Debug, Clone)]
pub struct FileTarget {
    /// Original path from tool_input (for display in reason messages).
    pub raw_path: String,
    /// Normalized absolute path (for config matching).
    pub normalized_path: PathBuf,
    /// Working directory at the time of the request.
    pub cwd: PathBuf,
    /// Project root directory.
    pub project_path: PathBuf,
}
