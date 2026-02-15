use serde::Deserialize;
use serde_json::Value;

/// The input received from Claude Code on stdin for a PreToolUse hook.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInput {
    pub session_id: String,
    pub cwd: String,
    pub permission_mode: PermissionMode,
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_use_id: String,
}

/// Claude Code's permission modes.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    Plan,
    AcceptEdits,
    DontAsk,
    BypassPermissions,
}
