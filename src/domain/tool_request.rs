use super::file_target::FileTarget;
use super::{CommandSegment, FileOperation};

/// Domain representation of a tool invocation to be evaluated.
///
/// Built from protocol types at the boundary. The decision layer matches
/// on this enum without any protocol dependencies.
///
/// Only represents tools the hook knows how to evaluate. Unknown tools
/// and parse errors are handled at the protocol/CLI boundary before
/// reaching the decision engine.
#[derive(Debug)]
pub enum ToolRequest {
    /// Bash command with parsed program segments.
    Bash { segments: Vec<CommandSegment> },
    /// File tool with resolved targets and operation type.
    File {
        operation: FileOperation,
        targets: Vec<FileTarget>,
    },
}
