use super::Decision;

/// A permission decision paired with a human-readable reason.
///
/// Produced by the decision engine after evaluating a tool request
/// against the configured policy rules.
#[derive(Debug)]
pub struct PermissionDecision {
    pub decision: Decision,
    pub reason: String,
}
