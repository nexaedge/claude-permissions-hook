use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Permission hook for Claude Code with granular rule-based control.
#[derive(Debug, Parser)]
#[command(name = "claude-permissions-hook", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run as a Claude Code PreToolUse hook (reads stdin, writes stdout)
    Hook {
        /// Path to the KDL config file
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Hook { config } => claude_permissions_hook::run_hook(config.as_deref()),
    }
}
