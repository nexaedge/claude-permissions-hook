use serde::ser::SerializeStruct;
use serde::Serialize;

use crate::domain::Decision;

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
    /// use claude_permissions_hook::hook_adapter::HookOutput;
    /// use claude_permissions_hook::domain::Decision;
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
    /// use claude_permissions_hook::hook_adapter::HookOutput;
    /// use claude_permissions_hook::domain::Decision;
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
    /// use claude_permissions_hook::hook_adapter::HookOutput;
    /// use claude_permissions_hook::domain::Decision;
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
#[derive(Debug)]
pub struct PreToolUseOutput {
    pub hook_event_name: String,
    pub permission_decision: Decision,
    pub permission_decision_reason: String,
}

/// Custom Serialize for PreToolUseOutput: serializes Decision as lowercase wire format.
///
/// Keeps the domain `Decision` type free of serde concerns while producing
/// the camelCase JSON that Claude Code expects.
impl Serialize for PreToolUseOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("PreToolUseOutput", 3)?;
        state.serialize_field("hookEventName", &self.hook_event_name)?;
        state.serialize_field(
            "permissionDecision",
            match &self.permission_decision {
                Decision::Allow => "allow",
                Decision::Ask => "ask",
                Decision::Deny => "deny",
            },
        )?;
        state.serialize_field("permissionDecisionReason", &self.permission_decision_reason)?;
        state.end()
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
