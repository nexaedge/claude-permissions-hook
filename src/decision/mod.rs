mod aggregation;
mod bash;
mod files;
mod reason;

use crate::config::Config;
use crate::protocol::{HookInput, HookOutput, ToolUse};

pub(crate) const APP_NAME: &str = "claude-permissions-hook";

/// Evaluate a hook input against optional config and return a permission decision.
///
/// Returns `None` when the hook has no opinion (unrecognized tools, or all
/// programs/paths unlisted). Returns `Some(output)` with a concrete decision otherwise.
///
/// - No config → `Some(Ask)` for all tools (user needs to set up config)
/// - Bash tool → parse command, lookup programs, aggregate, apply mode
/// - File tool (Read/Write/Edit/Glob/Grep) → extract paths, lookup against file rules
/// - Other tool → `None` (no opinion)
///
/// # Examples
///
/// ```
/// use claude_permissions_hook::protocol::{HookInput, Decision};
/// use claude_permissions_hook::decision::evaluate;
///
/// let input: HookInput = serde_json::from_str(r#"{
///     "session_id": "s1",
///     "transcript_path": "/tmp/t.json",
///     "cwd": "/tmp",
///     "permission_mode": "default",
///     "hook_event_name": "PreToolUse",
///     "tool_name": "Bash",
///     "tool_input": {"command": "ls"},
///     "tool_use_id": "u1"
/// }"#).unwrap();
///
/// // No config → ask for everything
/// let output = evaluate(&input, None).unwrap();
/// assert_eq!(output.hook_specific_output.permission_decision, Decision::Ask);
/// ```
pub fn evaluate(input: &HookInput, config: Option<&Config>) -> Option<HookOutput> {
    // No config → ask for everything (user needs to set up config)
    let config = match config {
        Some(cfg) => cfg,
        None => {
            return Some(HookOutput::ask(
                "No config file provided — run with --config to enable rule-based decisions",
            ))
        }
    };

    let tool_use = ToolUse::parse(&input.tool_name, &input.tool_input);
    match &tool_use {
        ToolUse::Bash { command } => bash::evaluate_bash(command.as_deref(), input, config),
        ToolUse::Read { .. }
        | ToolUse::Write { .. }
        | ToolUse::Edit { .. }
        | ToolUse::Glob { .. }
        | ToolUse::Grep { .. } => files::evaluate_file_tool(&tool_use, input, config),
        ToolUse::Unknown { .. } => None,
    }
}

#[cfg(test)]
mod tests;
