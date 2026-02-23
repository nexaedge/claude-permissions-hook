//! Pure domain-level decision tests.
//!
//! These tests construct `ToolRequest` directly (no protocol types) to prove
//! the decision layer works independently of protocol parsing/deserialization.

use crate::config::Config;
use crate::domain::rule::bash::{BashConditions, BashRule};
use crate::domain::rule::files::{FileRule, PathPattern};
use crate::domain::{
    CommandSegment, Decision, FileOperation, PermissionMode, ProgramName, ResolvedPath,
    ToolCategory, ToolRequest,
};
use std::collections::HashSet;

fn segment(name: &str, args: &[&str]) -> CommandSegment {
    CommandSegment {
        program: ProgramName::parse(name).unwrap(),
        args: args.iter().map(|s| s.to_string()).collect(),
    }
}

fn bash_config(allow: &[&str], deny: &[&str], ask: &[&str]) -> Config {
    let mut rules = Vec::new();
    for &p in allow {
        rules.push(BashRule {
            decision: Decision::Allow,
            program: ProgramName::parse(p).unwrap(),
            conditions: BashConditions::default(),
        });
    }
    for &p in deny {
        rules.push(BashRule {
            decision: Decision::Deny,
            program: ProgramName::parse(p).unwrap(),
            conditions: BashConditions::default(),
        });
    }
    for &p in ask {
        rules.push(BashRule {
            decision: Decision::Ask,
            program: ProgramName::parse(p).unwrap(),
            conditions: BashConditions::default(),
        });
    }
    Config {
        bash: Some(rules),
        ..Default::default()
    }
}

fn files_config(rules: Vec<FileRule>) -> Config {
    Config {
        files: Some(rules),
        ..Default::default()
    }
}

fn file_rule(pattern: &str, expanded: &str, decision: Decision, ops: &[FileOperation]) -> FileRule {
    FileRule {
        decision,
        path: PathPattern {
            raw: pattern.to_string(),
            expanded: expanded.to_string(),
        },
        operations: ops.iter().copied().collect::<HashSet<_>>(),
    }
}

fn decide(
    request: &ToolRequest,
    cwd: &str,
    mode: PermissionMode,
    config: &Config,
) -> Option<(Decision, String)> {
    crate::decision::evaluate(request, cwd, &mode, config)
}

// ---- Bash: single command ----

#[test]
fn bash_allow_single() {
    let request = ToolRequest::Bash {
        segments: vec![segment("git", &["status"])],
    };
    let (d, _) = decide(
        &request,
        "/tmp",
        PermissionMode::Default,
        &bash_config(&["git"], &[], &[]),
    )
    .unwrap();
    assert_eq!(d, Decision::Allow);
}

#[test]
fn bash_deny_single() {
    let request = ToolRequest::Bash {
        segments: vec![segment("rm", &["-rf", "/"])],
    };
    let (d, _) = decide(
        &request,
        "/tmp",
        PermissionMode::Default,
        &bash_config(&[], &["rm"], &[]),
    )
    .unwrap();
    assert_eq!(d, Decision::Deny);
}

#[test]
fn bash_ask_single() {
    let request = ToolRequest::Bash {
        segments: vec![segment("docker", &["run"])],
    };
    let (d, _) = decide(
        &request,
        "/tmp",
        PermissionMode::Default,
        &bash_config(&[], &[], &["docker"]),
    )
    .unwrap();
    assert_eq!(d, Decision::Ask);
}

// ---- Bash: multi-command aggregation ----

#[test]
fn bash_deny_wins_over_allow() {
    let request = ToolRequest::Bash {
        segments: vec![segment("git", &["add"]), segment("rm", &["-rf", "/"])],
    };
    let (d, _) = decide(
        &request,
        "/tmp",
        PermissionMode::Default,
        &bash_config(&["git"], &["rm"], &[]),
    )
    .unwrap();
    assert_eq!(d, Decision::Deny);
}

#[test]
fn bash_unlisted_returns_none() {
    let request = ToolRequest::Bash {
        segments: vec![segment("foo", &[])],
    };
    let result = decide(
        &request,
        "/tmp",
        PermissionMode::Default,
        &bash_config(&["git"], &[], &[]),
    );
    assert!(result.is_none());
}

// ---- Bash: mode modifiers ----

#[test]
fn bash_bypass_converts_ask_to_allow() {
    let request = ToolRequest::Bash {
        segments: vec![segment("docker", &["run"])],
    };
    let (d, _) = decide(
        &request,
        "/tmp",
        PermissionMode::BypassPermissions,
        &bash_config(&[], &[], &["docker"]),
    )
    .unwrap();
    assert_eq!(d, Decision::Allow);
}

#[test]
fn bash_dont_ask_converts_ask_to_deny() {
    let request = ToolRequest::Bash {
        segments: vec![segment("docker", &["run"])],
    };
    let (d, _) = decide(
        &request,
        "/tmp",
        PermissionMode::DontAsk,
        &bash_config(&[], &[], &["docker"]),
    )
    .unwrap();
    assert_eq!(d, Decision::Deny);
}

// ---- File tools ----

#[test]
fn file_allow_read() {
    let config = files_config(vec![file_rule(
        "/tmp/**",
        "/tmp/**",
        Decision::Allow,
        &[FileOperation::Read],
    )]);
    let request = ToolRequest::File {
        operation: FileOperation::Read,
        paths: vec![ResolvedPath {
            raw: "/tmp/test.txt".into(),
            normalized: "/tmp/test.txt".into(),
        }],
    };
    let (d, _) = decide(&request, "/tmp", PermissionMode::Default, &config).unwrap();
    assert_eq!(d, Decision::Allow);
}

#[test]
fn file_deny_write() {
    let config = files_config(vec![file_rule(
        "/etc/**",
        "/etc/**",
        Decision::Deny,
        &[FileOperation::Write],
    )]);
    let request = ToolRequest::File {
        operation: FileOperation::Write,
        paths: vec![ResolvedPath {
            raw: "/etc/passwd".into(),
            normalized: "/etc/passwd".into(),
        }],
    };
    let (d, _) = decide(&request, "/tmp", PermissionMode::Default, &config).unwrap();
    assert_eq!(d, Decision::Deny);
}

#[test]
fn file_no_match_returns_none() {
    let config = files_config(vec![file_rule(
        "/tmp/**",
        "/tmp/**",
        Decision::Allow,
        &[FileOperation::Read],
    )]);
    let request = ToolRequest::File {
        operation: FileOperation::Write,
        paths: vec![ResolvedPath {
            raw: "/tmp/test.txt".into(),
            normalized: "/tmp/test.txt".into(),
        }],
    };
    assert!(decide(&request, "/tmp", PermissionMode::Default, &config).is_none());
}

#[test]
fn file_bypass_ask_to_allow() {
    let config = files_config(vec![file_rule(
        "/**",
        "/**",
        Decision::Ask,
        &[FileOperation::Edit],
    )]);
    let request = ToolRequest::File {
        operation: FileOperation::Edit,
        paths: vec![ResolvedPath {
            raw: "/etc/hosts".into(),
            normalized: "/etc/hosts".into(),
        }],
    };
    let (d, _) = decide(&request, "/tmp", PermissionMode::BypassPermissions, &config).unwrap();
    assert_eq!(d, Decision::Allow);
}

// ---- Unknown + ParseError ----

#[test]
fn unknown_returns_none() {
    let config = bash_config(&["git"], &[], &[]);
    assert!(decide(
        &ToolRequest::Unknown,
        "/tmp",
        PermissionMode::Default,
        &config
    )
    .is_none());
}

#[test]
fn parse_error_bash_with_config_returns_ask() {
    let config = bash_config(&["git"], &[], &[]);
    let request = ToolRequest::ParseError {
        category: ToolCategory::Bash,
        reason: "missing command".into(),
    };
    let (d, _) = decide(&request, "/tmp", PermissionMode::Default, &config).unwrap();
    assert_eq!(d, Decision::Ask);
}

#[test]
fn parse_error_file_without_config_returns_none() {
    // Config has bash only, not files — so file parse errors get no opinion
    let config = bash_config(&["git"], &[], &[]);
    let request = ToolRequest::ParseError {
        category: ToolCategory::File,
        reason: "no file path".into(),
    };
    assert!(decide(&request, "/tmp", PermissionMode::Default, &config).is_none());
}

// ---- CWD expansion in file rules ----

#[test]
fn file_cwd_expansion_matches() {
    let config = files_config(vec![file_rule(
        "<cwd>/**",
        "<cwd>/**",
        Decision::Allow,
        &[FileOperation::Read],
    )]);
    let request = ToolRequest::File {
        operation: FileOperation::Read,
        paths: vec![ResolvedPath {
            raw: "/project/src/main.rs".into(),
            normalized: "/project/src/main.rs".into(),
        }],
    };
    let (d, _) = decide(&request, "/project", PermissionMode::Default, &config).unwrap();
    assert_eq!(d, Decision::Allow);
}
