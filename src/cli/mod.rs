pub mod hook;

use clap::Subcommand;

/// CLI subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run as a Claude Code PreToolUse hook (reads stdin, writes stdout)
    Hook,
}
