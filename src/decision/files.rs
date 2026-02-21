use crate::config::Config;
use crate::protocol::output::Decision;
use crate::protocol::{HookInput, HookOutput, ToolUse};

use super::aggregation::{aggregate_decisions, apply_mode_modifier};
use super::reason::{build_file_reason, operation_str};
use super::APP_NAME;

/// Evaluate a file tool invocation against file config rules.
///
/// Receives the already-parsed `ToolUse` variant with typed path data.
/// Flow: check files config → extract paths → fail-closed on empty → normalize
/// → lookup per-path → aggregate → apply mode → build reason.
pub(super) fn evaluate_file_tool(
    tool_use: &ToolUse,
    input: &HookInput,
    config: &Config,
) -> Option<HookOutput> {
    // No files config → no opinion on file tools (backwards compat)
    let files_config = config.files.as_ref()?;

    let operation = tool_use.file_operation()?;
    let paths = tool_use.file_paths(&input.cwd)?;

    // No paths extracted → fail-closed
    if paths.is_empty() {
        return Some(HookOutput::ask(format!(
            "{APP_NAME}: no file path provided for {} tool",
            input.tool_name
        )));
    }

    // Per-path lookup
    let per_path: Vec<Option<Decision>> = paths
        .iter()
        .map(|p| match crate::path::normalize(p, &input.cwd) {
            Ok(normalized) => files_config.lookup(&normalized, operation, &input.cwd),
            Err(_) => Some(Decision::Ask), // fail-closed: $HOME not set
        })
        .collect();

    let aggregated = aggregate_decisions(&per_path);

    match aggregated {
        Some(decision) => {
            let modified = apply_mode_modifier(decision.clone(), &input.permission_mode);
            let op_str = operation_str(operation);
            let reason = build_file_reason(&modified, &paths, &per_path, &decision, op_str);
            Some(match modified {
                Decision::Allow => HookOutput::allow(reason),
                Decision::Ask => HookOutput::ask(reason),
                Decision::Deny => HookOutput::deny(reason),
            })
        }
        None => None,
    }
}
