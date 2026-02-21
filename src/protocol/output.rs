use serde::{Deserialize, Serialize};

/// The output returned to Claude Code on stdout.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOutput {
    pub hook_specific_output: PreToolUseOutput,
}

impl HookOutput {
    /// Create an "allow" response with the given reason.
    ///
    /// # Examples
    ///
    /// ```
    /// use claude_permissions_hook::protocol::{HookOutput, Decision};
    ///
    /// let output = HookOutput::allow("git is allowed");
    /// assert_eq!(output.hook_specific_output.permission_decision, Decision::Allow);
    /// ```
    pub fn allow(reason: impl Into<String>) -> Self {
        Self::with_decision(Decision::Allow, reason)
    }

    /// Create an "ask" response with the given reason.
    ///
    /// # Examples
    ///
    /// ```
    /// use claude_permissions_hook::protocol::{HookOutput, Decision};
    ///
    /// let output = HookOutput::ask("needs human confirmation");
    /// assert_eq!(output.hook_specific_output.permission_decision, Decision::Ask);
    /// ```
    pub fn ask(reason: impl Into<String>) -> Self {
        Self::with_decision(Decision::Ask, reason)
    }

    /// Create a "deny" response with the given reason.
    ///
    /// # Examples
    ///
    /// ```
    /// use claude_permissions_hook::protocol::{HookOutput, Decision};
    ///
    /// let output = HookOutput::deny("blocked by rule");
    /// assert_eq!(output.hook_specific_output.permission_decision, Decision::Deny);
    /// ```
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
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

impl Decision {
    /// Explicit severity ranking: Allow(0) < Ask(1) < Deny(2).
    ///
    /// Used by aggregation to select the most restrictive decision.
    /// Explicit mapping prevents accidental breakage from enum reordering.
    pub fn severity(&self) -> u8 {
        match self {
            Decision::Allow => 0,
            Decision::Ask => 1,
            Decision::Deny => 2,
        }
    }
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
    fn decision_severity_allow_less_than_ask_less_than_deny() {
        assert!(Decision::Allow.severity() < Decision::Ask.severity());
        assert!(Decision::Ask.severity() < Decision::Deny.severity());
        assert!(Decision::Allow.severity() < Decision::Deny.severity());

        // max_by_key(severity) should return most restrictive
        let decisions = vec![Decision::Allow, Decision::Deny, Decision::Ask];
        assert_eq!(
            decisions.into_iter().max_by_key(|d| d.severity()),
            Some(Decision::Deny)
        );
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
