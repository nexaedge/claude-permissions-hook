/// Identifies which policy category a tool belongs to.
///
/// Replaces the old `ToolCategory` enum with a name that better
/// reflects its purpose: selecting a subset of policy rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicySet {
    Bash,
    File,
}
