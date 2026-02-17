use crate::command;
use crate::config::Config;
use crate::protocol::output::Decision;
use crate::protocol::{HookInput, HookOutput, PermissionMode};

/// Evaluate a hook input against optional config and return a permission decision.
///
/// Returns `None` when the hook has no opinion (non-Bash tools with config, or
/// all programs unlisted). Returns `Some(output)` with a concrete decision otherwise.
///
/// - No config → `Some(Ask)` for all tools (user needs to set up config)
/// - Non-Bash tool with config → `None` (empty `{}` response)
/// - Bash tool with config → parse command, lookup programs, aggregate, apply mode
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

    // Non-Bash tool → no opinion (let Claude handle natively)
    if input.tool_name != "Bash" {
        return None;
    }

    // Bash tool: extract command
    let command = match input.tool_input.get("command").and_then(|v| v.as_str()) {
        Some(cmd) => cmd,
        None => return Some(HookOutput::ask("Bash tool without command field")),
    };

    // Empty command is suspicious
    if command.trim().is_empty() {
        return Some(HookOutput::ask("Empty bash command"));
    }

    // Parse command into segments — fail closed on parse errors
    let segments = match command::parse(command) {
        Ok(segs) => segs,
        Err(e) => return Some(HookOutput::ask(format!("Failed to parse command: {e}"))),
    };

    // Non-empty command but no programs extracted — fail closed
    if segments.is_empty() {
        return Some(HookOutput::ask(
            "No programs extracted from command".to_string(),
        ));
    }

    // Look up each program
    let per_program: Vec<Option<Decision>> = segments
        .iter()
        .map(|seg| config.bash.lookup(&seg.program))
        .collect();

    // Aggregate decisions
    let aggregated = aggregate_decisions(&per_program);

    // Apply permission mode modifier
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
            if is_single {
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
