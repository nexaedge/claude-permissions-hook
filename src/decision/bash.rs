use crate::config::Config;
use crate::protocol::output::Decision;
use crate::protocol::{BashToolUse, HookInput, HookOutput};

use super::aggregation::{aggregate_decisions, apply_mode_modifier};
use super::reason::build_reason;

/// Evaluate a Bash tool invocation against bash config rules.
///
/// Receives a `BashToolUse` with guaranteed non-empty, parsed segments.
/// Only performs config matching — no validation or parsing.
pub(super) fn evaluate_bash(
    bash: &BashToolUse,
    input: &HookInput,
    config: &Config,
) -> Option<HookOutput> {
    let bash_config = config.bash.as_ref()?;
    let per_program: Vec<Option<Decision>> = bash
        .segments
        .iter()
        .map(|seg| crate::config::bash::lookup(bash_config, seg))
        .collect();

    let aggregated = aggregate_decisions(&per_program);

    match aggregated {
        Some(decision) => {
            let modified = apply_mode_modifier(decision.clone(), &input.permission_mode);
            let programs: Vec<&str> = bash.segments.iter().map(|s| s.program.as_str()).collect();
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
