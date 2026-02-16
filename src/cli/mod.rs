pub mod hook;

use std::path::PathBuf;

use clap::Subcommand;

/// CLI subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run as a Claude Code PreToolUse hook (reads stdin, writes stdout)
    Hook {
        /// Path to the KDL config file
        #[arg(long)]
        config: Option<PathBuf>,
    },
}
