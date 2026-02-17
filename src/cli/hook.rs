use std::io::Read;
use std::path::Path;

use crate::config::Config;
use crate::decision;
use crate::protocol::HookOutput;

/// Execute the hook subcommand: read JSON from stdin, evaluate, write JSON to stdout.
///
/// Loads config from the optional `--config` path. Without config, all tools
/// receive an "ask" decision prompting the user to configure the hook.
///
/// All runtime errors (bad stdin, config errors, parse failures) produce valid
/// JSON on stdout. Panics only on invariant violations (e.g., broken Serialize
/// derive), which indicate programming bugs rather than runtime conditions.
pub fn run(config_path: Option<&Path>) {
    let config = config_path.map(Config::load);

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
