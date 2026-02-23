use crate::config::Config;
use crate::decision_engine::matcher;
use crate::domain::rule::bash::BashRule;
use crate::domain::CommandSegment;
use crate::domain::Decision;
use crate::domain::PermissionMode;

use super::super::aggregation::{aggregate_decisions, apply_mode_modifier};
use super::super::reason::build_reason;

/// Look up a command segment against bash rules.
///
/// Uses `matcher::bash::matches()` for full condition evaluation (program name,
/// flags, subcommands, positionals, required arguments).
/// Severity ordering: deny > ask > allow. Returns `None` for unlisted programs.
fn lookup(rules: &[BashRule], segment: &CommandSegment) -> Option<Decision> {
    rules
        .iter()
        .filter(|r| matcher::bash::matches(r, segment))
        .map(|r| r.decision.clone())
        .max_by_key(|d| d.severity())
}

/// Evaluate a Bash tool invocation against bash config rules.
///
/// Receives already-parsed command segments.
/// Only performs config matching — no validation or parsing.
/// Returns the final decision and a human-readable reason string.
pub(crate) fn evaluate_bash(
    segments: &[CommandSegment],
    permission_mode: &PermissionMode,
    config: &Config,
) -> Option<(Decision, String)> {
    if config.bash.is_empty() {
        return None;
    }
    let per_program: Vec<Option<Decision>> = segments
        .iter()
        .map(|seg| lookup(&config.bash, seg))
        .collect();

    let aggregated = aggregate_decisions(&per_program);

    aggregated.map(|decision| {
        let modified = apply_mode_modifier(decision.clone(), permission_mode);
        let programs: Vec<&str> = segments.iter().map(|s| s.program.as_str()).collect();
        let reason = build_reason(&modified, &programs, &per_program, &decision);
        (modified, reason)
    })
}
