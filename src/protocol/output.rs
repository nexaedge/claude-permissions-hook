use serde::Serialize;

/// The output returned to Claude Code on stdout.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOutput {
    pub hook_specific_output: PreToolUseOutput,
}

/// PreToolUse-specific output containing the permission decision.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseOutput {
    pub permission_decision: Decision,
    pub permission_decision_reason: String,
}

/// The permission decision: allow, ask, or deny.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}
