//! Bash tool configuration.
//!
//! Struct and lookup logic. Parsing is in [`crate::config::parse::bash`].

use super::rule;
use super::section::{ToolConfig, ToolSection};
use super::ConfigError;
use crate::command::CommandSegment;
use crate::protocol::Decision;

/// Bash-specific configuration: rules for allow, deny, or ask decisions.
#[derive(Debug, Default)]
pub struct BashConfig {
    pub allow: Vec<rule::BashRule>,
    pub deny: Vec<rule::BashRule>,
    pub ask: Vec<rule::BashRule>,
}

impl ToolConfig for BashConfig {
    const SECTION: &'static str = "bash";

    fn from_section(section: ToolSection) -> Result<Self, ConfigError> {
        Ok(BashConfig {
            allow: super::parse::bash::parse_rules(section.allow)?,
            deny: super::parse::bash::parse_rules(section.deny)?,
            ask: super::parse::bash::parse_rules(section.ask)?,
        })
    }
}

impl BashConfig {
    /// Look up a command segment and return its configured decision.
    ///
    /// Uses `BashRule::matches()` for full condition evaluation (program name,
    /// flags, subcommands, positionals, required arguments).
    /// Precedence: deny > ask > allow. Returns `None` for unlisted programs.
    pub(crate) fn lookup(&self, segment: &CommandSegment) -> Option<Decision> {
        if self.deny.iter().any(|r| r.matches(segment)) {
            Some(Decision::Deny)
        } else if self.ask.iter().any(|r| r.matches(segment)) {
            Some(Decision::Ask)
        } else if self.allow.iter().any(|r| r.matches(segment)) {
            Some(Decision::Allow)
        } else {
            None
        }
    }
}
