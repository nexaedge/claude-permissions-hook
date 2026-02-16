use std::io::Read;
use std::path::Path;

use crate::decision;
use crate::protocol::HookOutput;

/// Execute the hook subcommand: read JSON from stdin, evaluate, write JSON to stdout.
///
/// Any error (I/O, parse) results in an `ask` decision with the error as the reason.
/// This function never panics and always produces valid JSON on stdout.
///
/// The `_config_path` parameter is accepted but not yet wired into decision logic
/// (that happens in Step 03: Decision Matrix).
pub fn run(_config_path: Option<&Path>) {
    let output = match execute_from_stdin() {
        Ok(output) => output,
        Err(e) => HookOutput::ask(format!("Error: {e}")),
    };

    // Serialization of HookOutput (strings + enums) cannot realistically fail,
    // but we handle it to uphold the no-panic contract.
    let json = serde_json::to_string(&output).unwrap_or_else(|_| {
        r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"Internal serialization error"}}"#.to_string()
    });
    println!("{json}");
}

fn execute_from_stdin() -> Result<HookOutput, Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let hook_input = serde_json::from_str(&input)?;
    Ok(decision::evaluate(&hook_input))
}
