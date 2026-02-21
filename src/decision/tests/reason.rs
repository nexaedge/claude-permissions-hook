use super::{bash_input, make_config, make_input};
use crate::config::files::{FileRule, FilesConfig};
use crate::config::Config;
use crate::decision::evaluate;
use crate::protocol::FileOperation;
use serde_json::json;
use std::collections::HashSet;

/// Helper to extract the reason string from an evaluate() result.
fn reason_of(input: &crate::protocol::HookInput, config: &Config) -> String {
    evaluate(input, Some(config))
        .unwrap()
        .hook_specific_output
        .permission_decision_reason
}

/// Build a FileRule from a pattern and list of operations.
fn file_rule(pattern: &str, ops: &[FileOperation]) -> FileRule {
    FileRule {
        home_expanded_pattern: crate::config::normalize::files::expand_home(pattern),
        raw_pattern: pattern.to_string(),
        operations: ops.iter().copied().collect::<HashSet<_>>(),
        line: 0,
    }
}

fn make_files_config(files: FilesConfig) -> Config {
    Config {
        files: Some(files),
        ..Default::default()
    }
}

fn file_input(tool: &str, mode: &str, tool_input: serde_json::Value) -> crate::protocol::HookInput {
    make_input(tool, mode, tool_input)
}

fn file_reason(input: &crate::protocol::HookInput, config: &Config) -> String {
    evaluate(input, Some(config))
        .unwrap()
        .hook_specific_output
        .permission_decision_reason
}

// ---- Bash reason message tests ----

#[test]
fn reason_single_allow() {
    let config = make_config(&["git"], &[], &[]);
    let input = bash_input("git status", "default");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: allowed (git)"
    );
}

#[test]
fn reason_multi_allow() {
    let config = make_config(&["git", "cargo"], &[], &[]);
    let input = bash_input("git add . && cargo build", "default");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: allowed (git, cargo)"
    );
}

#[test]
fn reason_single_deny() {
    let config = make_config(&[], &["rm"], &[]);
    let input = bash_input("rm -rf /", "default");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: 'rm' is in your deny list"
    );
}

#[test]
fn reason_multi_deny() {
    let config = make_config(&["git"], &["rm"], &[]);
    let input = bash_input("git add && rm -rf /", "default");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: 'rm' is denied (in: git, rm)"
    );
}

#[test]
fn reason_single_ask() {
    let config = make_config(&[], &[], &["docker"]);
    let input = bash_input("docker run ubuntu", "default");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: 'docker' requires confirmation"
    );
}

#[test]
fn reason_multi_ask() {
    let config = make_config(&["git"], &[], &["docker"]);
    let input = bash_input("git pull && docker run ubuntu", "default");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: 'docker' requires confirmation (in: git, docker)"
    );
}

#[test]
fn reason_unlisted_triggers_ask_with_listed() {
    // git is allowed, ls is unlisted → defaults to Ask → Ask wins
    let config = make_config(&["git"], &[], &[]);
    let input = bash_input("git status && ls", "default");
    let reason = reason_of(&input, &config);
    assert_eq!(
        reason,
        "claude-permissions-hook: 'ls' requires confirmation (in: git, ls)"
    );
}

#[test]
fn reason_bypass_converts_ask_to_allow() {
    let config = make_config(&[], &[], &["docker"]);
    let input = bash_input("docker run", "bypassPermissions");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: allowed (docker)"
    );
}

#[test]
fn reason_dont_ask_converts_ask_to_deny() {
    let config = make_config(&[], &[], &["docker"]);
    let input = bash_input("docker run", "dontAsk");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: 'docker' denied by dontAsk mode"
    );
}

#[test]
fn reason_dont_ask_multi_converts_ask_to_deny() {
    let config = make_config(&["git"], &[], &["docker"]);
    let input = bash_input("git pull && docker run", "dontAsk");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: 'docker' denied by dontAsk mode (in: git, docker)"
    );
}

#[test]
fn reason_explicit_deny_not_affected_by_mode_text() {
    // An explicit deny in dontAsk mode should still say "deny list", not "dontAsk mode"
    let config = make_config(&[], &["rm"], &[]);
    let input = bash_input("rm -rf /", "dontAsk");
    assert_eq!(
        reason_of(&input, &config),
        "claude-permissions-hook: 'rm' is in your deny list"
    );
}

// ---- File tool reason string tests ----

#[test]
fn file_reason_allow() {
    let config = make_files_config(FilesConfig {
        allow: vec![file_rule("/**", &[FileOperation::Read])],
        ..Default::default()
    });
    let input = file_input("Read", "default", json!({"file_path": "/tmp/test.txt"}));
    assert_eq!(
        file_reason(&input, &config),
        "claude-permissions-hook: allowed read (/tmp/test.txt)"
    );
}

#[test]
fn file_reason_deny() {
    let home = std::env::var("HOME").unwrap();
    let config = make_files_config(FilesConfig {
        deny: vec![file_rule("~/.ssh/**", &[FileOperation::Write])],
        ..Default::default()
    });
    let path = format!("{home}/.ssh/id_rsa");
    let input = file_input("Write", "default", json!({"file_path": path.clone()}));
    assert_eq!(
        file_reason(&input, &config),
        format!("claude-permissions-hook: '{path}' denied by file rules (write)")
    );
}

#[test]
fn file_reason_ask() {
    let config = make_files_config(FilesConfig {
        ask: vec![file_rule("/**", &[FileOperation::Edit])],
        ..Default::default()
    });
    let input = file_input("Edit", "default", json!({"file_path": "/etc/hosts"}));
    assert_eq!(
        file_reason(&input, &config),
        "claude-permissions-hook: '/etc/hosts' requires confirmation (edit)"
    );
}

#[test]
fn file_reason_dont_ask_mode() {
    let config = make_files_config(FilesConfig {
        ask: vec![file_rule("/**", &[FileOperation::Read])],
        ..Default::default()
    });
    let input = file_input("Read", "dontAsk", json!({"file_path": "/tmp/secret.txt"}));
    assert_eq!(
        file_reason(&input, &config),
        "claude-permissions-hook: '/tmp/secret.txt' denied by dontAsk mode (read)"
    );
}

#[test]
fn file_reason_fail_closed() {
    let config = make_files_config(FilesConfig {
        allow: vec![file_rule("/**", &[FileOperation::Read])],
        ..Default::default()
    });
    let input = file_input("Read", "default", json!({}));
    let reason = file_reason(&input, &config);
    assert!(
        reason.contains("no file path provided"),
        "expected fail-closed reason, got: {reason}"
    );
}
