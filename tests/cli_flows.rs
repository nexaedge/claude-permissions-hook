// Representative integration flow tests.
// One test per major capability path — proves the full pipeline works
// without duplicating the exhaustive matrix in cli_test.rs.

mod common;

use common::{
    bash_input_json, binary_path, make_input_json, parse_hook_output, run_hook_with_config,
};

const FLOW_CONFIG: &str = r#"
bash {
    allow "git"
    deny "rm"
    ask "docker"
}
files {
    deny "~/.ssh/**" "read" "write" "edit"
    "<cwd>/**" {
        allow "read" "glob" "grep"
        ask "write" "edit"
    }
}
"#;

fn assert_decision(stdout: &str, expected: &str) {
    let value = parse_hook_output(stdout);
    let decision = value["hookSpecificOutput"]["permissionDecision"]
        .as_str()
        .expect("missing permissionDecision");
    assert_eq!(decision, expected);
}

fn assert_empty(stdout: &str) {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value, serde_json::json!({}));
}

// ---- Bash flows: one per decision type ----

#[test]
fn flow_bash_allow() {
    let input = bash_input_json("git status", "default");
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_decision(&stdout, "allow");
}

#[test]
fn flow_bash_deny() {
    let input = bash_input_json("rm -rf /", "default");
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_decision(&stdout, "deny");
}

#[test]
fn flow_bash_ask() {
    let input = bash_input_json("docker run nginx", "default");
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_decision(&stdout, "ask");
}

#[test]
fn flow_bash_unlisted() {
    let input = bash_input_json("python script.py", "default");
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_empty(&stdout);
}

// ---- File tool flows: one per decision type ----

#[test]
fn flow_file_read_allow() {
    let input = make_input_json(
        "Read",
        "default",
        serde_json::json!({"file_path": "/tmp/test/src/main.rs"}),
    );
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_decision(&stdout, "allow");
}

#[test]
fn flow_file_read_deny() {
    let input = make_input_json(
        "Read",
        "default",
        serde_json::json!({"file_path": "~/.ssh/id_rsa"}),
    );
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_decision(&stdout, "deny");
}

#[test]
fn flow_file_write_ask() {
    let input = make_input_json(
        "Write",
        "default",
        serde_json::json!({"file_path": "/tmp/test/new.rs", "content": "data"}),
    );
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_decision(&stdout, "ask");
}

// ---- Mode modifier flows ----

#[test]
fn flow_mode_bypass_converts_ask_to_allow() {
    let input = bash_input_json("docker run nginx", "bypassPermissions");
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_decision(&stdout, "allow");
}

#[test]
fn flow_mode_dontask_converts_ask_to_deny() {
    let input = bash_input_json("docker run nginx", "dontAsk");
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_decision(&stdout, "deny");
}

// ---- Multi-command flow ----

#[test]
fn flow_multi_command_pipe_highest_severity_wins() {
    let input = bash_input_json("git log | rm -rf /", "default");
    let (stdout, _, exit_code) = run_hook_with_config(&input, FLOW_CONFIG);
    assert_eq!(exit_code, 0);
    assert_decision(&stdout, "deny");
}

// ---- Regression: $HOME unset must not panic ----

/// When $HOME is not set, file-tool lookups must return a graceful JSON
/// response instead of panicking with exit 101.
#[test]
fn flow_home_unset_file_tool_returns_valid_json() {
    use std::io::Write;
    let input = make_input_json(
        "Read",
        "default",
        serde_json::json!({"file_path": "/tmp/test.txt"}),
    );
    let mut tmpfile = tempfile::NamedTempFile::new().expect("failed to create temp config");
    tmpfile
        .write_all(
            br#"files {
    deny "~/.ssh/**" "read"
}"#,
        )
        .expect("failed to write config");
    let config_path = tmpfile.path().to_str().unwrap().to_string();

    let output = std::process::Command::new(binary_path())
        .arg("hook")
        .arg("--config")
        .arg(&config_path)
        .env_remove("HOME")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(input.as_bytes()).ok();
            child.wait_with_output()
        })
        .expect("failed to execute binary");

    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let exit_code = output.status.code().unwrap_or(-1);

    // Must exit 0 (not 101 panic) and emit valid JSON
    assert_eq!(exit_code, 0, "binary must not panic; stderr: {stderr}");
    let value: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("stdout must be valid JSON even when $HOME is unset");
    // Fail-closed: $HOME unset → home_expanded_pattern is Err → lookup returns Ask.
    // The deny rule `~/.ssh/**` has an unexpandable pattern, so any path lookup
    // must return Ask (not deny, not allow, not empty).
    let decision = value["hookSpecificOutput"]["permissionDecision"]
        .as_str()
        .expect("expected permissionDecision in output");
    assert_eq!(
        decision, "ask",
        "expected fail-closed ask when $HOME is unset"
    );
}
