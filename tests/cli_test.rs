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
            use std::io::{ErrorKind, Write};
            let write_result = child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin_input.as_bytes());
            // Ignore BrokenPipe: child may exit before reading stdin (e.g. config errors)
            if let Err(e) = write_result {
                if e.kind() != ErrorKind::BrokenPipe {
                    return Err(e);
                }
            }
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
        "session_id": "sess-e2e-test",
        "transcript_path": "/tmp/transcript.json",
        "cwd": "/tmp/test",
        "permission_mode": mode,
        "hook_event_name": "PreToolUse",
        "tool_name": tool_name,
        "tool_input": tool_input,
        "tool_use_id": "toolu_e2e"
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

// ---- Config file not found: unique error type distinct from parse failure ----

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

// ---- Empty command string: fail-closed behavior with config present ----

#[test]
fn edge_empty_command() {
    let input = make_input_json("Bash", "default", serde_json::json!({"command": ""}));
    let config = r#"bash { deny "rm" }"#;
    let (stdout, exit_code) = run_hook_with_config(&input, config);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "ask", "empty command should fail-closed to ask");
}

// ---- Separate flags match combined-flag rule: integration of parse + match ----

#[test]
fn edge_flag_expansion_r_space_f() {
    // `-r -f` should produce the same result as `-rf`
    let config = load_fixture("argument-matching.kdl");
    let (stdout, exit_code) =
        run_hook_with_config(&bash_input_json("rm -r -f /", "default"), &config);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(
        decision, "deny",
        "rm -r -f / should match the same as rm -rf /"
    );
}

// ==== File tool: allow paths verified with minimal config (no conflicting ask tier) ====

#[test]
fn ft_write_cwd_allow() {
    let config = r#"
        files {
            deny "~/.ssh/**" "write"
            "<cwd>/**" { allow "write" }
        }
    "#;
    let input = make_input_json(
        "Write",
        "default",
        serde_json::json!({"file_path": "/tmp/test/new.rs", "content": "data"}),
    );
    let (stdout, exit_code) = run_hook_with_config(&input, config);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "allow");
}

#[test]
fn ft_edit_cwd_allow() {
    let config = r#"
        files {
            deny "~/.ssh/**" "edit"
            "<cwd>/**" { allow "edit" }
        }
    "#;
    let input = make_input_json(
        "Edit",
        "default",
        serde_json::json!({"file_path": "/tmp/test/lib.rs", "old_string": "a", "new_string": "b"}),
    );
    let (stdout, exit_code) = run_hook_with_config(&input, config);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "allow");
}

// ---- Backwards compatibility: cross-config-section routing ----

// Files-only config + Bash tool → empty JSON (no bash section → no opinion)
#[test]
fn ft_compat_files_only_config_bash_tool() {
    let files_only = r#"files { allow "/tmp/**" "read" "write" "edit" "glob" "grep" }"#;
    let (stdout, exit_code) =
        run_hook_with_config(&bash_input_json("git status", "default"), files_only);
    assert_eq!(exit_code, 0);
    assert_empty_json(&stdout);
}

// Mixed config: Bash tool evaluated against bash rules
#[test]
fn ft_compat_mixed_config_bash_uses_bash_rules() {
    let mixed = r#"
        bash { allow "git"; deny "rm" }
        files { allow "/tmp/**" "read" }
    "#;
    let (stdout, exit_code) =
        run_hook_with_config(&bash_input_json("git status", "default"), mixed);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "allow");
}

// Mixed config: File tool evaluated against file rules
#[test]
fn ft_compat_mixed_config_file_uses_file_rules() {
    let mixed = r#"
        bash { allow "git"; deny "rm" }
        files { allow "/tmp/**" "read" }
    "#;
    let input = make_input_json(
        "Read",
        "default",
        serde_json::json!({"file_path": "/tmp/test/file.rs"}),
    );
    let (stdout, exit_code) = run_hook_with_config(&input, mixed);
    assert_eq!(exit_code, 0);
    let (decision, _) = parse_output(&stdout);
    assert_eq!(decision, "allow");
}

// ---- Regression: no config mode produces ask for all fixture tools ----

#[test]
fn no_config_all_tools_return_ask() {
    // Verify that all tool fixture types return ask without config.
    // Spot-check with two representative fixtures; full no-config contract
    // is in cli_contract.rs.
    for fixture in &["bash-ls.json", "read-file.json"] {
        let (stdout, exit_code) = run_hook(&load_fixture(fixture));
        assert_eq!(exit_code, 0, "fixture {fixture} should exit 0");
        let (decision, _) = parse_output(&stdout);
        assert_eq!(
            decision, "ask",
            "fixture {fixture} should return ask without config"
        );
    }
}
