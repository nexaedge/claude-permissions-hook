use std::path::PathBuf;

use super::{PermissionMode, ToolRequest};

/// A complete permission request carrying the tool invocation and its context.
///
/// Groups everything the decision engine needs to evaluate a single
/// tool use event from Claude Code.
#[derive(Debug)]
pub struct PermissionRequest {
    pub tool: ToolRequest,
    pub cwd: PathBuf,
    pub mode: PermissionMode,
    pub session_id: String,
    pub project_path: PathBuf,
}
