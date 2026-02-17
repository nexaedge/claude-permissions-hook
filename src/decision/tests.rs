use super::*;
use serde_json::json;

fn make_input(tool_name: &str, permission_mode: &str, tool_input: serde_json::Value) -> HookInput {
    serde_json::from_value(json!({
        "sessionId": "sess-test",
        "transcriptPath": "/tmp/transcript.json",
        "cwd": "/home/user/project",
        "permissionMode": permission_mode,
        "hookEventName": "PreToolUse",
        "toolName": tool_name,
        "toolInput": tool_input,
        "toolUseId": "tu-test"
    }))
    .expect("test input should parse")
}

fn bash_input(command: &str, mode: &str) -> HookInput {
    make_input("Bash", mode, json!({"command": command}))
}

fn make_config(allow: &[&str], deny: &[&str], ask: &[&str]) -> Config {
    Config {
        bash: crate::config::BashConfig {
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
            ask: ask.iter().map(|s| s.to_string()).collect(),
        },
    }
}

// ---- Test macros ----

/// Bash command with config → expects a specific Decision variant.
macro_rules! bash_decision_test {
    ($name:ident, cmd: $cmd:expr, mode: $mode:expr,
     allow: [$($a:expr),*], deny: [$($d:expr),*], ask: [$($q:expr),*],
     expect: $decision:expr) => {
        #[test]
        fn $name() {
            let config = make_config(&[$($a),*], &[$($d),*], &[$($q),*]);
            let input = bash_input($cmd, $mode);
            let result = evaluate(&input, Some(&config)).unwrap();
            assert_eq!(result.hook_specific_output.permission_decision, $decision);
        }
    };
}

/// Bash command with config → expects None (no opinion).
macro_rules! bash_none_test {
    ($name:ident, cmd: $cmd:expr, mode: $mode:expr,
     allow: [$($a:expr),*], deny: [$($d:expr),*], ask: [$($q:expr),*]) => {
        #[test]
        fn $name() {
            let config = make_config(&[$($a),*], &[$($d),*], &[$($q),*]);
            let input = bash_input($cmd, $mode);
            assert!(evaluate(&input, Some(&config)).is_none());
        }
    };
}

/// Non-Bash tool with config → expects None.
macro_rules! non_bash_none_test {
    ($name:ident, tool: $tool:expr) => {
        #[test]
        fn $name() {
            let config = make_config(&["git"], &["rm"], &[]);
            let input = make_input($tool, "default", json!({}));
            assert!(evaluate(&input, Some(&config)).is_none());
        }
    };
}

/// aggregate_decisions() test case.
macro_rules! aggregate_test {
    ($name:ident, input: [$($val:expr),*], expect: $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(aggregate_decisions(&[$($val),*]), $expected);
        }
    };
}

/// apply_mode_modifier() test case.
macro_rules! mode_modifier_test {
    ($name:ident, decision: $decision:expr, mode: $mode:expr, expect: $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(apply_mode_modifier($decision, &$mode), $expected);
        }
    };
}

// ---- No config ----

#[test]
fn no_config_bash_tool_returns_ask() {
    let input = bash_input("ls", "default");
    let output = evaluate(&input, None).unwrap();
    assert_eq!(
        output.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn no_config_non_bash_tool_returns_ask() {
    let input = make_input("Read", "default", json!({"file_path": "/tmp/x"}));
    let output = evaluate(&input, None).unwrap();
    assert_eq!(
        output.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

// ---- Non-Bash tools with config → None ----

non_bash_none_test!(config_read_returns_none,  tool: "Read");
non_bash_none_test!(config_write_returns_none, tool: "Write");
non_bash_none_test!(config_edit_returns_none,  tool: "Edit");
non_bash_none_test!(config_glob_returns_none,  tool: "Glob");

// ---- Single command evaluation ----

bash_decision_test!(single_allowed_program,
    cmd: "git status", mode: "default",
    allow: ["git"], deny: [], ask: [],
    expect: Decision::Allow);

bash_decision_test!(single_denied_program,
    cmd: "rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(single_ask_program,
    cmd: "docker run ubuntu", mode: "default",
    allow: [], deny: [], ask: ["docker"],
    expect: Decision::Ask);

bash_none_test!(single_unlisted_program,
    cmd: "ls -la", mode: "default",
    allow: ["git"], deny: ["rm"], ask: ["docker"]);

// ---- Multi-command aggregation ----

bash_decision_test!(both_allow,
    cmd: "git add . && git commit", mode: "default",
    allow: ["git"], deny: [], ask: [],
    expect: Decision::Allow);

bash_decision_test!(allow_plus_deny,
    cmd: "git add && rm -rf /", mode: "default",
    allow: ["git"], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(allow_plus_unlisted,
    cmd: "git status && ls", mode: "default",
    allow: ["git"], deny: [], ask: [],
    expect: Decision::Ask);

bash_none_test!(both_unlisted,
    cmd: "foo && bar", mode: "default",
    allow: ["git"], deny: ["rm"], ask: []);

// ---- Permission mode modifiers (full evaluate) ----

bash_decision_test!(bypass_with_ask_returns_allow,
    cmd: "docker run", mode: "bypassPermissions",
    allow: [], deny: [], ask: ["docker"],
    expect: Decision::Allow);

bash_decision_test!(dont_ask_with_ask_returns_deny,
    cmd: "docker run", mode: "dontAsk",
    allow: [], deny: [], ask: ["docker"],
    expect: Decision::Deny);

bash_decision_test!(default_with_ask_returns_ask,
    cmd: "docker run", mode: "default",
    allow: [], deny: [], ask: ["docker"],
    expect: Decision::Ask);

// ---- Fail-closed edge cases ----

#[test]
fn bash_tool_without_command_field_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    let input = make_input("Bash", "default", json!({"description": "something"}));
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn empty_command_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    let input = bash_input("", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn whitespace_command_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    let input = bash_input("   ", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn parse_error_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    let input = bash_input("git add . &&", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn no_programs_extracted_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    // Arithmetic expression parses but yields no program segments
    let input = bash_input("(( x + 1 ))", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

// ---- aggregate_decisions() unit tests ----

aggregate_test!(aggregate_empty,             input: [],                                       expect: None);
aggregate_test!(aggregate_all_none,          input: [None, None],                             expect: None);
aggregate_test!(aggregate_allow_and_deny,    input: [Some(Decision::Allow), Some(Decision::Deny)],  expect: Some(Decision::Deny));
aggregate_test!(aggregate_allow_and_none,    input: [Some(Decision::Allow), None],            expect: Some(Decision::Ask));
aggregate_test!(aggregate_deny_and_none,     input: [Some(Decision::Deny), None],             expect: Some(Decision::Deny));

// ---- apply_mode_modifier() unit tests ----

mode_modifier_test!(bypass_allow_stays_allow, decision: Decision::Allow, mode: PermissionMode::BypassPermissions, expect: Decision::Allow);
mode_modifier_test!(bypass_deny_stays_deny,   decision: Decision::Deny,  mode: PermissionMode::BypassPermissions, expect: Decision::Deny);
mode_modifier_test!(dont_ask_allow_stays,     decision: Decision::Allow, mode: PermissionMode::DontAsk,           expect: Decision::Allow);
mode_modifier_test!(dont_ask_deny_stays,      decision: Decision::Deny,  mode: PermissionMode::DontAsk,           expect: Decision::Deny);
