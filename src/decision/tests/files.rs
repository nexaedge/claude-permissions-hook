use super::{eval, make_config, make_input};
use crate::config::Config;
use crate::domain::rule::files::{FileRule, PathPattern};
use crate::domain::Decision;
use crate::domain::FileOperation;
use serde_json::json;
use std::collections::HashSet;

/// Build a Config with only files config (no bash).
fn make_files_config(files: Vec<FileRule>) -> Config {
    Config {
        files,
        has_files: true,
        ..Default::default()
    }
}

/// Build a FileRule from a pattern, decision, and list of operations.
fn file_rule(pattern: &str, decision: Decision, ops: &[FileOperation]) -> FileRule {
    FileRule {
        decision,
        path: PathPattern {
            raw: pattern.to_string(),
            expanded: crate::config::normalize::files::expand_home(pattern).unwrap(),
        },
        operations: ops.iter().copied().collect::<HashSet<_>>(),
    }
}

fn allow_rule(pattern: &str, ops: &[FileOperation]) -> FileRule {
    file_rule(pattern, Decision::Allow, ops)
}

fn deny_rule(pattern: &str, ops: &[FileOperation]) -> FileRule {
    file_rule(pattern, Decision::Deny, ops)
}

fn ask_rule(pattern: &str, ops: &[FileOperation]) -> FileRule {
    file_rule(pattern, Decision::Ask, ops)
}

fn file_input(tool: &str, mode: &str, tool_input: serde_json::Value) -> crate::protocol::HookInput {
    make_input(tool, mode, tool_input)
}

/// Extract decision from evaluate result.
fn file_decision(input: &crate::protocol::HookInput, config: &Config) -> Decision {
    eval(input, Some(config)).unwrap().0
}

// ---- Read file in CWD allow rule → allow ----

#[test]
fn file_read_in_cwd_allow() {
    let config = make_files_config(vec![allow_rule("<cwd>/**", &[FileOperation::Read])]);
    let input = file_input(
        "Read",
        "default",
        json!({"file_path": "/home/user/project/src/main.rs"}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Allow);
}

// ---- Write to denied path → deny ----

#[test]
fn file_write_to_denied_path() {
    let home = std::env::var("HOME").unwrap();
    let config = make_files_config(vec![deny_rule("~/.ssh/**", &[FileOperation::Write])]);
    let input = file_input(
        "Write",
        "default",
        json!({"file_path": format!("{home}/.ssh/id_rsa")}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Deny);
}

// ---- Edit outside CWD → ask (catch-all) ----

#[test]
fn file_edit_outside_cwd_ask() {
    let config = make_files_config(vec![
        allow_rule("<cwd>/**", &[FileOperation::Edit]),
        ask_rule("/**", &[FileOperation::Edit]),
    ]);
    // Path outside CWD (/home/user/project) → matches ask catch-all, not allow
    let input = file_input("Edit", "default", json!({"file_path": "/etc/hosts"}));
    assert_eq!(file_decision(&input, &config), Decision::Ask);
}

// ---- Glob with no path field (defaults to CWD) → evaluate CWD ----

#[test]
fn file_glob_no_path_defaults_to_cwd() {
    // Glob without path → extract_file_paths defaults to CWD (/home/user/project)
    // Pattern must match the directory itself (not just contents)
    let config = make_files_config(vec![allow_rule(
        "/home/user/project",
        &[FileOperation::Glob],
    )]);
    let input = file_input("Glob", "default", json!({"pattern": "**/*.rs"}));
    assert_eq!(file_decision(&input, &config), Decision::Allow);
}

#[test]
fn file_glob_explicit_path_inside_cwd() {
    let config = make_files_config(vec![allow_rule("<cwd>/**", &[FileOperation::Glob])]);
    // Glob with explicit path inside CWD → matches <cwd>/**
    let input = file_input(
        "Glob",
        "default",
        json!({"pattern": "**/*.rs", "path": "/home/user/project/src"}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Allow);
}

// ---- Grep with explicit path → evaluate that path ----

#[test]
fn file_grep_explicit_path() {
    let config = make_files_config(vec![deny_rule("/etc/**", &[FileOperation::Grep])]);
    let input = file_input(
        "Grep",
        "default",
        json!({"pattern": "password", "path": "/etc/passwd"}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Deny);
}

// ---- No files config → None (no opinion on file tools) ----

#[test]
fn file_no_files_config_returns_none() {
    // Config with bash only, no files section
    let config = make_config(&["git"], &[], &[]);
    let input = file_input("Read", "default", json!({"file_path": "/tmp/test.txt"}));
    assert!(eval(&input, Some(&config)).is_none());
}

// ---- Deny > ask > allow precedence ----

#[test]
fn file_deny_beats_ask_beats_allow() {
    let config = make_files_config(vec![
        allow_rule("/**", &[FileOperation::Read]),
        ask_rule("/**", &[FileOperation::Read]),
        deny_rule("/**", &[FileOperation::Read]),
    ]);
    let input = file_input("Read", "default", json!({"file_path": "/any/path"}));
    assert_eq!(file_decision(&input, &config), Decision::Deny);
}

#[test]
fn file_ask_beats_allow() {
    let config = make_files_config(vec![
        allow_rule("/**", &[FileOperation::Write]),
        ask_rule("/**", &[FileOperation::Write]),
    ]);
    let input = file_input("Write", "default", json!({"file_path": "/any/path"}));
    assert_eq!(file_decision(&input, &config), Decision::Ask);
}

// ---- Mode modifier: bypass + ask → allow ----

#[test]
fn file_bypass_mode_ask_to_allow() {
    let config = make_files_config(vec![ask_rule("/**", &[FileOperation::Read])]);
    let input = file_input(
        "Read",
        "bypassPermissions",
        json!({"file_path": "/tmp/test.txt"}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Allow);
}

// ---- Mode modifier: dontAsk + ask → deny ----

#[test]
fn file_dont_ask_mode_ask_to_deny() {
    let config = make_files_config(vec![ask_rule("/**", &[FileOperation::Write])]);
    let input = file_input("Write", "dontAsk", json!({"file_path": "/tmp/test.txt"}));
    assert_eq!(file_decision(&input, &config), Decision::Deny);
}

// ---- Fail-closed: missing file_path → parse error (None from to_request) ----
// Note: fail-closed behavior for parse errors is now handled at the CLI boundary
// (cli/hook.rs), not in the decision engine. These tests verify the domain path
// returns None for parse errors.

#[test]
fn file_missing_file_path_returns_none() {
    let config = make_files_config(vec![allow_rule("/**", &[FileOperation::Read])]);
    // Read with no file_path → parse error → to_request returns None
    let input = file_input("Read", "default", json!({}));
    assert!(eval(&input, Some(&config)).is_none());
}

#[test]
fn file_missing_file_path_write_returns_none() {
    let config = make_files_config(vec![allow_rule("/**", &[FileOperation::Write])]);
    let input = file_input("Write", "default", json!({}));
    assert!(eval(&input, Some(&config)).is_none());
}

// ---- Operation mismatch → no match ----

#[test]
fn file_operation_mismatch_returns_none() {
    let config = make_files_config(vec![allow_rule("/**", &[FileOperation::Read])]);
    // Write tool but allow rule only has read → no match → None
    let input = file_input("Write", "default", json!({"file_path": "/tmp/test.txt"}));
    assert!(eval(&input, Some(&config)).is_none());
}

// ---- Variable expansion in rules ----

#[test]
fn file_cwd_variable_expansion() {
    let config = make_files_config(vec![allow_rule(
        "<cwd>/**",
        &[
            FileOperation::Read,
            FileOperation::Write,
            FileOperation::Edit,
        ],
    )]);
    let input = file_input(
        "Write",
        "default",
        json!({"file_path": "/home/user/project/src/lib.rs"}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Allow);
}

#[test]
fn file_home_variable_expansion() {
    let home = std::env::var("HOME").unwrap();
    let config = make_files_config(vec![deny_rule("<home>/.ssh/**", &[FileOperation::Read])]);
    let input = file_input(
        "Read",
        "default",
        json!({"file_path": format!("{home}/.ssh/config")}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Deny);
}

#[test]
fn file_tilde_expansion() {
    let home = std::env::var("HOME").unwrap();
    let config = make_files_config(vec![deny_rule("~/.env", &[FileOperation::Read])]);
    let input = file_input(
        "Read",
        "default",
        json!({"file_path": format!("{home}/.env")}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Deny);
}

// ---- Mode modifiers don't change Allow/Deny from config ----

#[test]
fn file_bypass_mode_allow_stays_allow() {
    let config = make_files_config(vec![allow_rule("/**", &[FileOperation::Read])]);
    let input = file_input(
        "Read",
        "bypassPermissions",
        json!({"file_path": "/tmp/test.txt"}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Allow);
}

#[test]
fn file_bypass_mode_deny_stays_deny() {
    let config = make_files_config(vec![deny_rule("/**", &[FileOperation::Write])]);
    let input = file_input(
        "Write",
        "bypassPermissions",
        json!({"file_path": "/tmp/test.txt"}),
    );
    assert_eq!(file_decision(&input, &config), Decision::Deny);
}

#[test]
fn file_dont_ask_mode_deny_stays_deny() {
    let config = make_files_config(vec![deny_rule("/**", &[FileOperation::Write])]);
    let input = file_input("Write", "dontAsk", json!({"file_path": "/tmp/test.txt"}));
    assert_eq!(file_decision(&input, &config), Decision::Deny);
}

// ---- Backwards compat: config with both bash and files ----

#[test]
fn file_config_with_both_bash_and_files() {
    use super::rules_of_with_decision;
    let config = Config {
        bash: rules_of_with_decision(&["git"], Decision::Allow),
        has_bash: true,
        files: vec![allow_rule("<cwd>/**", &[FileOperation::Read])],
        has_files: true,
    };
    // Bash still evaluates independently
    let bash_in = super::bash_input("git status", "default");
    assert_eq!(eval(&bash_in, Some(&config)).unwrap().0, Decision::Allow);
    // File tools also evaluate independently
    let read_in = file_input(
        "Read",
        "default",
        json!({"file_path": "/home/user/project/file.txt"}),
    );
    assert_eq!(file_decision(&read_in, &config), Decision::Allow);
}

// ---- Unknown tool still returns None ----

#[test]
fn unknown_tool_with_files_config_returns_none() {
    let config = make_files_config(vec![allow_rule("/**", &[FileOperation::Read])]);
    let input = make_input("NotebookEdit", "default", json!({}));
    assert!(eval(&input, Some(&config)).is_none());
}

// ---- Path normalization in evaluation ----

#[test]
fn file_relative_path_normalized_to_cwd() {
    let config = make_files_config(vec![allow_rule("<cwd>/**", &[FileOperation::Read])]);
    // file_path is relative (shouldn't happen in practice but verifies normalization)
    let input = file_input("Read", "default", json!({"file_path": "src/main.rs"}));
    // Normalized: /home/user/project/src/main.rs → matches <cwd>/**
    assert_eq!(file_decision(&input, &config), Decision::Allow);
}
