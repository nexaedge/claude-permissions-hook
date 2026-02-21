mod aggregation;
mod bash;
mod files;
mod reason;

use crate::config::Config;
use crate::protocol::HookInput;
use serde_json::json;

fn make_input(tool_name: &str, permission_mode: &str, tool_input: serde_json::Value) -> HookInput {
    serde_json::from_value(json!({
        "session_id": "sess-test",
        "transcript_path": "/tmp/transcript.json",
        "cwd": "/home/user/project",
        "permission_mode": permission_mode,
        "hook_event_name": "PreToolUse",
        "tool_name": tool_name,
        "tool_input": tool_input,
        "tool_use_id": "tu-test"
    }))
    .expect("test input should parse")
}

fn bash_input(command: &str, mode: &str) -> HookInput {
    make_input("Bash", mode, json!({"command": command}))
}

fn rules_of(programs: &[&str]) -> Vec<crate::config::rule::BashRule> {
    programs
        .iter()
        .map(|p| crate::config::rule::BashRule {
            program: crate::domain::ProgramName::new(p),
            conditions: crate::config::rule::RuleConditions::default(),
        })
        .collect()
}

fn make_config(allow: &[&str], deny: &[&str], ask: &[&str]) -> Config {
    Config {
        bash: Some(crate::config::BashConfig {
            allow: rules_of(allow),
            deny: rules_of(deny),
            ask: rules_of(ask),
        }),
        ..Default::default()
    }
}
