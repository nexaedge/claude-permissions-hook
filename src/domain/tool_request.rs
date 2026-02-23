use super::{CommandSegment, FileOperation, ResolvedPath, ToolCategory};

/// Domain representation of a tool invocation to be evaluated.
///
/// Built from protocol types at the boundary. The decision layer matches
/// on this enum without any protocol dependencies.
#[derive(Debug)]
pub enum ToolRequest {
    /// Bash command with parsed program segments.
    Bash { segments: Vec<CommandSegment> },
    /// File tool with resolved paths and operation type.
    File {
        operation: FileOperation,
        paths: Vec<ResolvedPath>,
    },
    /// Unrecognized tool — hook has no opinion.
    Unknown,
    /// Known tool with invalid input — carries category for fail-closed gating.
    ParseError {
        category: ToolCategory,
        reason: String,
    },
}
