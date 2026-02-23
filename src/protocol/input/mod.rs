pub mod tool_use;

use serde::Deserialize;
use serde_json::Value;

use crate::domain::PermissionMode;
use crate::domain::ToolRequest;
pub use crate::error::ToolParseError;
pub use tool_use::{BashToolUse, FileToolUse, ToolUse};

/// The input received from Claude Code on stdin for a PreToolUse hook.
///
/// `tool_use` is constructed automatically during deserialization from the
/// raw `tool_name` and `tool_input` JSON fields. It is a `Result` because
/// known tools may have invalid input (missing command, bad path, etc.).
#[derive(Debug)]
pub struct HookInput {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    pub permission_mode: PermissionMode,
    pub hook_event_name: String,
    pub tool_use: Result<ToolUse, ToolParseError>,
    pub tool_use_id: String,
}

impl HookInput {
    /// Convert protocol input into domain `ToolRequest` for the decision layer.
    ///
    /// Returns `Some` for tools the hook can evaluate (Bash, File).
    /// Returns `None` for unknown tools — the caller should return no opinion.
    /// Parse errors remain accessible via `self.tool_use` for fail-closed handling.
    pub fn to_request(&self) -> Option<ToolRequest> {
        match &self.tool_use {
            Ok(ToolUse::Bash(ref bash)) => Some(ToolRequest::Bash {
                segments: bash.segments.clone(),
            }),
            Ok(ToolUse::File(ref file)) => Some(ToolRequest::File {
                operation: file.operation,
                targets: file.targets.clone(),
            }),
            Ok(ToolUse::Unknown { .. }) => None,
            Err(_) => None,
        }
    }
}

/// Wire permission mode — camelCase variant names matching Claude Code's JSON.
///
/// Deserialized from wire format and converted to domain `PermissionMode`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum WirePermissionMode {
    Default,
    Plan,
    AcceptEdits,
    DontAsk,
    BypassPermissions,
}

impl From<WirePermissionMode> for PermissionMode {
    fn from(wire: WirePermissionMode) -> Self {
        match wire {
            WirePermissionMode::Default => PermissionMode::Default,
            WirePermissionMode::Plan => PermissionMode::Plan,
            WirePermissionMode::AcceptEdits => PermissionMode::AcceptEdits,
            WirePermissionMode::DontAsk => PermissionMode::DontAsk,
            WirePermissionMode::BypassPermissions => PermissionMode::BypassPermissions,
        }
    }
}

/// Raw wire format — mirrors the JSON that Claude Code sends.
///
/// Passed to `ToolUse::parse` as context so helpers can access `tool_name`
/// (for error messages) and `cwd` (for path normalization).
#[derive(Deserialize)]
struct RawHookInput {
    session_id: String,
    transcript_path: String,
    cwd: String,
    permission_mode: WirePermissionMode,
    hook_event_name: String,
    tool_name: String,
    tool_input: Value,
    tool_use_id: String,
}

impl<'de> Deserialize<'de> for HookInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawHookInput::deserialize(deserializer)?;
        let tool_use = ToolUse::parse(&raw);
        Ok(HookInput {
            session_id: raw.session_id,
            transcript_path: raw.transcript_path,
            cwd: raw.cwd,
            permission_mode: raw.permission_mode.into(),
            hook_event_name: raw.hook_event_name,
            tool_use,
            tool_use_id: raw.tool_use_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_input_json() -> serde_json::Value {
        json!({
            "session_id": "sess-123",
            "transcript_path": "/tmp/transcript.json",
            "cwd": "/home/user/project",
            "permission_mode": "default",
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "ls"},
            "tool_use_id": "tu-456"
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
        assert_eq!(input.tool_use_id, "tu-456");

        let tool_use = input.tool_use.expect("should be Ok");
        assert_eq!(tool_use.tool_name(), "Bash");
        assert!(matches!(
            tool_use,
            ToolUse::Bash(ref b) if b.raw == "ls" && !b.segments.is_empty()
        ));
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
            input["permission_mode"] = json!(json_value);
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

    #[test]
    fn invalid_bash_is_err() {
        let mut input = minimal_input_json();
        input["tool_input"] = json!({});
        let parsed: HookInput = serde_json::from_value(input).expect("should parse");
        assert!(parsed.tool_use.is_err());
    }
}
