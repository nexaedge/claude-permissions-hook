use std::io::Read;
use std::path::{Path, PathBuf};

use crate::config::{BashConfig, Config};
use crate::decision;
use crate::protocol::HookOutput;

/// Discover config path when `--config` is not provided.
///
/// Checks in order:
/// 1. `$CLAUDE_PERMISSIONS_HOOK_CONFIG` environment variable
/// 2. `~/.config/claude-permissions-hook/config.kdl`
///
/// Returns `None` if no config is found (no-config mode: ask for everything).
fn discover_config() -> Option<PathBuf> {
    // 1. Environment variable
    if let Ok(path) = std::env::var("CLAUDE_PERMISSIONS_HOOK_CONFIG") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // 2. XDG-style config directory
    if let Some(home) = home_dir() {
        let xdg = home.join(".config/claude-permissions-hook/config.kdl");
        if xdg.exists() {
            return Some(xdg);
        }
    }

    None
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Execute the hook subcommand: read JSON from stdin, evaluate, write JSON to stdout.
///
/// Loads config from the `--config` path, or auto-discovers it from well-known
/// locations. Without config, all tools receive an "ask" decision prompting
/// the user to configure the hook.
///
/// All runtime errors (bad stdin, config errors, parse failures) produce valid
/// JSON on stdout. Panics only on invariant violations (e.g., broken Serialize
/// derive), which indicate programming bugs rather than runtime conditions.
pub fn run(config_path: Option<&Path>) {
    let discovered = config_path.is_none().then(discover_config).flatten();
    let effective_path = config_path.or(discovered.as_deref());
    let config = effective_path.map(|p| {
        Config::builder()
            .register::<BashConfig>()
            .load(p)
    });

    // Handle config load error: output ask with error message
    let config_ref = match &config {
        Some(Ok(cfg)) => Some(cfg),
        Some(Err(e)) => {
            output_json(&HookOutput::ask(format!("Config error: {e}")));
            return;
        }
        None => None,
    };

    match execute_from_stdin(config_ref) {
        Ok(Some(output)) => output_json(&output),
        Ok(None) => println!("{{}}"),
        Err(e) => output_json(&HookOutput::ask(format!("Error: {e}"))),
    }
}

fn execute_from_stdin(
    config: Option<&Config>,
) -> Result<Option<HookOutput>, Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let hook_input = serde_json::from_str(&input)?;
    Ok(decision::evaluate(&hook_input, config))
}

/// Serialize a HookOutput to JSON and print to stdout.
///
/// # Panics
///
/// Panics if serialization fails, which cannot happen with the derived
/// `Serialize` impl on strings and enums. This is an invariant, not a
/// runtime error — failure here indicates a programming bug.
fn output_json(output: &HookOutput) {
    let json = serde_json::to_string(output).expect("HookOutput serialization cannot fail");
    println!("{json}");
}
