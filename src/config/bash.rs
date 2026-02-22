//! Bash tool configuration.
//!
//! Type alias and lookup logic. Parsing is in [`crate::config::parse::bash`].

use super::rule;
use crate::command::CommandSegment;
use crate::protocol::Decision;

/// Bash-specific configuration: a flat ordered list of rules.
///
/// Rules carry their own `decision` field (allow/deny/ask). Lookup applies
/// severity ordering: deny > ask > allow.
pub type BashConfig = Vec<rule::BashRule>;

/// Look up a command segment against bash rules.
///
/// Uses `BashRule::matches()` for full condition evaluation (program name,
/// flags, subcommands, positionals, required arguments).
/// Severity ordering: deny > ask > allow. Returns `None` for unlisted programs.
pub(crate) fn lookup(rules: &[rule::BashRule], segment: &CommandSegment) -> Option<Decision> {
    rules
        .iter()
        .filter(|r| r.matches(segment))
        .map(|r| r.decision.clone())
        .max_by_key(|d| d.severity())
}
