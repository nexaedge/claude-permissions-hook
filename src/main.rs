use clap::Parser;
use claude_permissions_hook::cli::Commands;

/// Permission hook for Claude Code with granular rule-based control.
#[derive(Debug, Parser)]
#[command(name = "claude-permissions-hook", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Hook { config } => claude_permissions_hook::cli::hook::run(config.as_deref()),
    }
}
