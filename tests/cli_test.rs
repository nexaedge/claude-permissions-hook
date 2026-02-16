use std::path::PathBuf;
use std::process::{Command, Output};

/// Path to the compiled binary.
fn binary_path() -> PathBuf {
    // cargo test builds to target/debug; env gives us the exact directory
    let path = PathBuf::from(env!("CARGO_BIN_EXE_claude-permissions-hook"));
    assert!(path.exists(), "binary not found at {}", path.display());
    path
}

/// Run the hook subcommand with the given stdin input, returning stdout and exit code.
fn run_hook(stdin_input: &str) -> (String, i32) {
    let output: Output = Command::new(binary_path())
        .arg("hook")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin_input.as_bytes())
                .unwrap();
            child.wait_with_output()
        })
        .expect("failed to execute binary");

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid UTF-8");
    let exit_code = output.status.code().unwrap_or(-1);
    (stdout, exit_code)
}

/// Load a fixture file from tests/fixtures/.
fn load_fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()))
}

/// Parse the JSON output and extract the decision and reason.
fn parse_output(stdout: &str) -> (String, String) {
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
    let specific = &value["hookSpecificOutput"];
    let decision = specific["permissionDecision"]
        .as_str()
        .expect("missing permissionDecision")
        .to_string();
    let reason = specific["permissionDecisionReason"]
        .as_str()
        .expect("missing permissionDecisionReason")
        .to_string();
    (decision, reason)
}

// ---- Fixture-based tests ----

#[test]
fn fixture_bash_ls_default_mode_returns_ask() {
    let input = load_fixture("bash-ls.json");
    let (stdout, exit_code) = run_hook(&input);
    assert_eq!(exit_code, 0);
    let (decision, reason) = parse_output(&stdout);
    assert_eq!(decision, "ask");
    assert!(!reason.is_empty());
}

#[test]
fn fixture_bash_git_status_plan_mode_returns_ask() {
    let input = load_fixture("bash-git-status.json");
    let (stdout, exit_code) = run_hook(&input);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "ask");
}

#[test]
fn fixture_read_file_accept_edits_returns_ask() {
    let input = load_fixture("read-file.json");
    let (stdout, exit_code) = run_hook(&input);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "ask");
}

#[test]
fn fixture_write_file_dont_ask_returns_deny() {
    let input = load_fixture("write-file.json");
    let (stdout, exit_code) = run_hook(&input);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "deny");
}

#[test]
fn fixture_edit_file_bypass_permissions_returns_allow() {
    let input = load_fixture("edit-file.json");
    let (stdout, exit_code) = run_hook(&input);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "allow");
}

#[test]
fn fixture_glob_search_default_mode_returns_ask() {
    let input = load_fixture("glob-search.json");
    let (stdout, exit_code) = run_hook(&input);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "ask");
}

#[test]
fn fixture_grep_search_plan_mode_returns_ask() {
    let input = load_fixture("grep-search.json");
    let (stdout, exit_code) = run_hook(&input);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "ask");
}

// ---- All permission modes end-to-end ----

fn make_input_with_mode(mode: &str) -> String {
    serde_json::json!({
        "sessionId": "sess-e2e-test",
        "transcriptPath": "/tmp/transcript.json",
        "cwd": "/tmp/test",
        "permissionMode": mode,
        "hookEventName": "PreToolUse",
        "toolName": "Bash",
        "toolInput": {"command": "echo test"},
        "toolUseId": "toolu_e2e"
    })
    .to_string()
}

#[test]
fn e2e_default_mode_returns_ask() {
    let (stdout, exit_code) = run_hook(&make_input_with_mode("default"));
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "ask");
}

#[test]
fn e2e_plan_mode_returns_ask() {
    let (stdout, exit_code) = run_hook(&make_input_with_mode("plan"));
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "ask");
}

#[test]
fn e2e_accept_edits_mode_returns_ask() {
    let (stdout, exit_code) = run_hook(&make_input_with_mode("acceptEdits"));
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "ask");
}

#[test]
fn e2e_dont_ask_mode_returns_deny() {
    let (stdout, exit_code) = run_hook(&make_input_with_mode("dontAsk"));
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "deny");
}

#[test]
fn e2e_bypass_permissions_mode_returns_allow() {
    let (stdout, exit_code) = run_hook(&make_input_with_mode("bypassPermissions"));
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "allow");
}

// ---- Error case tests ----

#[test]
fn malformed_json_returns_ask_decision() {
    let (stdout, exit_code) = run_hook("this is not json at all");
    assert_eq!(exit_code, 0);
    let (decision, reason) = parse_output(&stdout);
    assert_eq!(decision, "ask");
    assert!(reason.contains("Error"), "reason should contain error info");
}

#[test]
fn empty_stdin_returns_ask_decision() {
    let (stdout, exit_code) = run_hook("");
    assert_eq!(exit_code, 0);
    let (decision, reason) = parse_output(&stdout);
    assert_eq!(decision, "ask");
    assert!(reason.contains("Error"), "reason should contain error info");
}

#[test]
fn partial_json_returns_ask_decision() {
    let (stdout, exit_code) = run_hook(r#"{"sessionId": "incomplete"}"#);
    assert_eq!(exit_code, 0);
    let (decision, reason) = parse_output(&stdout);
    assert_eq!(decision, "ask");
    assert!(reason.contains("Error"), "reason should contain error info");
}

// ---- Output structure validation ----

#[test]
fn output_always_has_correct_structure() {
    let input = load_fixture("bash-ls.json");
    let (stdout, _) = run_hook(&input);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    // Verify structure
    assert!(value.get("hookSpecificOutput").is_some());
    let specific = &value["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    assert!(specific.get("permissionDecision").is_some());
    assert!(specific.get("permissionDecisionReason").is_some());
}

#[test]
fn error_output_still_has_correct_structure() {
    let (stdout, _) = run_hook("garbage");
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    assert!(value.get("hookSpecificOutput").is_some());
    let specific = &value["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    assert_eq!(specific["permissionDecision"], "ask");
}
