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
    let decisions: Vec<Option<Decision>> = segments
        .iter()
        .map(|seg| config.bash.lookup(&seg.program))
        .collect();

    // Aggregate decisions
    let aggregated = aggregate_decisions(&decisions);

    // Apply permission mode modifier
    match aggregated {
        Some(decision) => {
            let modified = apply_mode_modifier(decision, &input.permission_mode);
            let programs: Vec<&str> = segments.iter().map(|s| s.program.as_str()).collect();
            let reason = format!("programs: [{}]", programs.join(", "));
            Some(match modified {
                Decision::Allow => HookOutput::allow(reason),
                Decision::Ask => HookOutput::ask(reason),
                Decision::Deny => HookOutput::deny(reason),
            })
        }
        None => None,
    }
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
