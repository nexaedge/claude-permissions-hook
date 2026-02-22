mod aggregation;
mod bash;
mod files;
mod reason;

use crate::config::Config;
use crate::protocol::{HookInput, HookOutput, ToolCategory, ToolUse};

pub(crate) const APP_NAME: &str = "claude-permissions-hook";

/// Evaluate a hook input against optional config and return a permission decision.
///
/// Returns `None` when the hook has no opinion (unrecognized tools, or all
/// programs/paths unlisted). Returns `Some(output)` with a concrete decision otherwise.
///
/// - No config → `Some(Ask)` for all tools (user needs to set up config)
/// - Bash tool → lookup programs, aggregate, apply mode
/// - File tool (Read/Write/Edit/Glob/Grep) → lookup paths against file rules
/// - Invalid tool input → `Some(Ask)` if relevant config exists (fail-closed)
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

    match &input.tool_use {
        Ok(ToolUse::Bash(ref bash)) => bash::evaluate_bash(bash, input, config),
        Ok(
            ToolUse::Read(ref file)
            | ToolUse::Write(ref file)
            | ToolUse::Edit(ref file)
            | ToolUse::Glob(ref file)
            | ToolUse::Grep(ref file),
        ) => files::evaluate_file_tool(input.tool_use.as_ref().unwrap(), file, input, config),
        Ok(ToolUse::Unknown { .. }) => None,
        // Fail-closed: invalid input for a known tool → ask, but only if we have
        // config for that tool category. Without config, we have no opinion.
        Err(err) => {
            let has_config = match err.category {
                ToolCategory::Bash => config.bash.is_some(),
                ToolCategory::File => config.files.is_some(),
            };
            if has_config {
                Some(HookOutput::ask(&err.reason))
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests;
