use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::NamedTempFile;

// ---- Test helpers ----

fn binary_path() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_claude-permissions-hook"));
    assert!(path.exists(), "binary not found at {}", path.display());
    path
}

fn run_hook(stdin_input: &str) -> (String, i32) {
    run_hook_args(stdin_input, &[])
}

fn run_hook_with_config(stdin_input: &str, config_content: &str) -> (String, i32) {
    let mut tmpfile = NamedTempFile::new().expect("failed to create temp config");
    tmpfile
        .write_all(config_content.as_bytes())
        .expect("failed to write config");
    let config_path = tmpfile.path().to_str().unwrap().to_string();
    run_hook_args(stdin_input, &["--config", &config_path])
}

fn run_hook_args(stdin_input: &str, extra_args: &[&str]) -> (String, i32) {
    let mut cmd = Command::new(binary_path());
    cmd.arg("hook");
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output: Output = cmd
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

fn load_fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()))
}

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

fn make_input_json(tool_name: &str, mode: &str, tool_input: serde_json::Value) -> String {
    serde_json::json!({
        "sessionId": "sess-e2e-test",
        "transcriptPath": "/tmp/transcript.json",
        "cwd": "/tmp/test",
        "permissionMode": mode,
        "hookEventName": "PreToolUse",
        "toolName": tool_name,
        "toolInput": tool_input,
        "toolUseId": "toolu_e2e"
    })
    .to_string()
}

fn bash_input_json(command: &str, mode: &str) -> String {
    make_input_json("Bash", mode, serde_json::json!({"command": command}))
}

fn assert_empty_json(stdout: &str) {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value, serde_json::json!({}));
}

// ---- Test macros ----

/// No-config fixture: loads a JSON fixture, runs without --config, expects "ask".
macro_rules! no_config_fixture_test {
    ($name:ident, fixture: $fixture:expr) => {
        #[test]
        fn $name() {
            let (stdout, exit_code) = run_hook(&load_fixture($fixture));
            assert_eq!(exit_code, 0);
            let (decision, _) = parse_output(&stdout);
            assert_eq!(decision, "ask");
        }
    };
}

/// Config test expecting a specific decision (allow/deny/ask).
macro_rules! config_decision_test {
    ($name:ident, cmd: $cmd:expr, mode: $mode:expr, config: $cfg:expr, expect: $expected:expr) => {
        #[test]
        fn $name() {
            let (stdout, exit_code) = run_hook_with_config(&bash_input_json($cmd, $mode), $cfg);
            assert_eq!(exit_code, 0);
            let (decision, _) = parse_output(&stdout);
            assert_eq!(decision, $expected);
        }
    };
}

/// Config test expecting empty JSON response (no opinion).
macro_rules! config_empty_test {
    ($name:ident, cmd: $cmd:expr, mode: $mode:expr, config: $cfg:expr) => {
        #[test]
        fn $name() {
            let (stdout, exit_code) = run_hook_with_config(&bash_input_json($cmd, $mode), $cfg);
            assert_eq!(exit_code, 0);
            assert_empty_json(&stdout);
        }
    };
}

/// Stdin error test: sends raw string, expects "ask" with reason containing substring.
macro_rules! stdin_error_test {
    ($name:ident, input: $input:expr, reason_contains: $substr:expr) => {
        #[test]
        fn $name() {
            let (stdout, exit_code) = run_hook($input);
            assert_eq!(exit_code, 0);
            let (decision, reason) = parse_output(&stdout);
            assert_eq!(decision, "ask");
            assert!(
                reason.contains($substr),
                "reason should contain '{}': {reason}",
                $substr
            );
        }
    };
}

// ---- No-config: all tools return "ask" regardless of mode ----

no_config_fixture_test!(no_config_bash_default, fixture: "bash-ls.json");
no_config_fixture_test!(no_config_bash_plan, fixture: "bash-git-status.json");
no_config_fixture_test!(no_config_read_accept_edits, fixture: "read-file.json");
no_config_fixture_test!(no_config_write_dont_ask, fixture: "write-file.json");
no_config_fixture_test!(no_config_edit_bypass, fixture: "edit-file.json");
no_config_fixture_test!(no_config_glob_default, fixture: "glob-search.json");
no_config_fixture_test!(no_config_grep_plan, fixture: "grep-search.json");

// ---- With config: non-Bash tools return empty {} ----

#[test]
fn config_non_bash_tool_returns_empty_json() {
    let input = make_input_json(
        "Read",
        "default",
        serde_json::json!({"file_path": "/tmp/x"}),
    );
    let (stdout, exit_code) = run_hook_with_config(&input, r#"bash { allow "git" }"#);
    assert_eq!(exit_code, 0);
    assert_empty_json(&stdout);
}

// ---- With config: representative decision types (logic tested in unit tests) ----

config_decision_test!(config_allowed_program,
    cmd: "git status", mode: "default",
    config: r#"bash { allow "git" }"#, expect: "allow");

config_decision_test!(config_denied_program,
    cmd: "rm -rf /", mode: "default",
    config: r#"bash { deny "rm" }"#, expect: "deny");

config_decision_test!(config_ask_program,
    cmd: "docker run ubuntu", mode: "default",
    config: r#"bash { ask "docker" }"#, expect: "ask");

config_empty_test!(config_unlisted_program,
    cmd: "python script.py", mode: "default",
    config: r#"bash { allow "git" }"#);

// ---- With config: fail-closed on unparseable commands ----

config_decision_test!(config_parse_error_returns_ask,
    cmd: "git add . &&", mode: "default",
    config: r#"bash { allow "git" }"#, expect: "ask");

config_decision_test!(config_empty_command_returns_ask,
    cmd: "", mode: "default",
    config: r#"bash { allow "git" }"#, expect: "ask");

config_decision_test!(config_arithmetic_only_returns_ask,
    cmd: "(( x + 1 ))", mode: "default",
    config: r#"bash { allow "git" }"#, expect: "ask");

// ---- Config error handling ----

#[test]
fn invalid_config_path_returns_ask_with_error() {
    let input = bash_input_json("git status", "default");
    let (stdout, exit_code) = run_hook_args(&input, &["--config", "/tmp/nonexistent-12345.kdl"]);
    assert_eq!(exit_code, 0);
    let (decision, reason) = parse_output(&stdout);
    assert_eq!(decision, "ask");
    assert!(
        reason.contains("Config error"),
        "reason should mention config error: {reason}"
    );
}

#[test]
fn invalid_config_syntax_returns_ask_with_error() {
    let mut tmpfile = NamedTempFile::new().unwrap();
    tmpfile.write_all(b"invalid {{ kdl {{ syntax").unwrap();
    let config_path = tmpfile.path().to_str().unwrap().to_string();
    let input = bash_input_json("git status", "default");
    let (stdout, exit_code) = run_hook_args(&input, &["--config", &config_path]);
    assert_eq!(exit_code, 0);
    let (decision, reason) = parse_output(&stdout);
    assert_eq!(decision, "ask");
    assert!(
        reason.contains("Config error"),
        "reason should mention config error: {reason}"
    );
}

// ---- Stdin error cases ----

stdin_error_test!(malformed_json, input: "this is not json at all", reason_contains: "Error");
stdin_error_test!(empty_stdin, input: "", reason_contains: "Error");
stdin_error_test!(partial_json, input: r#"{"sessionId": "incomplete"}"#, reason_contains: "Error");

// ---- Output structure validation ----

#[test]
fn output_always_has_correct_structure() {
    let (stdout, _) = run_hook(&load_fixture("bash-ls.json"));
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

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
