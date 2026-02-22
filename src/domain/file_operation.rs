/// Identifies which file tool operation is being performed.
///
/// A domain concept shared across protocol and config layers.
/// The protocol preserves per-tool identity via `ToolUse` variants;
/// this enum is used by config rules for operation-scoped matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileOperation {
    Read,
    Write,
    Edit,
    Glob,
    Grep,
}
