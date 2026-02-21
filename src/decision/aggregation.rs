use crate::protocol::output::Decision;
use crate::protocol::PermissionMode;

/// Aggregate multiple per-program decisions into a single decision.
///
/// - All None → None (no opinion on any program)
/// - Any listed → default unlisted to Ask, take most restrictive (max)
pub(crate) fn aggregate_decisions(decisions: &[Option<Decision>]) -> Option<Decision> {
    if decisions.is_empty() {
        return None;
    }

    let has_any_listed = decisions.iter().any(|d| d.is_some());

    if !has_any_listed {
        return None;
    }

    // Default unlisted to Ask, then take most restrictive (highest severity)
    decisions
        .iter()
        .map(|d| d.clone().unwrap_or(Decision::Ask))
        .max_by_key(|d| d.severity())
}

/// Apply permission mode modifier to a decision.
///
/// Allow and Deny are absolute (from config). Ask is modulated by mode:
/// - bypassPermissions → Allow
/// - dontAsk → Deny
/// - default/plan/acceptEdits → Ask (unchanged)
pub(crate) fn apply_mode_modifier(decision: Decision, mode: &PermissionMode) -> Decision {
    match decision {
        Decision::Allow | Decision::Deny => decision,
        Decision::Ask => match mode {
            PermissionMode::BypassPermissions => Decision::Allow,
            PermissionMode::DontAsk => Decision::Deny,
            _ => Decision::Ask,
        },
    }
}
