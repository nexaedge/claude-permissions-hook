mod aggregation;
mod bash;
mod files;
mod reason;

use crate::config::Config;
use crate::domain::{Decision, PermissionMode, ToolCategory, ToolRequest};

pub(crate) const APP_NAME: &str = "claude-permissions-hook";

/// Evaluate a tool request against config rules and return a permission decision.
///
/// Takes domain types only — the caller maps from protocol types (e.g., `HookInput`)
/// to `ToolRequest` at the boundary.
///
/// Returns `None` when the hook has no opinion (unrecognized tools, or all
/// programs/paths unlisted). Returns `Some((decision, reason))` with a
/// concrete decision otherwise.
///
/// - Bash tool → lookup programs, aggregate, apply mode
/// - File tool (Read/Write/Edit/Glob/Grep) → lookup paths against file rules
/// - Invalid tool input → `Some(Ask)` if relevant config exists (fail-closed)
/// - Other tool → `None` (no opinion)
///
/// The caller is responsible for handling the no-config case (e.g., defaulting
/// to "ask" with a user-facing message).
///
/// # Examples
///
/// ```
/// use claude_permissions_hook::protocol::HookInput;
/// use claude_permissions_hook::domain::Decision;
/// use claude_permissions_hook::config::Config;
/// use claude_permissions_hook::decision::evaluate;
///
/// let input: HookInput = serde_json::from_str(r#"{
///     "session_id": "s1",
///     "transcript_path": "/tmp/t.json",
///     "cwd": "/tmp",
///     "permission_mode": "default",
///     "hook_event_name": "PreToolUse",
///     "tool_name": "Bash",
///     "tool_input": {"command": "git status"},
///     "tool_use_id": "u1"
/// }"#).unwrap();
///
/// let config = Config::parse("bash { allow \"git\" }").unwrap();
/// let request = input.to_request();
/// let (decision, _reason) = evaluate(&request, &input.cwd, &input.permission_mode, &config).unwrap();
/// assert_eq!(decision, Decision::Allow);
/// ```
pub fn evaluate(
    request: &ToolRequest,
    cwd: &str,
    permission_mode: &PermissionMode,
    config: &Config,
) -> Option<(Decision, String)> {
    match request {
        ToolRequest::Bash { ref segments } => {
            bash::evaluate_bash(segments, permission_mode, config)
        }
        ToolRequest::File {
            operation,
            ref paths,
        } => files::evaluate_file_tool(*operation, paths, cwd, permission_mode, config),
        ToolRequest::Unknown => None,
        // Fail-closed: invalid input for a known tool → ask, but only if we have
        // config for that tool category. Without config, we have no opinion.
        ToolRequest::ParseError {
            ref category,
            ref reason,
        } => {
            let has_config = match category {
                ToolCategory::Bash => config.bash.is_some(),
                ToolCategory::File => config.files.is_some(),
            };
            if has_config {
                Some((Decision::Ask, reason.clone()))
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests;
