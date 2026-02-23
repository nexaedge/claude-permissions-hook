use serde_json::Value;

use crate::command;
use crate::domain::path;
use crate::domain::CommandSegment;
use crate::domain::FileOperation;
use crate::domain::ResolvedPath;
use crate::domain::ToolCategory;

use super::RawHookInput;

/// Typed representation of a tool invocation, parsed and validated at the protocol boundary.
///
/// Every variant carries guaranteed-valid data — the decision layer receives types
/// that make invalid states unrepresentable ("parse, don't validate").
#[derive(Debug)]
pub enum ToolUse {
    /// Bash command with successfully parsed program segments.
    Bash(BashToolUse),
    /// File tool (Read/Write/Edit/Glob/Grep) with resolved paths and operation.
    File(FileToolUse),
    /// Unrecognized tool — hook has no opinion.
    Unknown { tool_name: String },
}

/// Error from parsing a known tool's input at the protocol boundary.
///
/// Carries the tool category (for config-gated fail-closed behavior) and
/// a human-readable reason (for the hook output message).
#[derive(Debug)]
pub struct ToolParseError {
    pub category: ToolCategory,
    pub reason: String,
}

/// A valid, parsed Bash tool invocation.
///
/// Invariant: `segments` is non-empty and contains successfully parsed programs.
#[derive(Debug)]
pub struct BashToolUse {
    pub raw: String,
    pub segments: Vec<CommandSegment>,
}

/// A valid, resolved file tool invocation.
///
/// Invariant: `paths` is non-empty and all paths are normalized.
/// The `operation` identifies which file tool was invoked (Read, Write, Edit, Glob, Grep).
#[derive(Debug)]
pub struct FileToolUse {
    pub operation: FileOperation,
    pub tool_name: String,
    pub paths: Vec<ResolvedPath>,
}

impl ToolUse {
    /// Parse and validate from raw hook input.
    ///
    /// Returns `Ok` with a valid `ToolUse` variant, or `Err` with a
    /// `ToolParseError` when a known tool has invalid input.
    /// Unknown tools always succeed as `Ok(Unknown)`.
    ///
    /// Called internally during `HookInput` deserialization.
    pub(super) fn parse(ctx: &RawHookInput) -> Result<Self, ToolParseError> {
        match ctx.tool_name.as_str() {
            "Bash" => parse_bash(&ctx.tool_input),
            "Read" | "Write" | "Edit" => {
                let operation = file_operation(&ctx.tool_name);
                parse_required_path(extract_string(&ctx.tool_input, "file_path"), ctx).map(
                    |paths| {
                        ToolUse::File(FileToolUse {
                            operation,
                            tool_name: ctx.tool_name.clone(),
                            paths,
                        })
                    },
                )
            }
            "Glob" | "Grep" => {
                let operation = file_operation(&ctx.tool_name);
                parse_optional_path(extract_string(&ctx.tool_input, "path"), ctx).map(|paths| {
                    ToolUse::File(FileToolUse {
                        operation,
                        tool_name: ctx.tool_name.clone(),
                        paths,
                    })
                })
            }
            _ => Ok(ToolUse::Unknown {
                tool_name: ctx.tool_name.clone(),
            }),
        }
    }

    /// Returns the tool name string for this variant.
    pub fn tool_name(&self) -> &str {
        match self {
            ToolUse::Bash(_) => "Bash",
            ToolUse::File(f) => &f.tool_name,
            ToolUse::Unknown { tool_name } => tool_name,
        }
    }

    /// Returns `true` if this is a file tool variant.
    pub fn is_file_tool(&self) -> bool {
        matches!(self, ToolUse::File(_))
    }
}

/// Map tool name to file operation.
fn file_operation(tool_name: &str) -> FileOperation {
    match tool_name {
        "Read" => FileOperation::Read,
        "Write" => FileOperation::Write,
        "Edit" => FileOperation::Edit,
        "Glob" => FileOperation::Glob,
        "Grep" => FileOperation::Grep,
        _ => unreachable!("file_operation called with non-file tool: {tool_name}"),
    }
}

fn bash_err(reason: impl Into<String>) -> Result<ToolUse, ToolParseError> {
    Err(ToolParseError {
        category: ToolCategory::Bash,
        reason: reason.into(),
    })
}

fn file_err(reason: impl Into<String>) -> ToolParseError {
    ToolParseError {
        category: ToolCategory::File,
        reason: reason.into(),
    }
}

/// Parse and validate the Bash command.
fn parse_bash(tool_input: &Value) -> Result<ToolUse, ToolParseError> {
    let command = match tool_input.get("command").and_then(|v| v.as_str()) {
        Some(cmd) => cmd,
        None => return bash_err("Bash tool without command field"),
    };

    if command.trim().is_empty() {
        return bash_err("Empty bash command");
    }

    let segments = match command::parse(command) {
        Ok(segs) => segs,
        Err(e) => return bash_err(format!("Failed to parse command: {e}")),
    };

    if segments.is_empty() {
        return bash_err("No programs extracted from command");
    }

    Ok(ToolUse::Bash(BashToolUse {
        raw: command.to_string(),
        segments,
    }))
}

/// Parse a file tool where the path is required (Read/Write/Edit).
fn parse_required_path(
    raw_path: Option<String>,
    ctx: &RawHookInput,
) -> Result<Vec<ResolvedPath>, ToolParseError> {
    let raw_path = match raw_path {
        Some(p) => p,
        None => {
            return Err(file_err(format!(
                "claude-permissions-hook: no file path provided for {} tool",
                ctx.tool_name,
            )))
        }
    };
    resolve_paths(vec![raw_path], &ctx.cwd, ctx)
}

/// Parse a file tool where the path defaults to cwd (Glob/Grep).
fn parse_optional_path(
    raw_path: Option<String>,
    ctx: &RawHookInput,
) -> Result<Vec<ResolvedPath>, ToolParseError> {
    let p = raw_path.unwrap_or_else(|| ctx.cwd.clone());
    resolve_paths(vec![p], &ctx.cwd, ctx)
}

/// Normalize raw paths into resolved paths.
fn resolve_paths(
    raw_paths: Vec<String>,
    cwd: &str,
    ctx: &RawHookInput,
) -> Result<Vec<ResolvedPath>, ToolParseError> {
    let mut resolved = Vec::with_capacity(raw_paths.len());
    for rp in raw_paths {
        match path::normalize(&rp, cwd) {
            Ok(normalized) => resolved.push(ResolvedPath {
                raw: rp,
                normalized,
            }),
            Err(_) => {
                return Err(file_err(format!(
                    "claude-permissions-hook: failed to normalize path for {} tool",
                    ctx.tool_name,
                )))
            }
        }
    }
    Ok(resolved)
}

/// Extract a non-empty string field from JSON.
fn extract_string(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::input::WirePermissionMode;
    use serde_json::json;

    const CWD: &str = "/home/user/project";

    fn raw(tool_name: &str, tool_input: serde_json::Value) -> RawHookInput {
        RawHookInput {
            session_id: "s".to_string(),
            transcript_path: "/tmp/t.json".to_string(),
            cwd: CWD.to_string(),
            permission_mode: WirePermissionMode::Default,
            hook_event_name: "PreToolUse".to_string(),
            tool_name: tool_name.to_string(),
            tool_input,
            tool_use_id: "u".to_string(),
        }
    }

    /// Shorthand: parse from raw helper.
    fn parse(name: &str, tool_input: serde_json::Value) -> Result<ToolUse, ToolParseError> {
        ToolUse::parse(&raw(name, tool_input))
    }

    /// Helper to extract paths from a file tool variant.
    fn file_paths(tool_use: &ToolUse) -> &[ResolvedPath] {
        match tool_use {
            ToolUse::File(f) => &f.paths,
            other => panic!("expected file tool, got {other:?}"),
        }
    }

    /// Helper to extract operation from a file tool variant.
    fn file_op(tool_use: &ToolUse) -> FileOperation {
        match tool_use {
            ToolUse::File(f) => f.operation,
            other => panic!("expected file tool, got {other:?}"),
        }
    }

    // ---- Bash: valid → Ok(Bash) ----

    #[test]
    fn bash_valid_command_parsed() {
        let tool_use = parse("Bash", json!({"command": "ls -la"})).unwrap();
        match &tool_use {
            ToolUse::Bash(bash) => {
                assert_eq!(bash.raw, "ls -la");
                assert_eq!(bash.segments.len(), 1);
                assert_eq!(bash.segments[0].program.as_str(), "ls");
            }
            other => panic!("expected Bash, got {other:?}"),
        }
    }

    // ---- Bash: invalid → Err ----

    #[test]
    fn bash_empty_command_err() {
        let result = parse("Bash", json!({"command": ""}));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().category, ToolCategory::Bash);
    }

    #[test]
    fn bash_missing_command_err() {
        assert!(parse("Bash", json!({"description": "something"})).is_err());
    }

    #[test]
    fn bash_whitespace_command_err() {
        assert!(parse("Bash", json!({"command": "   "})).is_err());
    }

    #[test]
    fn bash_parse_error_err() {
        assert!(parse("Bash", json!({"command": "git add . &&"})).is_err());
    }

    #[test]
    fn bash_no_programs_err() {
        assert!(parse("Bash", json!({"command": "(( x + 1 ))"})).is_err());
    }

    // ---- File tools: valid → File variant with correct operation ----

    #[test]
    fn read_resolves_path() {
        let tool_use = parse("Read", json!({"file_path": "/foo/bar.rs"})).unwrap();
        assert!(matches!(&tool_use, ToolUse::File(_)));
        assert_eq!(tool_use.tool_name(), "Read");
        assert_eq!(file_op(&tool_use), FileOperation::Read);
        let paths = file_paths(&tool_use);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].raw, "/foo/bar.rs");
        assert_eq!(paths[0].normalized, "/foo/bar.rs");
    }

    #[test]
    fn write_resolves_path() {
        let tool_use = parse("Write", json!({"file_path": "/foo/new.rs"})).unwrap();
        assert!(matches!(&tool_use, ToolUse::File(_)));
        assert_eq!(file_op(&tool_use), FileOperation::Write);
        assert_eq!(file_paths(&tool_use)[0].raw, "/foo/new.rs");
    }

    #[test]
    fn edit_resolves_path() {
        let tool_use = parse("Edit", json!({"file_path": "/foo/lib.rs"})).unwrap();
        assert!(matches!(&tool_use, ToolUse::File(_)));
        assert_eq!(file_op(&tool_use), FileOperation::Edit);
        assert_eq!(file_paths(&tool_use)[0].raw, "/foo/lib.rs");
    }

    #[test]
    fn glob_extracts_explicit_path() {
        let tool_use = parse("Glob", json!({"pattern": "**/*.rs", "path": "/src"})).unwrap();
        assert!(matches!(&tool_use, ToolUse::File(_)));
        assert_eq!(file_op(&tool_use), FileOperation::Glob);
        assert_eq!(file_paths(&tool_use)[0].raw, "/src");
    }

    #[test]
    fn glob_no_path_uses_cwd() {
        let tool_use = parse("Glob", json!({"pattern": "**/*.rs"})).unwrap();
        assert!(matches!(&tool_use, ToolUse::File(_)));
        assert_eq!(file_paths(&tool_use)[0].raw, CWD);
    }

    #[test]
    fn grep_extracts_explicit_path() {
        let tool_use = parse("Grep", json!({"pattern": "TODO", "path": "/src"})).unwrap();
        assert!(matches!(&tool_use, ToolUse::File(_)));
        assert_eq!(file_op(&tool_use), FileOperation::Grep);
        assert_eq!(file_paths(&tool_use)[0].raw, "/src");
    }

    #[test]
    fn grep_no_path_uses_cwd() {
        let tool_use = parse("Grep", json!({"pattern": "TODO"})).unwrap();
        assert!(matches!(&tool_use, ToolUse::File(_)));
        assert_eq!(file_paths(&tool_use)[0].raw, CWD);
    }

    // ---- File tools: invalid → Err ----

    #[test]
    fn read_missing_file_path_err() {
        let result = parse("Read", json!({}));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().category, ToolCategory::File);
    }

    #[test]
    fn read_wrong_type_file_path_err() {
        assert!(parse("Read", json!({"file_path": 42})).is_err());
    }

    #[test]
    fn read_empty_string_file_path_err() {
        assert!(parse("Read", json!({"file_path": ""})).is_err());
    }

    // ---- Glob/Grep empty path defaults to cwd (not error) ----

    #[test]
    fn glob_empty_path_defaults_to_cwd() {
        let tool_use = parse("Glob", json!({"pattern": "**/*.rs", "path": ""})).unwrap();
        assert!(matches!(&tool_use, ToolUse::File(_)));
        assert_eq!(file_paths(&tool_use)[0].raw, CWD);
    }

    #[test]
    fn grep_empty_path_defaults_to_cwd() {
        let tool_use = parse("Grep", json!({"pattern": "TODO", "path": ""})).unwrap();
        assert!(matches!(&tool_use, ToolUse::File(_)));
        assert_eq!(file_paths(&tool_use)[0].raw, CWD);
    }

    // ---- Unknown tools → Ok(Unknown) ----

    #[test]
    fn unknown_tool() {
        let tool_use = parse("NotebookEdit", json!({})).unwrap();
        assert!(matches!(
            tool_use,
            ToolUse::Unknown { tool_name } if tool_name == "NotebookEdit"
        ));
    }

    #[test]
    fn mcp_tool_is_unknown() {
        let tool_use = parse("mcp__test__run", json!({"server": "test"})).unwrap();
        assert!(matches!(
            tool_use,
            ToolUse::Unknown { tool_name } if tool_name == "mcp__test__run"
        ));
    }

    // ---- is_file_tool ----

    #[test]
    fn bash_is_not_file_tool() {
        let tool_use = parse("Bash", json!({"command": "ls"})).unwrap();
        assert!(!tool_use.is_file_tool());
    }

    #[test]
    fn unknown_is_not_file_tool() {
        let tool_use = parse("NotebookEdit", json!({})).unwrap();
        assert!(!tool_use.is_file_tool());
    }

    #[test]
    fn all_file_tools_are_file_tools() {
        for (name, input) in [
            ("Read", json!({"file_path": "/tmp/f"})),
            ("Write", json!({"file_path": "/tmp/f"})),
            ("Edit", json!({"file_path": "/tmp/f"})),
            ("Glob", json!({"pattern": "*"})),
            ("Grep", json!({"pattern": "x"})),
        ] {
            let tool_use = parse(name, input).unwrap();
            assert!(tool_use.is_file_tool(), "{name} should be a file tool");
        }
    }

    // ---- Relative path normalization ----

    #[test]
    fn relative_path_normalized_to_cwd() {
        let tool_use = parse("Read", json!({"file_path": "src/main.rs"})).unwrap();
        let paths = file_paths(&tool_use);
        assert_eq!(paths[0].raw, "src/main.rs");
        assert_eq!(paths[0].normalized, "/home/user/project/src/main.rs");
    }
}
