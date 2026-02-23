mod aggregation;
mod bash;
mod domain_level;
mod files;
mod reason;

use crate::config::Config;
use crate::domain::Decision;
use crate::protocol::HookInput;
use serde_json::json;

/// Convenience wrapper: converts HookInput → domain types and calls evaluate.
fn eval(input: &HookInput, config: Option<&Config>) -> Option<(Decision, String)> {
    let request = input.to_request();
    config.and_then(|cfg| {
        crate::decision::evaluate(&request, &input.cwd, &input.permission_mode, cfg)
    })
}

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

fn rules_of_with_decision(
    programs: &[&str],
    decision: crate::domain::Decision,
) -> Vec<crate::domain::rule::bash::BashRule> {
    programs
        .iter()
        .map(|p| crate::domain::rule::bash::BashRule {
            decision: decision.clone(),
            program: crate::domain::ProgramName::parse(p).unwrap(),
            conditions: crate::domain::rule::bash::BashConditions::default(),
        })
        .collect()
}

fn make_config(allow: &[&str], deny: &[&str], ask: &[&str]) -> Config {
    use crate::domain::Decision;
    let mut bash: crate::domain::rule::bash::BashConfig = Vec::new();
    bash.extend(rules_of_with_decision(allow, Decision::Allow));
    bash.extend(rules_of_with_decision(deny, Decision::Deny));
    bash.extend(rules_of_with_decision(ask, Decision::Ask));
    Config {
        bash: Some(bash),
        ..Default::default()
    }
}
