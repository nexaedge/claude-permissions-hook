//! Cross-module error types.
//!
//! Per ARCHITECTURE.md §Error Types, all errors shared across module boundaries
//! live here. Domain-internal invariant errors (e.g. `EmptyProgramName`, `PathError`)
//! stay in their respective domain modules.

use crate::domain::PolicySet;

/// Error from parsing a known tool's input at the protocol boundary.
///
/// Carries the policy set (for config-gated fail-closed behavior) and
/// a human-readable reason (for the hook output message).
#[derive(Debug, thiserror::Error)]
pub enum ToolParseError {
    /// Unrecognized tool name — CLI returns no opinion.
    #[error("unknown tool: {tool_name}")]
    UnknownTool { tool_name: String },
    /// Known policy set but malformed input — CLI returns Ask or no opinion.
    #[error("{reason}")]
    InvalidInput {
        policy_set: PolicySet,
        reason: String,
    },
}

/// Error from parsing hook JSON input into a valid request.
#[derive(Debug, thiserror::Error)]
pub enum HookParseError {
    /// JSON deserialization failed.
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    /// Required field missing from hook input.
    #[error("missing field: {0}")]
    MissingField(String),
    /// Tool input could not produce a valid ToolRequest.
    #[error(transparent)]
    ToolError(#[from] ToolParseError),
}

/// Error from parsing KDL config content.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// KDL syntax or semantic error in config content.
    #[error("invalid KDL syntax: {0}")]
    InvalidSyntax(String),
}
