// Contract tests: assert only durable external invariants.
// These tests survive internal restructuring — they never assert specific
// decision values or reason strings, only the shape and properties of output.

mod common;

use common::{bash_input_json, make_input_json, parse_hook_output, run_hook, run_hook_with_config};

// ---- JSON shape invariants ----

#[test]
fn contract_output_is_valid_json() {
    let input = bash_input_json("ls", "default");
    let (stdout, _, _) = run_hook(&input);
    let _: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output must be valid JSON");
}

#[test]
fn contract_output_has_hook_specific_output() {
    let input = bash_input_json("ls", "default");
    let (stdout, _, _) = run_hook(&input);
    let value = parse_hook_output(&stdout);
    assert!(
        value.get("hookSpecificOutput").is_some(),
        "output must contain hookSpecificOutput"
    );
}

#[test]
fn contract_hook_event_name_is_pre_tool_use() {
    let input = bash_input_json("ls", "default");
    let (stdout, _, _) = run_hook(&input);
    let value = parse_hook_output(&stdout);
    assert_eq!(
        value["hookSpecificOutput"]["hookEventName"], "PreToolUse",
        "hookEventName must always be PreToolUse"
    );
}

#[test]
fn contract_decision_is_valid_enum() {
    let input = bash_input_json("ls", "default");
    let (stdout, _, _) = run_hook(&input);
    let value = parse_hook_output(&stdout);
    let decision = value["hookSpecificOutput"]["permissionDecision"]
        .as_str()
        .expect("permissionDecision must be a string");
    assert!(
        ["allow", "ask", "deny"].contains(&decision),
        "permissionDecision must be allow/ask/deny, got: {decision}"
    );
}

#[test]
fn contract_decision_reason_is_string() {
    let input = bash_input_json("ls", "default");
    let (stdout, _, _) = run_hook(&input);
    let value = parse_hook_output(&stdout);
    assert!(
        value["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .is_some(),
        "permissionDecisionReason must be a string"
    );
}

#[test]
fn contract_with_config_has_same_shape() {
    let input = bash_input_json("git status", "default");
    let config = r#"bash { allow "git" }"#;
    let (stdout, _, _) = run_hook_with_config(&input, config);
    let value = parse_hook_output(&stdout);
    let specific = &value["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    assert!(specific.get("permissionDecision").is_some());
    assert!(specific.get("permissionDecisionReason").is_some());
}

// ---- Exit code invariants ----

#[test]
fn contract_exit_code_zero_on_normal_input() {
    let input = bash_input_json("ls", "default");
    let (_, _, exit_code) = run_hook(&input);
    assert_eq!(exit_code, 0, "exit code must always be 0");
}

#[test]
fn contract_exit_code_zero_on_malformed_json() {
    let (_, _, exit_code) = run_hook("this is not json");
    assert_eq!(exit_code, 0, "exit code must be 0 even on malformed input");
}

#[test]
fn contract_exit_code_zero_on_empty_stdin() {
    let (_, _, exit_code) = run_hook("");
    assert_eq!(exit_code, 0, "exit code must be 0 even on empty stdin");
}

// ---- Fail-closed invariants ----

#[test]
fn contract_malformed_json_returns_ask() {
    let (stdout, _, _) = run_hook("totally broken {{{");
    let value = parse_hook_output(&stdout);
    let decision = value["hookSpecificOutput"]["permissionDecision"]
        .as_str()
        .unwrap();
    assert_eq!(decision, "ask", "malformed JSON must fail-closed to ask");
}

#[test]
fn contract_empty_stdin_returns_ask() {
    let (stdout, _, _) = run_hook("");
    let value = parse_hook_output(&stdout);
    let decision = value["hookSpecificOutput"]["permissionDecision"]
        .as_str()
        .unwrap();
    assert_eq!(decision, "ask", "empty stdin must fail-closed to ask");
}

#[test]
fn contract_missing_tool_input_fields_returns_ask() {
    let input = make_input_json(
        "Bash",
        "default",
        serde_json::json!({"not_command": "value"}),
    );
    let config = r#"bash { allow "git" }"#;
    let (stdout, _, _) = run_hook_with_config(&input, config);
    let value = parse_hook_output(&stdout);
    let decision = value["hookSpecificOutput"]["permissionDecision"]
        .as_str()
        .unwrap();
    assert_eq!(
        decision, "ask",
        "missing tool_input fields must fail-closed to ask"
    );
}

#[test]
fn contract_config_parse_failure_returns_ask() {
    let input = bash_input_json("ls", "default");
    let (stdout, _, _) = run_hook_with_config(&input, "invalid {{ kdl {{ syntax");
    let value = parse_hook_output(&stdout);
    let decision = value["hookSpecificOutput"]["permissionDecision"]
        .as_str()
        .unwrap();
    assert_eq!(
        decision, "ask",
        "config parse failure must fail-closed to ask"
    );
}

// ---- Stderr invariant ----

#[test]
fn contract_no_stderr_on_normal_operation() {
    let input = bash_input_json("ls", "default");
    let (_, stderr, _) = run_hook(&input);
    assert!(
        stderr.is_empty(),
        "stderr should be empty in normal operation, got: {stderr}"
    );
}

#[test]
fn contract_no_stderr_on_error_input() {
    let (_, stderr, _) = run_hook("garbage");
    assert!(
        stderr.is_empty(),
        "stderr should be empty even on error input, got: {stderr}"
    );
}

// ---- No-opinion invariant: empty JSON is valid ----

#[test]
fn contract_no_opinion_returns_empty_json_object() {
    // Non-bash tool with bash-only config → no opinion → empty {}
    let input = make_input_json(
        "Read",
        "default",
        serde_json::json!({"file_path": "/tmp/x"}),
    );
    let config = r#"bash { allow "git" }"#;
    let (stdout, _, exit_code) = run_hook_with_config(&input, config);
    assert_eq!(exit_code, 0);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        value,
        serde_json::json!({}),
        "no-opinion must return empty JSON object"
    );
}
