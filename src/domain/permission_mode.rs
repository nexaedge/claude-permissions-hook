/// Claude Code's permission modes.
///
/// Pure domain type — deserialization is handled by the protocol layer.
#[derive(Debug, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    Plan,
    AcceptEdits,
    DontAsk,
    BypassPermissions,
}
