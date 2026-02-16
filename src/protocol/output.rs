use serde::{Deserialize, Serialize};

/// The output returned to Claude Code on stdout.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOutput {
    pub hook_specific_output: PreToolUseOutput,
}

impl HookOutput {
    /// Create an "allow" response with the given reason.
    pub fn allow(reason: impl Into<String>) -> Self {
        Self::with_decision(Decision::Allow, reason)
    }

    /// Create an "ask" response with the given reason.
    pub fn ask(reason: impl Into<String>) -> Self {
        Self::with_decision(Decision::Ask, reason)
    }

    /// Create a "deny" response with the given reason.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self::with_decision(Decision::Deny, reason)
    }

    fn with_decision(decision: Decision, reason: impl Into<String>) -> Self {
        Self {
            hook_specific_output: PreToolUseOutput {
                hook_event_name: "PreToolUse".to_string(),
                permission_decision: decision,
                permission_decision_reason: reason.into(),
            },
        }
    }
}

/// PreToolUse-specific output containing the permission decision.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseOutput {
    pub hook_event_name: String,
    pub permission_decision: Decision,
    pub permission_decision_reason: String,
}

/// The permission decision: allow, ask, or deny.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hook_output_serializes_to_expected_json() {
        let output = HookOutput::allow("test reason");
        let json = serde_json::to_value(&output).expect("should serialize");

        assert_eq!(
            json,
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "permissionDecisionReason": "test reason"
                }
            })
        );
    }

    #[test]
    fn decision_ordering_allow_less_than_ask_less_than_deny() {
        assert!(Decision::Allow < Decision::Ask);
        assert!(Decision::Ask < Decision::Deny);
        assert!(Decision::Allow < Decision::Deny);

        // max() should return most restrictive
        let decisions = vec![Decision::Allow, Decision::Deny, Decision::Ask];
        assert_eq!(decisions.into_iter().max(), Some(Decision::Deny));
    }

    #[test]
    fn helper_allow_produces_correct_output() {
        let output = HookOutput::allow("allowed by rule");
        assert_eq!(
            output.hook_specific_output.permission_decision,
            Decision::Allow
        );
        assert_eq!(
            output.hook_specific_output.permission_decision_reason,
            "allowed by rule"
        );
        assert_eq!(output.hook_specific_output.hook_event_name, "PreToolUse");
    }

    #[test]
    fn helper_ask_produces_correct_output() {
        let output = HookOutput::ask("needs confirmation");
        assert_eq!(
            output.hook_specific_output.permission_decision,
            Decision::Ask
        );
        assert_eq!(
            output.hook_specific_output.permission_decision_reason,
            "needs confirmation"
        );
    }

    #[test]
    fn helper_deny_produces_correct_output() {
        let output = HookOutput::deny("blocked");
        assert_eq!(
            output.hook_specific_output.permission_decision,
            Decision::Deny
        );
        assert_eq!(
            output.hook_specific_output.permission_decision_reason,
            "blocked"
        );
    }
}
