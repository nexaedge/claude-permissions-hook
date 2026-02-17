use std::collections::HashSet;

use globset::{Glob, GlobMatcher};

/// A parsed rule for a bash program with optional conditions.
///
/// Created from KDL config entries like `deny "rm -rf /"` or children blocks.
/// Matching logic is in Step 03 — this module defines types and construction only.
#[derive(Debug)]
pub struct BashRule {
    pub program: String,
    pub conditions: RuleConditions,
}

/// Conditions that must be met for a rule to match a command.
///
/// An empty `RuleConditions` (all fields empty/default) means the rule matches
/// any invocation of the program — backwards compatible with v0.2.0.
#[derive(Debug, Default)]
pub struct RuleConditions {
    /// Flags that must ALL be present (AND semantics).
    pub required_flags: HashSet<String>,
    /// Flags where ANY one triggers the rule (OR semantics).
    pub optional_flags: HashSet<String>,
    /// Ordered prefix from rule string non-flag args (e.g., `["push"]` from `git push --force`).
    pub subcommand: Vec<String>,
    /// Glob patterns for positional arguments (any order).
    pub positionals: Vec<PositionalPattern>,
    /// Flag+value pairs that must be present (e.g., `--upload-file *.txt`).
    pub required_arguments: Vec<ArgumentPattern>,
    /// OR list of ordered subcommand chains from children blocks.
    pub subcommands: Vec<Vec<String>>,
}

/// A glob pattern for matching positional arguments.
#[derive(Debug)]
pub struct PositionalPattern {
    /// Original pattern string for display/debugging.
    pub raw: String,
    /// Compiled glob matcher.
    pub matcher: GlobMatcher,
}

/// A flag+value pattern for matching arguments like `--upload-file *.txt`.
#[derive(Debug)]
pub struct ArgumentPattern {
    /// The flag (e.g., `"--upload-file"`).
    pub flag: String,
    /// Glob pattern for the value.
    pub value: PositionalPattern,
}

impl BashRule {
    /// Returns true when conditions are all empty — backwards-compatible unconditional match.
    pub fn is_unconditional(&self) -> bool {
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
/// Returns `Err` with a message if the pattern is invalid.
pub fn compile_glob(raw: &str) -> Result<PositionalPattern, String> {
    let glob = Glob::new(raw).map_err(|e| format!("invalid glob pattern '{raw}': {e}"))?;
    Ok(PositionalPattern {
        raw: raw.to_string(),
        matcher: glob.compile_matcher(),
    })
}

/// Normalize a bare flag string from children blocks.
///
/// - Single character `"r"` → `"-r"`
/// - Multi-character `"force"` → `"--force"`
/// - Already has dash prefix `"-r"` or `"--force"` → keep as-is
pub fn normalize_flag(s: &str) -> String {
    if s.starts_with('-') {
        s.to_string()
    } else if s.len() == 1 {
        format!("-{s}")
    } else {
        format!("--{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconditional_rule_with_empty_conditions() {
        let rule = BashRule {
            program: "rm".to_string(),
            conditions: RuleConditions::default(),
        };
        assert!(rule.is_unconditional());
    }

    #[test]
    fn conditional_rule_with_flags() {
        let mut conditions = RuleConditions::default();
        conditions.required_flags.insert("-r".to_string());
        let rule = BashRule {
            program: "rm".to_string(),
            conditions,
        };
        assert!(!rule.is_unconditional());
    }

    #[test]
    fn compile_valid_glob() {
        let pattern = compile_glob("/*").unwrap();
        assert_eq!(pattern.raw, "/*");
        assert!(pattern.matcher.is_match("/tmp"));
    }

    #[test]
    fn compile_invalid_glob_returns_error() {
        let result = compile_glob("[invalid");
        assert!(result.is_err());
    }

    #[test]
    fn normalize_single_char_flag() {
        assert_eq!(normalize_flag("r"), "-r");
        assert_eq!(normalize_flag("f"), "-f");
    }

    #[test]
    fn normalize_multi_char_flag() {
        assert_eq!(normalize_flag("force"), "--force");
        assert_eq!(normalize_flag("verbose"), "--verbose");
    }

    #[test]
    fn normalize_already_dashed_flag() {
        assert_eq!(normalize_flag("-r"), "-r");
        assert_eq!(normalize_flag("--force"), "--force");
    }
}
