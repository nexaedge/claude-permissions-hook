use super::ProgramName;

/// A parsed segment of a shell command, representing one program invocation.
///
/// Each segment has a program name (normalized to its basename) and a list
/// of arguments. This is a domain type — shell parsing that produces it
/// lives in the `command` adapter module.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandSegment {
    pub program: ProgramName,
    pub args: Vec<String>,
}
