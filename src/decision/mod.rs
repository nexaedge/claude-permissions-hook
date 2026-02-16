use crate::protocol::{HookInput, HookOutput, PermissionMode};

/// Evaluate a hook input and return a permission decision.
///
/// Maps permission modes to decisions:
/// - `bypassPermissions` → allow (user chose to bypass all checks)
/// - `dontAsk` → deny (cannot prompt user; safer to deny)
/// - all others → ask (no rules configured; prompt user)
pub fn evaluate(input: &HookInput) -> HookOutput {
    match input.permission_mode {
        PermissionMode::BypassPermissions => {
            HookOutput::allow("Bypass permissions mode — all tools allowed")
        }
        PermissionMode::DontAsk => HookOutput::deny("Don't ask mode — cannot prompt user, denying"),
        PermissionMode::Default | PermissionMode::Plan | PermissionMode::AcceptEdits => {
            HookOutput::ask("No rules configured — prompting user")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::output::Decision;
    use serde_json::json;

    fn make_input(permission_mode: &str) -> HookInput {
        let json = json!({
            "sessionId": "sess-test",
            "transcriptPath": "/tmp/transcript.json",
            "cwd": "/home/user/project",
            "permissionMode": permission_mode,
            "hookEventName": "PreToolUse",
            "toolName": "Bash",
            "toolInput": {"command": "ls"},
            "toolUseId": "tu-test"
        });
        serde_json::from_value(json).expect("test input should parse")
    }

    #[test]
    fn bypass_permissions_returns_allow() {
        let input = make_input("bypassPermissions");
        let output = evaluate(&input);
        assert_eq!(
            output.hook_specific_output.permission_decision,
            Decision::Allow
        );
    }

    #[test]
    fn dont_ask_returns_deny() {
        let input = make_input("dontAsk");
        let output = evaluate(&input);
        assert_eq!(
            output.hook_specific_output.permission_decision,
            Decision::Deny
        );
    }

    #[test]
    fn default_returns_ask() {
        let input = make_input("default");
        let output = evaluate(&input);
        assert_eq!(
            output.hook_specific_output.permission_decision,
            Decision::Ask
        );
    }

    #[test]
    fn plan_returns_ask() {
        let input = make_input("plan");
        let output = evaluate(&input);
        assert_eq!(
            output.hook_specific_output.permission_decision,
            Decision::Ask
        );
    }

    #[test]
    fn accept_edits_returns_ask() {
        let input = make_input("acceptEdits");
        let output = evaluate(&input);
        assert_eq!(
            output.hook_specific_output.permission_decision,
            Decision::Ask
        );
    }

    #[test]
    fn all_decisions_include_non_empty_reason() {
        let modes = [
            "bypassPermissions",
            "dontAsk",
            "default",
            "plan",
            "acceptEdits",
        ];
        for mode in modes {
            let input = make_input(mode);
            let output = evaluate(&input);
            assert!(
                !output
                    .hook_specific_output
                    .permission_decision_reason
                    .is_empty(),
                "reason should not be empty for mode {mode}"
            );
        }
    }
}
