use crate::domain::FileOperation;
use crate::config::Config;
use crate::protocol::output::Decision;
use crate::protocol::{FileToolUse, HookInput, HookOutput, ToolUse};

use super::aggregation::{aggregate_decisions, apply_mode_modifier};
use super::reason::{build_file_reason, operation_str};

/// Map a `ToolUse` file variant to its `FileOperation`.
///
/// The protocol preserves per-tool identity; this function translates
/// it to the config domain's `FileOperation` for rule matching.
fn file_operation(tool_use: &ToolUse) -> FileOperation {
    match tool_use {
        ToolUse::Read(_) => FileOperation::Read,
        ToolUse::Write(_) => FileOperation::Write,
        ToolUse::Edit(_) => FileOperation::Edit,
        ToolUse::Glob(_) => FileOperation::Glob,
        ToolUse::Grep(_) => FileOperation::Grep,
        _ => unreachable!("file_operation called with non-file ToolUse variant"),
    }
}

/// Evaluate a file tool invocation against file config rules.
///
/// Receives a `FileToolUse` with guaranteed non-empty, normalized paths.
/// Only performs config matching — no validation, parsing, or normalization.
pub(super) fn evaluate_file_tool(
    tool_use: &ToolUse,
    file: &FileToolUse,
    input: &HookInput,
    config: &Config,
) -> Option<HookOutput> {
    let files_config = config.files.as_ref()?;
    let operation = file_operation(tool_use);

    let per_path: Vec<Option<Decision>> = file
        .paths
        .iter()
        .map(|resolved| {
            crate::config::files::lookup(
                files_config,
                &resolved.normalized,
                operation,
                &input.cwd,
            )
        })
        .collect();

    let aggregated = aggregate_decisions(&per_path);

    match aggregated {
        Some(decision) => {
            let modified = apply_mode_modifier(decision.clone(), &input.permission_mode);
            let op_str = operation_str(operation);
            let raw_paths: Vec<&str> = file.paths.iter().map(|p| p.raw.as_str()).collect();
            let reason = build_file_reason(&modified, &raw_paths, &per_path, &decision, op_str);
            Some(match modified {
                Decision::Allow => HookOutput::allow(reason),
                Decision::Ask => HookOutput::ask(reason),
                Decision::Deny => HookOutput::deny(reason),
            })
        }
        None => None,
    }
}
