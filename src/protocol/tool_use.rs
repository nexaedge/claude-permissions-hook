use serde_json::Value;

/// Identifies which file tool operation is being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileOperation {
    Read,
    Write,
    Edit,
    Glob,
    Grep,
}

/// Typed representation of a tool invocation, parsed at the protocol boundary.
///
/// Replaces stringly-typed `tool_name` matching. Each variant carries
/// the tool-specific fields extracted from `tool_input`.
pub enum ToolUse {
    /// Bash command execution.
    Bash {
        /// Raw command string from tool_input["command"].
        /// `None` when the field is missing or not a string.
        command: Option<String>,
    },
    /// File read operation.
    Read { file_path: Option<String> },
    /// File write operation.
    Write { file_path: Option<String> },
    /// File edit operation.
    Edit { file_path: Option<String> },
    /// Glob pattern search.
    Glob {
        /// Search directory (None = use cwd).
        path: Option<String>,
    },
    /// Grep content search.
    Grep {
        /// Search directory (None = use cwd).
        path: Option<String>,
    },
    /// Unrecognized tool — hook has no opinion.
    Unknown { tool_name: String },
}

impl ToolUse {
    /// Parse from raw hook input fields.
    ///
    /// Extracts tool-specific fields from `tool_input` based on `tool_name`.
    /// This is the single point where JSON field knowledge lives.
    ///
    /// # Examples
    ///
    /// ```
    /// use claude_permissions_hook::protocol::ToolUse;
    ///
    /// // Bash tool
    /// let tool_use = ToolUse::parse("Bash", &serde_json::json!({"command": "git status"}));
    /// match tool_use {
    ///     ToolUse::Bash { command } => assert_eq!(command.as_deref(), Some("git status")),
    ///     _ => unreachable!(),
    /// }
    ///
    /// // File read tool
    /// let tool_use = ToolUse::parse("Read", &serde_json::json!({"file_path": "/tmp/foo.rs"}));
    /// match tool_use {
    ///     ToolUse::Read { file_path } => assert_eq!(file_path.as_deref(), Some("/tmp/foo.rs")),
    ///     _ => unreachable!(),
    /// }
    /// ```
    pub fn parse(tool_name: &str, tool_input: &Value) -> Self {
        match tool_name {
            "Bash" => {
                let command = tool_input
                    .get("command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                ToolUse::Bash { command }
            }
            "Read" => ToolUse::Read {
                file_path: extract_string(tool_input, "file_path"),
            },
            "Write" => ToolUse::Write {
                file_path: extract_string(tool_input, "file_path"),
            },
            "Edit" => ToolUse::Edit {
                file_path: extract_string(tool_input, "file_path"),
            },
            "Glob" => ToolUse::Glob {
                path: extract_string(tool_input, "path"),
            },
            "Grep" => ToolUse::Grep {
                path: extract_string(tool_input, "path"),
            },
            _ => ToolUse::Unknown {
                tool_name: tool_name.to_string(),
            },
        }
    }

    /// Derive the file operation from this variant, if applicable.
    pub fn file_operation(&self) -> Option<FileOperation> {
        match self {
            ToolUse::Read { .. } => Some(FileOperation::Read),
            ToolUse::Write { .. } => Some(FileOperation::Write),
            ToolUse::Edit { .. } => Some(FileOperation::Edit),
            ToolUse::Glob { .. } => Some(FileOperation::Glob),
            ToolUse::Grep { .. } => Some(FileOperation::Grep),
            _ => None,
        }
    }

    /// Extract file paths from a file tool variant.
    ///
    /// - Read/Write/Edit: returns the file_path if present, or empty vec (fail-closed).
    /// - Glob/Grep: returns the path if present, or cwd as default.
    /// - Non-file tools: returns None.
    pub fn file_paths(&self, cwd: &str) -> Option<Vec<String>> {
        match self {
            ToolUse::Read { file_path }
            | ToolUse::Write { file_path }
            | ToolUse::Edit { file_path } => {
                let paths = match file_path {
                    Some(p) => vec![p.clone()],
                    None => vec![],
                };
                Some(paths)
            }
            ToolUse::Glob { path } | ToolUse::Grep { path } => {
                let p = path.as_deref().unwrap_or(cwd);
                Some(vec![p.to_string()])
            }
            _ => None,
        }
    }
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
    use serde_json::json;

    const CWD: &str = "/home/user/project";

    // ---- Bash parsing ----

    #[test]
    fn bash_extracts_command() {
        let tool_use = ToolUse::parse("Bash", &json!({"command": "ls -la"}));
        match tool_use {
            ToolUse::Bash { command } => assert_eq!(command.as_deref(), Some("ls -la")),
            _ => panic!("expected Bash variant"),
        }
    }

    #[test]
    fn bash_empty_command() {
        let tool_use = ToolUse::parse("Bash", &json!({"command": ""}));
        match tool_use {
            ToolUse::Bash { command } => assert_eq!(command.as_deref(), Some("")),
            _ => panic!("expected Bash variant"),
        }
    }

    #[test]
    fn bash_missing_command_yields_none() {
        let tool_use = ToolUse::parse("Bash", &json!({"description": "something"}));
        match tool_use {
            ToolUse::Bash { command } => assert!(command.is_none()),
            _ => panic!("expected Bash variant"),
        }
    }

    // ---- File tool parsing ----

    #[test]
    fn read_extracts_file_path() {
        let tool_use = ToolUse::parse("Read", &json!({"file_path": "/foo/bar.rs"}));
        match &tool_use {
            ToolUse::Read { file_path } => assert_eq!(file_path.as_deref(), Some("/foo/bar.rs")),
            _ => panic!("expected Read variant"),
        }
        assert_eq!(tool_use.file_operation(), Some(FileOperation::Read));
    }

    #[test]
    fn write_extracts_file_path() {
        let tool_use = ToolUse::parse("Write", &json!({"file_path": "/foo/new.rs"}));
        match &tool_use {
            ToolUse::Write { file_path } => assert_eq!(file_path.as_deref(), Some("/foo/new.rs")),
            _ => panic!("expected Write variant"),
        }
        assert_eq!(tool_use.file_operation(), Some(FileOperation::Write));
    }

    #[test]
    fn edit_extracts_file_path() {
        let tool_use = ToolUse::parse("Edit", &json!({"file_path": "/foo/lib.rs"}));
        match &tool_use {
            ToolUse::Edit { file_path } => assert_eq!(file_path.as_deref(), Some("/foo/lib.rs")),
            _ => panic!("expected Edit variant"),
        }
        assert_eq!(tool_use.file_operation(), Some(FileOperation::Edit));
    }

    #[test]
    fn glob_extracts_explicit_path() {
        let tool_use = ToolUse::parse("Glob", &json!({"pattern": "**/*.rs", "path": "/src"}));
        match &tool_use {
            ToolUse::Glob { path } => assert_eq!(path.as_deref(), Some("/src")),
            _ => panic!("expected Glob variant"),
        }
        assert_eq!(tool_use.file_operation(), Some(FileOperation::Glob));
    }

    #[test]
    fn glob_no_path_uses_cwd() {
        let tool_use = ToolUse::parse("Glob", &json!({"pattern": "**/*.rs"}));
        let paths = tool_use.file_paths(CWD).unwrap();
        assert_eq!(paths, vec![CWD.to_string()]);
    }

    #[test]
    fn grep_extracts_explicit_path() {
        let tool_use = ToolUse::parse("Grep", &json!({"pattern": "TODO", "path": "/src"}));
        match &tool_use {
            ToolUse::Grep { path } => assert_eq!(path.as_deref(), Some("/src")),
            _ => panic!("expected Grep variant"),
        }
        assert_eq!(tool_use.file_operation(), Some(FileOperation::Grep));
    }

    #[test]
    fn grep_no_path_uses_cwd() {
        let tool_use = ToolUse::parse("Grep", &json!({"pattern": "TODO"}));
        let paths = tool_use.file_paths(CWD).unwrap();
        assert_eq!(paths, vec![CWD.to_string()]);
    }

    // ---- Unknown tools ----

    #[test]
    fn unknown_tool() {
        let tool_use = ToolUse::parse("NotebookEdit", &json!({}));
        assert_eq!(tool_use.file_operation(), None);
        match tool_use {
            ToolUse::Unknown { tool_name } => assert_eq!(tool_name, "NotebookEdit"),
            _ => panic!("expected Unknown variant"),
        }
    }

    #[test]
    fn mcp_tool_is_unknown() {
        let tool_use = ToolUse::parse("mcp__test__run", &json!({"server": "test"}));
        match tool_use {
            ToolUse::Unknown { tool_name } => assert_eq!(tool_name, "mcp__test__run"),
            _ => panic!("expected Unknown variant"),
        }
    }

    // ---- Missing/invalid field handling (fail-closed) ----

    #[test]
    fn read_missing_file_path_returns_empty_paths() {
        let tool_use = ToolUse::parse("Read", &json!({}));
        let paths = tool_use.file_paths(CWD).unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn read_wrong_type_file_path_returns_empty_paths() {
        let tool_use = ToolUse::parse("Read", &json!({"file_path": 42}));
        let paths = tool_use.file_paths(CWD).unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn read_empty_string_file_path_returns_empty_paths() {
        let tool_use = ToolUse::parse("Read", &json!({"file_path": ""}));
        let paths = tool_use.file_paths(CWD).unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn glob_empty_path_defaults_to_cwd() {
        let tool_use = ToolUse::parse("Glob", &json!({"pattern": "**/*.rs", "path": ""}));
        let paths = tool_use.file_paths(CWD).unwrap();
        assert_eq!(paths, vec![CWD.to_string()]);
    }

    #[test]
    fn grep_empty_path_defaults_to_cwd() {
        let tool_use = ToolUse::parse("Grep", &json!({"pattern": "TODO", "path": ""}));
        let paths = tool_use.file_paths(CWD).unwrap();
        assert_eq!(paths, vec![CWD.to_string()]);
    }

    // ---- file_paths for non-file tools ----

    #[test]
    fn bash_file_operation_returns_none() {
        let tool_use = ToolUse::parse("Bash", &json!({"command": "ls"}));
        assert!(tool_use.file_operation().is_none());
        assert!(tool_use.file_paths(CWD).is_none());
    }

    #[test]
    fn unknown_file_paths_returns_none() {
        let tool_use = ToolUse::parse("NotebookEdit", &json!({}));
        assert!(tool_use.file_paths(CWD).is_none());
    }
}
