/// A file path that has been both extracted and normalized.
///
/// Domain value object — represents a validated, normalized path ready for
/// rule matching. Created at the protocol boundary during tool input parsing.
#[derive(Debug)]
pub struct ResolvedPath {
    /// Original path from tool_input (for display in reason messages).
    pub raw: String,
    /// Normalized absolute path (for config matching).
    pub normalized: String,
}
