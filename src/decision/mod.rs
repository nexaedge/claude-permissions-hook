use crate::command;
use crate::config::Config;
use crate::file_tools::{self, FileOperation};
use crate::protocol::output::Decision;
use crate::protocol::{HookInput, HookOutput, PermissionMode};

/// Evaluate a hook input against optional config and return a permission decision.
///
/// Returns `None` when the hook has no opinion (unrecognized tools, or all
/// programs/paths unlisted). Returns `Some(output)` with a concrete decision otherwise.
///
/// - No config → `Some(Ask)` for all tools (user needs to set up config)
/// - Bash tool → parse command, lookup programs, aggregate, apply mode
/// - File tool (Read/Write/Edit/Glob/Grep) → extract paths, lookup against file rules
/// - Other tool → `None` (no opinion)
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

    match input.tool_name.as_str() {
        "Bash" => evaluate_bash(input, config),
        "Read" | "Write" | "Edit" | "Glob" | "Grep" => evaluate_file_tool(input, config),
        _ => None,
    }
}

/// Evaluate a Bash tool invocation against bash config rules.
fn evaluate_bash(input: &HookInput, config: &Config) -> Option<HookOutput> {
    let command = match input.tool_input.get("command").and_then(|v| v.as_str()) {
        Some(cmd) => cmd,
        None => return Some(HookOutput::ask("Bash tool without command field")),
    };

    if command.trim().is_empty() {
        return Some(HookOutput::ask("Empty bash command"));
    }

    let segments = match command::parse(command) {
        Ok(segs) => segs,
        Err(e) => return Some(HookOutput::ask(format!("Failed to parse command: {e}"))),
    };

    if segments.is_empty() {
        return Some(HookOutput::ask(
            "No programs extracted from command".to_string(),
        ));
    }

    let bash = config.bash.as_ref()?;
    let per_program: Vec<Option<Decision>> = segments.iter().map(|seg| bash.lookup(seg)).collect();

    let aggregated = aggregate_decisions(&per_program);

    match aggregated {
        Some(decision) => {
            let modified = apply_mode_modifier(decision.clone(), &input.permission_mode);
            let programs: Vec<&str> = segments.iter().map(|s| s.program.as_str()).collect();
            let reason = build_reason(&modified, &programs, &per_program, &decision);
            Some(match modified {
                Decision::Allow => HookOutput::allow(reason),
                Decision::Ask => HookOutput::ask(reason),
                Decision::Deny => HookOutput::deny(reason),
            })
        }
        None => None,
    }
}

/// Evaluate a file tool invocation against file config rules.
///
/// Flow: check files config → extract paths → fail-closed on empty → normalize
/// → lookup per-path → aggregate → apply mode → build reason.
fn evaluate_file_tool(input: &HookInput, config: &Config) -> Option<HookOutput> {
    // No files config → no opinion on file tools (backwards compat)
    let files_config = config.files.as_ref()?;

    // Extract paths + operation
    let (paths, operation) =
        file_tools::extract_file_paths(&input.tool_name, &input.tool_input, &input.cwd)?;

    // No paths extracted → fail-closed
    if paths.is_empty() {
        return Some(HookOutput::ask(format!(
            "{APP_NAME}: no file path provided for {} tool",
            input.tool_name
        )));
    }

    let home = std::env::var("HOME").unwrap_or_default();

    // Per-path lookup
    let per_path: Vec<Option<Decision>> = paths
        .iter()
        .map(|p| {
            let normalized = crate::path::normalize(p, &input.cwd);
            files_config.lookup(&normalized, operation, &input.cwd, &home)
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

const APP_NAME: &str = "claude-permissions-hook";

/// Build a human-readable reason string for the decision.
///
/// `modified` is the final decision (after mode modifier).
/// `programs` is the full list of program names in the command.
/// `per_program` is the per-program lookup results (before aggregation).
/// `pre_modifier` is the aggregated decision before mode modifier was applied.
fn build_reason(
    modified: &Decision,
    programs: &[&str],
    per_program: &[Option<Decision>],
    pre_modifier: &Decision,
) -> String {
    let is_single = programs.len() == 1;
    match modified {
        Decision::Allow => {
            format!("{APP_NAME}: allowed ({})", programs.join(", "))
        }
        Decision::Deny => {
            let trigger = find_trigger(programs, per_program, pre_modifier);
            let mode_converted = *pre_modifier != Decision::Deny;
            if mode_converted {
                if is_single {
                    format!("{APP_NAME}: '{trigger}' denied by dontAsk mode")
                } else {
                    format!(
                        "{APP_NAME}: '{trigger}' denied by dontAsk mode (in: {})",
                        programs.join(", ")
                    )
                }
            } else if is_single {
                format!("{APP_NAME}: '{trigger}' is in your deny list")
            } else {
                format!(
                    "{APP_NAME}: '{trigger}' is denied (in: {})",
                    programs.join(", ")
                )
            }
        }
        Decision::Ask => {
            let trigger = find_trigger(programs, per_program, pre_modifier);
            if is_single {
                format!("{APP_NAME}: '{trigger}' requires confirmation")
            } else {
                format!(
                    "{APP_NAME}: '{trigger}' requires confirmation (in: {})",
                    programs.join(", ")
                )
            }
        }
    }
}

/// Convert a FileOperation to its lowercase string for reason messages.
fn operation_str(op: FileOperation) -> &'static str {
    match op {
        FileOperation::Read => "read",
        FileOperation::Write => "write",
        FileOperation::Edit => "edit",
        FileOperation::Glob => "glob",
        FileOperation::Grep => "grep",
    }
}

/// Build a human-readable reason string for a file tool decision.
fn build_file_reason(
    modified: &Decision,
    paths: &[String],
    per_path: &[Option<Decision>],
    pre_modifier: &Decision,
    operation: &str,
) -> String {
    match modified {
        Decision::Allow => {
            format!("{APP_NAME}: allowed {operation} ({})", paths.join(", "))
        }
        Decision::Deny => {
            let trigger = find_file_trigger(paths, per_path, pre_modifier);
            let mode_converted = *pre_modifier != Decision::Deny;
            if mode_converted {
                format!("{APP_NAME}: '{trigger}' denied by dontAsk mode ({operation})")
            } else {
                format!("{APP_NAME}: '{trigger}' denied by file rules ({operation})")
            }
        }
        Decision::Ask => {
            let trigger = find_file_trigger(paths, per_path, pre_modifier);
            format!("{APP_NAME}: '{trigger}' requires confirmation ({operation})")
        }
    }
}

/// Find the path that triggered the most restrictive file decision.
fn find_file_trigger<'a>(
    paths: &'a [String],
    per_path: &[Option<Decision>],
    target: &Decision,
) -> &'a str {
    for (path, dec) in paths.iter().zip(per_path.iter()) {
        if dec.as_ref() == Some(target) {
            return path;
        }
    }
    if *target == Decision::Ask {
        for (path, dec) in paths.iter().zip(per_path.iter()) {
            if dec.is_none() {
                return path;
            }
        }
    }
    &paths[0]
}

/// Find the program that triggered the most restrictive decision.
///
/// Searches for an explicit match first (program whose config decision equals the target),
/// then falls back to unlisted programs (which default to Ask during aggregation).
fn find_trigger<'a>(
    programs: &[&'a str],
    per_program: &[Option<Decision>],
    target: &Decision,
) -> &'a str {
    // First: find a program explicitly configured with the target decision
    for (prog, dec) in programs.iter().zip(per_program.iter()) {
        if dec.as_ref() == Some(target) {
            return prog;
        }
    }
    // Second: if target is Ask, find an unlisted program (None defaults to Ask)
    if *target == Decision::Ask {
        for (prog, dec) in programs.iter().zip(per_program.iter()) {
            if dec.is_none() {
                return prog;
            }
        }
    }
    // Fallback (shouldn't happen with valid aggregation)
    programs[0]
}

/// Aggregate multiple per-program decisions into a single decision.
///
/// - All None → None (no opinion on any program)
/// - Any listed → default unlisted to Ask, take most restrictive (max)
fn aggregate_decisions(decisions: &[Option<Decision>]) -> Option<Decision> {
    if decisions.is_empty() {
        return None;
    }

    let has_any_listed = decisions.iter().any(|d| d.is_some());

    if !has_any_listed {
        return None;
    }

    // Default unlisted to Ask, then take max (most restrictive)
    decisions
        .iter()
        .map(|d| d.clone().unwrap_or(Decision::Ask))
        .max()
}

/// Apply permission mode modifier to a decision.
///
/// Allow and Deny are absolute (from config). Ask is modulated by mode:
/// - bypassPermissions → Allow
/// - dontAsk → Deny
/// - default/plan/acceptEdits → Ask (unchanged)
fn apply_mode_modifier(decision: Decision, mode: &PermissionMode) -> Decision {
    match decision {
        Decision::Allow | Decision::Deny => decision,
        Decision::Ask => match mode {
            PermissionMode::BypassPermissions => Decision::Allow,
            PermissionMode::DontAsk => Decision::Deny,
            _ => Decision::Ask,
        },
    }
}

#[cfg(test)]
mod tests;
