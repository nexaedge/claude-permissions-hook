use crate::command;
use crate::config::Config;
use crate::protocol::output::Decision;
use crate::protocol::{HookInput, HookOutput};

use super::aggregation::{aggregate_decisions, apply_mode_modifier};
use super::reason::build_reason;

/// Evaluate a Bash tool invocation against bash config rules.
///
/// Receives the already-extracted `command` from `ToolUse::parse()`.
/// `None` means the command field was missing from tool_input.
pub(super) fn evaluate_bash(
    command: Option<&str>,
    input: &HookInput,
    config: &Config,
) -> Option<HookOutput> {
    let command = match command {
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
