// Shared test helpers for integration tests.
// Extracted from cli_test.rs helpers — used by cli_contract.rs, cli_flows.rs, diff_harness.rs.
#![allow(dead_code)]

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tempfile::NamedTempFile;

pub fn binary_path() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_claude-permissions-hook"));
    assert!(path.exists(), "binary not found at {}", path.display());
    path
}

pub fn run_hook(stdin_input: &str) -> (String, String, i32) {
    run_hook_args(stdin_input, &[])
}

pub fn run_hook_with_config(stdin_input: &str, config_content: &str) -> (String, String, i32) {
    let mut tmpfile = NamedTempFile::new().expect("failed to create temp config");
    tmpfile
        .write_all(config_content.as_bytes())
        .expect("failed to write config");
    let config_path = tmpfile.path().to_str().unwrap().to_string();
    run_hook_args(stdin_input, &["--config", &config_path])
}

/// Runs the binary with the given stdin and extra args.
/// Returns (stdout, stderr, exit_code).
pub fn run_hook_args(stdin_input: &str, extra_args: &[&str]) -> (String, String, i32) {
    let mut cmd = Command::new(binary_path());
    cmd.arg("hook");
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd
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
            if let Err(e) = write_result {
                if e.kind() != ErrorKind::BrokenPipe {
                    return Err(e);
                }
            }
            child.wait_with_output()
        })
        .expect("failed to execute binary");

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr not valid UTF-8");
    let exit_code = output.status.code().unwrap_or(-1);
    (stdout, stderr, exit_code)
}

pub fn make_input_json(tool_name: &str, mode: &str, tool_input: serde_json::Value) -> String {
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

pub fn bash_input_json(command: &str, mode: &str) -> String {
    make_input_json("Bash", mode, serde_json::json!({"command": command}))
}

/// Parses the hook output JSON and returns the full hookSpecificOutput value.
pub fn parse_hook_output(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON")
}
