use serde::Deserialize;
use serde_json::Value;

/// The input received from Claude Code on stdin for a PreToolUse hook.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInput {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    pub permission_mode: PermissionMode,
    pub hook_event_name: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_use_id: String,
}

/// Claude Code's permission modes.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    Plan,
    AcceptEdits,
    DontAsk,
    BypassPermissions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_input_json() -> serde_json::Value {
        json!({
            "sessionId": "sess-123",
            "transcriptPath": "/tmp/transcript.json",
            "cwd": "/home/user/project",
            "permissionMode": "default",
            "hookEventName": "PreToolUse",
            "toolName": "Bash",
            "toolInput": {"command": "ls"},
            "toolUseId": "tu-456"
        })
    }

    #[test]
    fn parse_minimal_hook_input() {
        let input: HookInput =
            serde_json::from_value(minimal_input_json()).expect("should parse valid input");

        assert_eq!(input.session_id, "sess-123");
        assert_eq!(input.transcript_path, "/tmp/transcript.json");
        assert_eq!(input.cwd, "/home/user/project");
        assert_eq!(input.permission_mode, PermissionMode::Default);
        assert_eq!(input.hook_event_name, "PreToolUse");
        assert_eq!(input.tool_name, "Bash");
        assert_eq!(input.tool_use_id, "tu-456");
        assert_eq!(input.tool_input, json!({"command": "ls"}));
    }

    #[test]
    fn all_permission_modes_deserialize() {
        let modes = [
            ("default", PermissionMode::Default),
            ("plan", PermissionMode::Plan),
            ("acceptEdits", PermissionMode::AcceptEdits),
            ("dontAsk", PermissionMode::DontAsk),
            ("bypassPermissions", PermissionMode::BypassPermissions),
        ];

        for (json_value, expected) in modes {
            let mut input = minimal_input_json();
            input["permissionMode"] = json!(json_value);
            let parsed: HookInput =
                serde_json::from_value(input).expect("should parse permission mode");
            assert_eq!(parsed.permission_mode, expected, "failed for {json_value}");
        }
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let mut input = minimal_input_json();
        input["brandNewField"] = json!("surprise");
        input["anotherUnknown"] = json!(42);

        let parsed: HookInput =
            serde_json::from_value(input).expect("unknown fields should not cause failure");
        assert_eq!(parsed.session_id, "sess-123");
    }
}
