use std::collections::HashSet;

use globset::GlobMatcher;

use crate::domain::{Decision, Flag, ProgramName};

/// A parsed rule for a bash program with optional conditions.
///
/// Created from KDL config entries like `deny "rm -rf /"` or children blocks.
#[derive(Debug)]
pub(crate) struct BashRule {
    pub(crate) decision: Decision,
    pub(crate) program: ProgramName,
    pub(crate) conditions: BashConditions,
}

/// Conditions that must be met for a rule to match a command.
///
/// An empty `BashConditions` (all fields empty/default) means the rule matches
/// any invocation of the program — backwards compatible with v0.2.0.
#[derive(Debug, Default, Clone)]
pub(crate) struct BashConditions {
    /// Flags that must ALL be present (AND semantics).
    pub(crate) required_flags: HashSet<Flag>,
    /// Flags where ANY one triggers the rule (OR semantics).
    pub(crate) optional_flags: HashSet<Flag>,
    /// Ordered prefix from rule string non-flag args (e.g., `["push"]` from `git push --force`).
    pub(crate) subcommand: Vec<String>,
    /// Glob patterns for positional arguments (any order).
    pub(crate) positionals: Vec<PositionalPattern>,
    /// Flag+value pairs that must be present (e.g., `--upload-file *.txt`).
    pub(crate) required_arguments: Vec<ArgumentPattern>,
    /// OR list of ordered subcommand chains from children blocks.
    pub(crate) subcommands: Vec<Vec<String>>,
}

/// A glob pattern for matching positional arguments.
#[derive(Debug, Clone)]
pub(crate) struct PositionalPattern {
    /// Original pattern string for display/debugging.
    #[allow(dead_code)]
    pub(crate) raw: String,
    /// Compiled glob matcher.
    pub(crate) matcher: GlobMatcher,
}

/// A flag+value pattern for matching arguments like `--upload-file *.txt`.
#[derive(Debug, Clone)]
pub(crate) struct ArgumentPattern {
    /// The flag (e.g., `"--upload-file"`).
    pub(crate) flag: String,
    /// Glob pattern for the value.
    pub(crate) value: PositionalPattern,
}

impl BashRule {
    /// Returns true when conditions are all empty — backwards-compatible unconditional match.
    #[cfg(test)]
    pub(crate) fn is_unconditional(&self) -> bool {
        self.conditions.required_flags.is_empty()
            && self.conditions.optional_flags.is_empty()
            && self.conditions.subcommand.is_empty()
            && self.conditions.positionals.is_empty()
            && self.conditions.required_arguments.is_empty()
            && self.conditions.subcommands.is_empty()
    }
}

/// Compile a glob pattern string into a `PositionalPattern`.
///
/// Uses `literal_separator(true)` so `*` does not match `/` — standard
/// filesystem glob semantics where `/*` matches `/tmp` but not `/home/user`.
///
/// Returns `Err` with a message if the pattern is invalid.
pub(crate) fn compile_glob(raw: &str) -> Result<PositionalPattern, String> {
    let glob = globset::GlobBuilder::new(raw)
        .literal_separator(true)
        .build()
        .map_err(|e| format!("invalid glob pattern '{raw}': {e}"))?;
    Ok(PositionalPattern {
        raw: raw.to_string(),
        matcher: glob.compile_matcher(),
    })
}
