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

/// Extracts file paths from a file tool's `tool_input` JSON.
///
/// Returns `None` if `tool_name` is not a file tool (Bash, MCP, etc.).
/// Returns `Some((paths, operation))` where:
/// - **Read/Write/Edit:** `paths` contains the `file_path` field value, or is
///   empty if the field is missing or not a string (fail-closed).
/// - **Glob/Grep:** `paths` contains the `path` field value, defaulting to
///   `cwd` when the field is absent or not a string.
pub fn extract_file_paths(
    tool_name: &str,
    tool_input: &Value,
    cwd: &str,
) -> Option<(Vec<String>, FileOperation)> {
    let op = match tool_name {
        "Read" => FileOperation::Read,
        "Write" => FileOperation::Write,
        "Edit" => FileOperation::Edit,
        "Glob" => FileOperation::Glob,
        "Grep" => FileOperation::Grep,
        _ => return None,
    };

    let paths = match op {
        FileOperation::Read | FileOperation::Write | FileOperation::Edit => {
            match tool_input
                .get("file_path")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                Some(p) => vec![p.to_string()],
                None => vec![],
            }
        }
        FileOperation::Glob | FileOperation::Grep => {
            let path = tool_input
                .get("path")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or(cwd);
            vec![path.to_string()]
        }
    };

    Some((paths, op))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CWD: &str = "/home/user/project";

    // ---- Read tool ----

    #[test]
    fn read_extracts_file_path() {
        let input = json!({"file_path": "/foo/bar.rs"});
        let result = extract_file_paths("Read", &input, CWD);
        assert_eq!(
            result,
            Some((vec!["/foo/bar.rs".to_string()], FileOperation::Read))
        );
    }

    // ---- Write tool ----

    #[test]
    fn write_extracts_file_path() {
        let input = json!({"file_path": "/foo/new.rs"});
        let result = extract_file_paths("Write", &input, CWD);
        assert_eq!(
            result,
            Some((vec!["/foo/new.rs".to_string()], FileOperation::Write))
        );
    }

    // ---- Edit tool ----

    #[test]
    fn edit_extracts_file_path() {
        let input = json!({"file_path": "/foo/lib.rs"});
        let result = extract_file_paths("Edit", &input, CWD);
        assert_eq!(
            result,
            Some((vec!["/foo/lib.rs".to_string()], FileOperation::Edit))
        );
    }

    // ---- Glob tool ----

    #[test]
    fn glob_extracts_explicit_path() {
        let input = json!({"pattern": "**/*.rs", "path": "/src"});
        let result = extract_file_paths("Glob", &input, CWD);
        assert_eq!(
            result,
            Some((vec!["/src".to_string()], FileOperation::Glob))
        );
    }

    #[test]
    fn glob_defaults_to_cwd_when_path_absent() {
        let input = json!({"pattern": "**/*.rs"});
        let result = extract_file_paths("Glob", &input, CWD);
        assert_eq!(result, Some((vec![CWD.to_string()], FileOperation::Glob)));
    }

    // ---- Grep tool ----

    #[test]
    fn grep_extracts_explicit_path() {
        let input = json!({"pattern": "TODO", "path": "/src"});
        let result = extract_file_paths("Grep", &input, CWD);
        assert_eq!(
            result,
            Some((vec!["/src".to_string()], FileOperation::Grep))
        );
    }

    #[test]
    fn grep_defaults_to_cwd_when_path_absent() {
        let input = json!({"pattern": "TODO"});
        let result = extract_file_paths("Grep", &input, CWD);
        assert_eq!(result, Some((vec![CWD.to_string()], FileOperation::Grep)));
    }

    // ---- Non-file tools ----

    #[test]
    fn bash_returns_none() {
        let input = json!({"command": "ls -la"});
        assert_eq!(extract_file_paths("Bash", &input, CWD), None);
    }

    #[test]
    fn mcp_returns_none() {
        let input = json!({"server": "test"});
        assert_eq!(extract_file_paths("mcp__test__run", &input, CWD), None);
    }

    #[test]
    fn unknown_tool_returns_none() {
        let input = json!({});
        assert_eq!(extract_file_paths("NotebookEdit", &input, CWD), None);
    }

    // ---- Missing/invalid field handling ----

    #[test]
    fn read_missing_file_path_returns_empty_vec() {
        let input = json!({});
        let result = extract_file_paths("Read", &input, CWD);
        assert_eq!(result, Some((vec![], FileOperation::Read)));
    }

    #[test]
    fn read_wrong_type_file_path_returns_empty_vec() {
        let input = json!({"file_path": 42});
        let result = extract_file_paths("Read", &input, CWD);
        assert_eq!(result, Some((vec![], FileOperation::Read)));
    }

    #[test]
    fn write_missing_file_path_returns_empty_vec() {
        let input = json!({});
        let result = extract_file_paths("Write", &input, CWD);
        assert_eq!(result, Some((vec![], FileOperation::Write)));
    }

    #[test]
    fn edit_missing_file_path_returns_empty_vec() {
        let input = json!({});
        let result = extract_file_paths("Edit", &input, CWD);
        assert_eq!(result, Some((vec![], FileOperation::Edit)));
    }

    // ---- Empty-string edge cases (fail-closed: treat as missing) ----

    #[test]
    fn read_empty_string_file_path_returns_empty_vec() {
        let input = json!({"file_path": ""});
        let result = extract_file_paths("Read", &input, CWD);
        assert_eq!(result, Some((vec![], FileOperation::Read)));
    }

    #[test]
    fn write_empty_string_file_path_returns_empty_vec() {
        let input = json!({"file_path": ""});
        let result = extract_file_paths("Write", &input, CWD);
        assert_eq!(result, Some((vec![], FileOperation::Write)));
    }

    #[test]
    fn edit_empty_string_file_path_returns_empty_vec() {
        let input = json!({"file_path": ""});
        let result = extract_file_paths("Edit", &input, CWD);
        assert_eq!(result, Some((vec![], FileOperation::Edit)));
    }

    #[test]
    fn glob_empty_string_path_defaults_to_cwd() {
        let input = json!({"pattern": "**/*.rs", "path": ""});
        let result = extract_file_paths("Glob", &input, CWD);
        assert_eq!(result, Some((vec![CWD.to_string()], FileOperation::Glob)));
    }

    #[test]
    fn grep_empty_string_path_defaults_to_cwd() {
        let input = json!({"pattern": "TODO", "path": ""});
        let result = extract_file_paths("Grep", &input, CWD);
        assert_eq!(result, Some((vec![CWD.to_string()], FileOperation::Grep)));
    }
}
