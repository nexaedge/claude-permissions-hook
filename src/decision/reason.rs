use crate::protocol::output::Decision;
use crate::protocol::FileOperation;

use super::APP_NAME;

/// Build a human-readable reason string for the decision.
///
/// `modified` is the final decision (after mode modifier).
/// `programs` is the full list of program names in the command.
/// `per_program` is the per-program lookup results (before aggregation).
/// `pre_modifier` is the aggregated decision before mode modifier was applied.
pub(crate) fn build_reason(
    modified: &Decision,
    programs: &[&str],
    per_program: &[Option<Decision>],
    pre_modifier: &Decision,
) -> String {
    let is_single = programs.len() == 1;
    match modified {
        Decision::Allow => {
            format!("{APP_NAME}: allowed ({})", programs.join(", "))
        }
        Decision::Deny => {
            let trigger = find_trigger(programs, per_program, pre_modifier);
            let mode_converted = *pre_modifier != Decision::Deny;
            if mode_converted {
                if is_single {
                    format!("{APP_NAME}: '{trigger}' denied by dontAsk mode")
                } else {
                    format!(
                        "{APP_NAME}: '{trigger}' denied by dontAsk mode (in: {})",
                        programs.join(", ")
                    )
                }
            } else if is_single {
                format!("{APP_NAME}: '{trigger}' is in your deny list")
            } else {
                format!(
                    "{APP_NAME}: '{trigger}' is denied (in: {})",
                    programs.join(", ")
                )
            }
        }
        Decision::Ask => {
            let trigger = find_trigger(programs, per_program, pre_modifier);
            if is_single {
                format!("{APP_NAME}: '{trigger}' requires confirmation")
            } else {
                format!(
                    "{APP_NAME}: '{trigger}' requires confirmation (in: {})",
                    programs.join(", ")
                )
            }
        }
    }
}

/// Convert a FileOperation to its lowercase string for reason messages.
pub(crate) fn operation_str(op: FileOperation) -> &'static str {
    match op {
        FileOperation::Read => "read",
        FileOperation::Write => "write",
        FileOperation::Edit => "edit",
        FileOperation::Glob => "glob",
        FileOperation::Grep => "grep",
    }
}

/// Build a human-readable reason string for a file tool decision.
pub(crate) fn build_file_reason(
    modified: &Decision,
    paths: &[String],
    per_path: &[Option<Decision>],
    pre_modifier: &Decision,
    operation: &str,
) -> String {
    match modified {
        Decision::Allow => {
            format!("{APP_NAME}: allowed {operation} ({})", paths.join(", "))
        }
        Decision::Deny => {
            let trigger = find_file_trigger(paths, per_path, pre_modifier);
            let mode_converted = *pre_modifier != Decision::Deny;
            if mode_converted {
                format!("{APP_NAME}: '{trigger}' denied by dontAsk mode ({operation})")
            } else {
                format!("{APP_NAME}: '{trigger}' denied by file rules ({operation})")
            }
        }
        Decision::Ask => {
            let trigger = find_file_trigger(paths, per_path, pre_modifier);
            format!("{APP_NAME}: '{trigger}' requires confirmation ({operation})")
        }
    }
}

/// Find the path that triggered the most restrictive file decision.
fn find_file_trigger<'a>(
    paths: &'a [String],
    per_path: &[Option<Decision>],
    target: &Decision,
) -> &'a str {
    for (path, dec) in paths.iter().zip(per_path.iter()) {
        if dec.as_ref() == Some(target) {
            return path;
        }
    }
    if *target == Decision::Ask {
        for (path, dec) in paths.iter().zip(per_path.iter()) {
            if dec.is_none() {
                return path;
            }
        }
    }
    &paths[0]
}

/// Find the program that triggered the most restrictive decision.
///
/// Searches for an explicit match first (program whose config decision equals the target),
/// then falls back to unlisted programs (which default to Ask during aggregation).
fn find_trigger<'a>(
    programs: &[&'a str],
    per_program: &[Option<Decision>],
    target: &Decision,
) -> &'a str {
    // First: find a program explicitly configured with the target decision
    for (prog, dec) in programs.iter().zip(per_program.iter()) {
        if dec.as_ref() == Some(target) {
            return prog;
        }
    }
    // Second: if target is Ask, find an unlisted program (None defaults to Ask)
    if *target == Decision::Ask {
        for (prog, dec) in programs.iter().zip(per_program.iter()) {
            if dec.is_none() {
                return prog;
            }
        }
    }
    // Fallback (shouldn't happen with valid aggregation)
    programs[0]
}
