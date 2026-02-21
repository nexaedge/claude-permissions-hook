use std::collections::HashSet;

use crate::command::CommandSegment;
use crate::config::rule::{ArgumentPattern, BashRule};

impl BashRule {
    /// Check whether a parsed command segment satisfies this rule's conditions.
    ///
    /// All non-empty conditions must pass (AND semantics).
    /// Empty conditions = unconditional match (backwards compat with v0.2.0).
    pub(crate) fn matches(&self, segment: &CommandSegment) -> bool {
        self.program_matches(segment)
            && self.flags_match(segment)
            && self.subcommand_matches(segment)
            && self.positionals_match(segment)
            && self.required_arguments_match(segment)
            && self.subcommands_match(segment)
    }

    /// Program name must match (both sides are already basename-normalized).
    fn program_matches(&self, segment: &CommandSegment) -> bool {
        self.program == segment.program
    }

    /// Required flags: ALL must be present. Optional flags: if non-empty, ANY one must be present.
    fn flags_match(&self, segment: &CommandSegment) -> bool {
        let (actual_flags, _) = classify_args(&segment.args);
        if !self
            .conditions
            .required_flags
            .iter()
            .all(|f| actual_flags.contains(f.as_str()))
        {
            return false;
        }
        if !self.conditions.optional_flags.is_empty()
            && !self
                .conditions
                .optional_flags
                .iter()
                .any(|f| actual_flags.contains(f.as_str()))
        {
            return false;
        }
        true
    }

    /// Subcommand chain from rule string: ordered prefix of actual non-flag args.
    fn subcommand_matches(&self, segment: &CommandSegment) -> bool {
        if self.conditions.subcommand.is_empty() {
            return true;
        }
        let (_, actual_positionals) = classify_args(&segment.args);
        if actual_positionals.len() < self.conditions.subcommand.len() {
            return false;
        }
        self.conditions
            .subcommand
            .iter()
            .zip(actual_positionals.iter())
            .all(|(rule_tok, actual_tok)| rule_tok == actual_tok)
    }

    /// Positionals from children blocks: each pattern must match at least one actual non-flag arg.
    fn positionals_match(&self, segment: &CommandSegment) -> bool {
        if self.conditions.positionals.is_empty() {
            return true;
        }
        let (_, actual_positionals) = classify_args(&segment.args);
        self.conditions.positionals.iter().all(|pattern| {
            actual_positionals
                .iter()
                .any(|arg| pattern.matcher.is_match(arg))
        })
    }

    /// Required arguments: each flag+value pair must be found in actual args.
    fn required_arguments_match(&self, segment: &CommandSegment) -> bool {
        self.conditions
            .required_arguments
            .iter()
            .all(|req| find_argument_value(&segment.args, req))
    }

    /// Subcommands from children block: OR list of ordered prefix chains.
    fn subcommands_match(&self, segment: &CommandSegment) -> bool {
        if self.conditions.subcommands.is_empty() {
            return true;
        }
        let (_, actual_positionals) = classify_args(&segment.args);
        self.conditions.subcommands.iter().any(|chain| {
            if actual_positionals.len() < chain.len() {
                return false;
            }
            chain
                .iter()
                .zip(actual_positionals.iter())
                .all(|(rule_tok, actual_tok)| rule_tok == actual_tok)
        })
    }
}

/// Classify command args into flags and positionals.
///
/// Flags start with `-` (not `-` alone or `--`). `--` marks end-of-options:
/// everything after it is positional regardless of dashes. `--` itself is
/// excluded from both sets. `-` (stdin) is a positional.
fn classify_args(args: &[String]) -> (HashSet<&str>, Vec<&str>) {
    let mut flags = HashSet::new();
    let mut positionals = Vec::new();
    let mut end_of_options = false;
    for arg in args {
        if end_of_options {
            positionals.push(arg.as_str());
            continue;
        }
        if arg == "--" {
            end_of_options = true;
            continue;
        }
        if arg.starts_with('-') && arg != "-" {
            flags.insert(arg.as_str());
        } else {
            positionals.push(arg.as_str());
        }
    }
    (flags, positionals)
}

/// Check if a required argument (flag+value) is present in the args.
///
/// Looks for the flag in two forms:
/// 1. Separate: `--flag value` (flag at position i, value at i+1 if not a flag)
/// 2. Equals: `--flag=value` (split on first `=`)
///
/// Honors `--` as end-of-options: tokens after `--` are positional and
/// cannot satisfy flag-value requirements.
fn find_argument_value(args: &[String], req: &ArgumentPattern) -> bool {
    for (i, arg) in args.iter().enumerate() {
        // Stop interpreting flags after --
        if *arg == "--" {
            return false;
        }
        // Form 1: separate args (--flag value)
        if *arg == req.flag {
            if let Some(next) = args.get(i + 1) {
                if (!next.starts_with('-') || next == "-")
                    && req.value.matcher.is_match(next.as_str())
                {
                    return true;
                }
            }
            continue;
        }
        // Form 2: equals form (--flag=value)
        if let Some(rest) = arg.strip_prefix(&req.flag) {
            if let Some(value) = rest.strip_prefix('=') {
                if req.value.matcher.is_match(value) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::rule::compile_glob;
    use crate::config::rule::{ArgumentPattern, BashRule, RuleConditions};

    fn seg(program: &str, args: &[&str]) -> CommandSegment {
        CommandSegment {
            program: crate::domain::ProgramName::new(program),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn rule(program: &str) -> BashRule {
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions: RuleConditions::default(),
        }
    }

    fn rule_required_flags(program: &str, flags: &[&str]) -> BashRule {
        let mut conditions = RuleConditions::default();
        for f in flags {
            conditions
                .required_flags
                .insert(crate::domain::Flag::new(f));
        }
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions,
        }
    }

    fn rule_optional_flags(program: &str, flags: &[&str]) -> BashRule {
        let mut conditions = RuleConditions::default();
        for f in flags {
            conditions
                .optional_flags
                .insert(crate::domain::Flag::new(f));
        }
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions,
        }
    }

    fn rule_subcommand(program: &str, subcmd: &[&str]) -> BashRule {
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions: RuleConditions {
                subcommand: subcmd.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
        }
    }

    fn rule_positionals(program: &str, patterns: &[&str]) -> BashRule {
        let mut conditions = RuleConditions::default();
        for p in patterns {
            conditions.positionals.push(compile_glob(p).unwrap());
        }
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions,
        }
    }

    fn rule_subcommands(program: &str, chains: &[&[&str]]) -> BashRule {
        let mut conditions = RuleConditions::default();
        for chain in chains {
            conditions
                .subcommands
                .push(chain.iter().map(|s| s.to_string()).collect());
        }
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions,
        }
    }

    fn rule_required_arguments(program: &str, pairs: &[(&str, &str)]) -> BashRule {
        let mut conditions = RuleConditions::default();
        for (flag, value_pattern) in pairs {
            conditions.required_arguments.push(ArgumentPattern {
                flag: flag.to_string(),
                value: compile_glob(value_pattern).unwrap(),
            });
        }
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions,
        }
    }

    // --- Group 1: Empty Conditions (Backwards Compat) ---

    #[test]
    fn match_empty_conditions_with_flags_and_positionals() {
        assert!(rule("rm").matches(&seg("rm", &["-r", "-f", "/tmp"])));
    }

    #[test]
    fn match_empty_conditions_with_no_args() {
        assert!(rule("rm").matches(&seg("rm", &[])));
    }

    #[test]
    fn match_empty_conditions_with_subcommand_and_flag() {
        assert!(rule("git").matches(&seg("git", &["push", "--force"])));
    }

    // --- Group 2: Required Flags (AND) ---

    #[test]
    fn match_required_flags_both_present() {
        let r = rule_required_flags("rm", &["-r", "-f"]);
        assert!(r.matches(&seg("rm", &["-r", "-f", "/tmp"])));
    }

    #[test]
    fn match_required_flags_extra_flag_ok() {
        let r = rule_required_flags("rm", &["-r", "-f"]);
        assert!(r.matches(&seg("rm", &["-r", "-v", "-f", "/tmp"])));
    }

    #[test]
    fn no_match_required_flags_missing_f() {
        let r = rule_required_flags("rm", &["-r", "-f"]);
        assert!(!r.matches(&seg("rm", &["-r", "/tmp"])));
    }

    #[test]
    fn no_match_required_flags_missing_r() {
        let r = rule_required_flags("rm", &["-r", "-f"]);
        assert!(!r.matches(&seg("rm", &["-f", "/tmp"])));
    }

    #[test]
    fn no_match_required_flags_none_present() {
        let r = rule_required_flags("rm", &["-r", "-f"]);
        assert!(!r.matches(&seg("rm", &["/tmp"])));
    }

    #[test]
    fn match_required_flags_position_irrelevant() {
        let r = rule_required_flags("rm", &["-r", "-f"]);
        assert!(r.matches(&seg("rm", &["/", "-r", "-f"])));
    }

    #[test]
    fn match_required_flags_only_no_positionals() {
        let r = rule_required_flags("rm", &["-r", "-f"]);
        assert!(r.matches(&seg("rm", &["-r", "-f"])));
    }

    // --- Group 3: Optional Flags (OR) ---

    #[test]
    fn match_optional_flags_first_present() {
        let r = rule_optional_flags("rm", &["-r", "-f"]);
        assert!(r.matches(&seg("rm", &["-r", "/tmp"])));
    }

    #[test]
    fn match_optional_flags_second_present() {
        let r = rule_optional_flags("rm", &["-r", "-f"]);
        assert!(r.matches(&seg("rm", &["-f", "/tmp"])));
    }

    #[test]
    fn match_optional_flags_both_present() {
        let r = rule_optional_flags("rm", &["-r", "-f"]);
        assert!(r.matches(&seg("rm", &["-r", "-f", "/tmp"])));
    }

    #[test]
    fn no_match_optional_flags_neither_present() {
        let r = rule_optional_flags("rm", &["-r", "-f"]);
        assert!(!r.matches(&seg("rm", &["/tmp"])));
    }

    #[test]
    fn no_match_optional_flags_wrong_flag() {
        let r = rule_optional_flags("rm", &["-r", "-f"]);
        assert!(!r.matches(&seg("rm", &["--verbose", "/tmp"])));
    }

    // --- Group 4: Subcommand Chain (Ordered Prefix) ---

    #[test]
    fn match_subcommand_prefix() {
        let r = rule_subcommand("git", &["push", "origin"]);
        assert!(r.matches(&seg("git", &["push", "origin", "main"])));
    }

    #[test]
    fn match_subcommand_exact() {
        let r = rule_subcommand("git", &["push", "origin"]);
        assert!(r.matches(&seg("git", &["push", "origin"])));
    }

    #[test]
    fn no_match_subcommand_wrong_second() {
        let r = rule_subcommand("git", &["push", "origin"]);
        assert!(!r.matches(&seg("git", &["push", "main"])));
    }

    #[test]
    fn no_match_subcommand_too_few_args() {
        let r = rule_subcommand("git", &["push", "origin"]);
        assert!(!r.matches(&seg("git", &["push"])));
    }

    #[test]
    fn no_match_subcommand_wrong_order() {
        let r = rule_subcommand("git", &["push", "origin"]);
        assert!(!r.matches(&seg("git", &["origin", "push"])));
    }

    #[test]
    fn match_subcommand_with_interleaved_flags() {
        // non-flag args: ["push", "origin", "main"], prefix ["push", "origin"] matches
        let r = rule_subcommand("git", &["push", "origin"]);
        assert!(r.matches(&seg("git", &["push", "origin", "--force", "main"])));
    }

    #[test]
    fn match_subcommand_empty_always_matches() {
        let r = rule_subcommand("rm", &[]);
        assert!(r.matches(&seg("rm", &["/tmp", "file.txt"])));
    }

    #[test]
    fn match_subcommand_empty_no_args() {
        let r = rule_subcommand("rm", &[]);
        assert!(r.matches(&seg("rm", &[])));
    }

    // --- Group 5: Positionals (Presence-Based, Any Order) ---

    #[test]
    fn match_positional_glob_single_level() {
        let r = rule_positionals("rm", &["/*"]);
        assert!(r.matches(&seg("rm", &["/tmp"])));
    }

    #[test]
    fn no_match_positional_glob_nested() {
        let r = rule_positionals("rm", &["/*"]);
        assert!(!r.matches(&seg("rm", &["/home/user"])));
    }

    #[test]
    fn no_match_positional_no_leading_slash() {
        let r = rule_positionals("rm", &["/*"]);
        assert!(!r.matches(&seg("rm", &["file.txt"])));
    }

    #[test]
    fn match_positional_with_flags_stripped() {
        let r = rule_positionals("rm", &["/*"]);
        assert!(r.matches(&seg("rm", &["-r", "-f", "/tmp"])));
    }

    #[test]
    fn match_positional_exact_root() {
        let r = rule_positionals("rm", &["/"]);
        assert!(r.matches(&seg("rm", &["/"])));
    }

    #[test]
    fn match_positional_any_order() {
        // Both "origin" and "main" must each match at least one arg (AND across patterns)
        let r = rule_positionals("git", &["origin", "main"]);
        assert!(r.matches(&seg("git", &["push", "main", "origin"])));
    }

    #[test]
    fn no_match_positional_one_missing() {
        let r = rule_positionals("git", &["origin", "main"]);
        assert!(!r.matches(&seg("git", &["push", "origin"])));
    }

    #[test]
    fn no_match_positional_none_matching() {
        let r = rule_positionals("rm", &["/*"]);
        assert!(!r.matches(&seg("rm", &["file.txt"])));
    }

    // --- Group 6: Subcommand + Positionals Combined ---

    fn rule_subcommand_and_positionals(
        program: &str,
        subcmd: &[&str],
        patterns: &[&str],
    ) -> BashRule {
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions: RuleConditions {
                subcommand: subcmd.iter().map(|s| s.to_string()).collect(),
                positionals: patterns.iter().map(|p| compile_glob(p).unwrap()).collect(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn match_subcommand_and_positional() {
        let r = rule_subcommand_and_positionals("claude", &["mcp", "add"], &["linear"]);
        assert!(r.matches(&seg("claude", &["mcp", "add", "linear"])));
    }

    #[test]
    fn no_match_subcommand_ok_positional_missing() {
        let r = rule_subcommand_and_positionals("claude", &["mcp", "add"], &["linear"]);
        assert!(!r.matches(&seg("claude", &["mcp", "add", "github"])));
    }

    #[test]
    fn no_match_subcommand_wrong_order_with_positional() {
        let r = rule_subcommand_and_positionals("claude", &["mcp", "add"], &["linear"]);
        assert!(!r.matches(&seg("claude", &["add", "mcp", "linear"])));
    }

    #[test]
    fn match_subcommand_and_positional_extra_args() {
        let r = rule_subcommand_and_positionals("claude", &["mcp", "add"], &["linear"]);
        assert!(r.matches(&seg("claude", &["mcp", "add", "linear", "extra"])));
    }

    // --- Group 7: Subcommands Children Block (OR List) ---

    #[test]
    fn match_subcommands_first_chain() {
        let r = rule_subcommands("git", &[&["status"], &["log"], &["diff"]]);
        assert!(r.matches(&seg("git", &["status"])));
    }

    #[test]
    fn match_subcommands_second_chain_with_flag() {
        let r = rule_subcommands("git", &[&["status"], &["log"], &["diff"]]);
        assert!(r.matches(&seg("git", &["log", "--oneline"])));
    }

    #[test]
    fn match_subcommands_third_chain_with_extra_args() {
        let r = rule_subcommands("git", &[&["status"], &["log"], &["diff"]]);
        assert!(r.matches(&seg("git", &["diff", "HEAD~1"])));
    }

    #[test]
    fn no_match_subcommands_not_in_any_chain() {
        let r = rule_subcommands("git", &[&["status"], &["log"], &["diff"]]);
        assert!(!r.matches(&seg("git", &["push"])));
    }

    #[test]
    fn no_match_subcommands_wrong_command() {
        let r = rule_subcommands("git", &[&["status"], &["log"], &["diff"]]);
        assert!(!r.matches(&seg("git", &["rebase", "main"])));
    }

    #[test]
    fn match_subcommands_multi_word_exact() {
        let r = rule_subcommands("git", &[&["push", "origin", "main"]]);
        assert!(r.matches(&seg("git", &["push", "origin", "main"])));
    }

    #[test]
    fn match_subcommands_multi_word_flag_before() {
        let r = rule_subcommands("git", &[&["push", "origin", "main"]]);
        assert!(r.matches(&seg("git", &["--force", "push", "origin", "main"])));
    }

    #[test]
    fn match_subcommands_multi_word_flag_middle() {
        let r = rule_subcommands("git", &[&["push", "origin", "main"]]);
        assert!(r.matches(&seg("git", &["push", "--force", "origin", "main"])));
    }

    #[test]
    fn match_subcommands_multi_word_flag_end() {
        let r = rule_subcommands("git", &[&["push", "origin", "main"]]);
        assert!(r.matches(&seg("git", &["push", "origin", "main", "--force"])));
    }

    #[test]
    fn no_match_subcommands_multi_word_too_few() {
        let r = rule_subcommands("git", &[&["push", "origin", "main"]]);
        assert!(!r.matches(&seg("git", &["push", "origin"])));
    }

    #[test]
    fn no_match_subcommands_multi_word_wrong_order() {
        let r = rule_subcommands("git", &[&["push", "origin", "main"]]);
        assert!(!r.matches(&seg("git", &["push", "main", "origin"])));
    }

    #[test]
    fn match_subcommands_mcp_add() {
        let r = rule_subcommands(
            "claude",
            &[&["mcp", "add"], &["mcp", "remove"], &["mcp", "list"]],
        );
        assert!(r.matches(&seg("claude", &["mcp", "add", "server"])));
    }

    #[test]
    fn match_subcommands_mcp_remove() {
        let r = rule_subcommands(
            "claude",
            &[&["mcp", "add"], &["mcp", "remove"], &["mcp", "list"]],
        );
        assert!(r.matches(&seg("claude", &["mcp", "remove", "server"])));
    }

    #[test]
    fn match_subcommands_mcp_list_exact() {
        let r = rule_subcommands(
            "claude",
            &[&["mcp", "add"], &["mcp", "remove"], &["mcp", "list"]],
        );
        assert!(r.matches(&seg("claude", &["mcp", "list"])));
    }

    #[test]
    fn no_match_subcommands_mcp_wrong_sub() {
        let r = rule_subcommands(
            "claude",
            &[&["mcp", "add"], &["mcp", "remove"], &["mcp", "list"]],
        );
        assert!(!r.matches(&seg("claude", &["config"])));
    }

    #[test]
    fn no_match_subcommands_mcp_wrong_order() {
        let r = rule_subcommands(
            "claude",
            &[&["mcp", "add"], &["mcp", "remove"], &["mcp", "list"]],
        );
        assert!(!r.matches(&seg("claude", &["add", "mcp"])));
    }

    // --- Group 8: Subcommands + Required Flags Combined ---

    fn rule_subcommands_with_flags(program: &str, chains: &[&[&str]], flags: &[&str]) -> BashRule {
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions: RuleConditions {
                subcommands: chains
                    .iter()
                    .map(|c| c.iter().map(|s| s.to_string()).collect())
                    .collect(),
                required_flags: flags.iter().map(|s| crate::domain::Flag::new(s)).collect(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn match_subcommands_and_required_flags() {
        let r = rule_subcommands_with_flags("git", &[&["push"]], &["--force"]);
        assert!(r.matches(&seg("git", &["push", "--force", "origin"])));
    }

    #[test]
    fn match_subcommands_and_required_flags_flag_before() {
        let r = rule_subcommands_with_flags("git", &[&["push"]], &["--force"]);
        assert!(r.matches(&seg("git", &["--force", "push", "origin"])));
    }

    #[test]
    fn match_subcommands_and_required_flags_flag_at_end() {
        let r = rule_subcommands_with_flags("git", &[&["push"]], &["--force"]);
        assert!(r.matches(&seg("git", &["push", "origin", "--force"])));
    }

    #[test]
    fn no_match_subcommands_ok_flags_missing() {
        let r = rule_subcommands_with_flags("git", &[&["push"]], &["--force"]);
        assert!(!r.matches(&seg("git", &["push", "origin"])));
    }

    #[test]
    fn no_match_flags_ok_subcommands_wrong() {
        let r = rule_subcommands_with_flags("git", &[&["push"]], &["--force"]);
        assert!(!r.matches(&seg("git", &["pull", "--force"])));
    }

    // --- Group 9: Required Arguments (Flag+Value Binding) ---

    #[test]
    fn match_required_argument_next_arg() {
        let r = rule_required_arguments("curl", &[("--upload-file", "*")]);
        assert!(r.matches(&seg("curl", &["--upload-file", "data.txt", "url"])));
    }

    #[test]
    fn match_required_argument_equals_form() {
        let r = rule_required_arguments("curl", &[("--upload-file", "*")]);
        assert!(r.matches(&seg("curl", &["--upload-file=data.txt", "url"])));
    }

    #[test]
    fn no_match_required_argument_flag_not_present() {
        let r = rule_required_arguments("curl", &[("--upload-file", "*")]);
        assert!(!r.matches(&seg("curl", &["url"])));
    }

    #[test]
    fn no_match_required_argument_flag_no_value() {
        let r = rule_required_arguments("curl", &[("--upload-file", "*")]);
        assert!(!r.matches(&seg("curl", &["--upload-file"])));
    }

    // --- Group 10: Arg Classification Edge Cases ---

    #[test]
    fn match_classification_standard_flags() {
        // ["-r", "-f", "/tmp"] → flags: {-r, -f}, positionals: ["/tmp"]
        let r = rule_required_flags("rm", &["-r", "-f"]);
        assert!(r.matches(&seg("rm", &["-r", "-f", "/tmp"])));
    }

    #[test]
    fn match_classification_flag_position_irrelevant() {
        // ["/", "-r", "-f"] → flags: {-r, -f}, positionals: ["/"]
        let r = rule_required_flags("rm", &["-r", "-f"]);
        assert!(r.matches(&seg("rm", &["/", "-r", "-f"])));
    }

    #[test]
    fn match_classification_double_dash_stops_flags() {
        // ["--", "-rf", "/tmp"] → flags: {}, positionals: ["-rf", "/tmp"]
        // Required flag "-r" should NOT be found (no flags after --)
        let r = rule_required_flags("rm", &["-r"]);
        assert!(!r.matches(&seg("rm", &["--", "-rf", "/tmp"])));
    }

    #[test]
    fn match_classification_stdin_dash_is_positional() {
        // ["-", "file"] → flags: {}, positionals: ["-", "file"]
        let r = rule_positionals("cat", &["-"]);
        assert!(r.matches(&seg("cat", &["-", "file"])));
    }

    #[test]
    fn match_classification_long_flag() {
        // ["--force", "push"] → flags: {"--force"}, positionals: ["push"]
        let r = rule_required_flags("git", &["--force"]);
        assert!(r.matches(&seg("git", &["--force", "push"])));
    }

    // --- Program basename normalization ---

    #[test]
    fn match_program_basename_absolute_path() {
        assert!(rule("rm").matches(&seg("/usr/bin/rm", &["-r"])));
    }

    #[test]
    fn no_match_program_different_name() {
        assert!(!rule("rm").matches(&seg("mv", &["-f"])));
    }

    // --- Regression: required arguments after -- ---

    #[test]
    fn no_match_required_argument_after_double_dash() {
        // curl -- --upload-file data.txt → flag after -- is positional, not a flag
        let r = rule_required_arguments("curl", &[("--upload-file", "*")]);
        assert!(!r.matches(&seg("curl", &["--", "--upload-file", "data.txt"])));
    }

    #[test]
    fn no_match_required_argument_equals_after_double_dash() {
        let r = rule_required_arguments("curl", &[("--upload-file", "*")]);
        assert!(!r.matches(&seg("curl", &["--", "--upload-file=data.txt"])));
    }

    #[test]
    fn match_required_argument_before_double_dash() {
        // --upload-file before -- is still a valid flag
        let r = rule_required_arguments("curl", &[("--upload-file", "*")]);
        assert!(r.matches(&seg("curl", &["--upload-file", "data.txt", "--", "url"])));
    }

    // --- Group 11: Subcommands + Positionals (Named Matchers) Combined ---

    fn rule_subcommands_with_positionals(
        program: &str,
        chains: &[&[&str]],
        patterns: &[&str],
    ) -> BashRule {
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions: RuleConditions {
                subcommands: chains
                    .iter()
                    .map(|c| c.iter().map(|s| s.to_string()).collect())
                    .collect(),
                positionals: patterns.iter().map(|p| compile_glob(p).unwrap()).collect(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn match_subcommands_and_positionals_both_pass() {
        // deny "git" { subcommands "push"; remotes "origin" }
        let r = rule_subcommands_with_positionals("git", &[&["push"]], &["origin"]);
        assert!(r.matches(&seg("git", &["push", "origin", "main"])));
    }

    #[test]
    fn no_match_subcommands_ok_positionals_missing() {
        // git push upstream → "origin" not in positionals
        let r = rule_subcommands_with_positionals("git", &[&["push"]], &["origin"]);
        assert!(!r.matches(&seg("git", &["push", "upstream", "main"])));
    }

    #[test]
    fn no_match_positionals_ok_subcommands_wrong() {
        // git pull origin → "pull" doesn't match subcommands ["push"]
        let r = rule_subcommands_with_positionals("git", &[&["push"]], &["origin"]);
        assert!(!r.matches(&seg("git", &["pull", "origin"])));
    }

    #[test]
    fn match_subcommands_and_positionals_with_flags() {
        // git --force push origin main → flags stripped, subcommands "push" matches, "origin" in positionals
        let r = rule_subcommands_with_positionals("git", &[&["push"]], &["origin"]);
        assert!(r.matches(&seg("git", &["--force", "push", "origin", "main"])));
    }

    // --- Group 12: Subcommands + Optional Flags Combined ---

    fn rule_subcommands_with_optional_flags(
        program: &str,
        chains: &[&[&str]],
        flags: &[&str],
    ) -> BashRule {
        BashRule {
            program: crate::domain::ProgramName::new(program),
            conditions: RuleConditions {
                subcommands: chains
                    .iter()
                    .map(|c| c.iter().map(|s| s.to_string()).collect())
                    .collect(),
                optional_flags: flags.iter().map(|s| crate::domain::Flag::new(s)).collect(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn match_subcommands_and_optional_flags_first_flag() {
        let r =
            rule_subcommands_with_optional_flags("git", &[&["push"]], &["--force", "--no-verify"]);
        assert!(r.matches(&seg("git", &["push", "--force", "origin"])));
    }

    #[test]
    fn match_subcommands_and_optional_flags_second_flag() {
        let r =
            rule_subcommands_with_optional_flags("git", &[&["push"]], &["--force", "--no-verify"]);
        assert!(r.matches(&seg("git", &["push", "--no-verify", "origin"])));
    }

    #[test]
    fn no_match_subcommands_ok_optional_flags_missing() {
        let r =
            rule_subcommands_with_optional_flags("git", &[&["push"]], &["--force", "--no-verify"]);
        assert!(!r.matches(&seg("git", &["push", "origin"])));
    }

    #[test]
    fn no_match_optional_flags_ok_subcommands_wrong() {
        let r =
            rule_subcommands_with_optional_flags("git", &[&["push"]], &["--force", "--no-verify"]);
        assert!(!r.matches(&seg("git", &["pull", "--force"])));
    }

    // --- Group 13: Empty Subcommands Block ---

    #[test]
    fn match_empty_subcommands_matches_any() {
        // Empty subcommands = no subcommand restriction
        let r = rule_subcommands("git", &[]);
        assert!(r.matches(&seg("git", &["push", "origin"])));
    }

    #[test]
    fn match_empty_subcommands_matches_no_args() {
        let r = rule_subcommands("git", &[]);
        assert!(r.matches(&seg("git", &[])));
    }

    // --- Group 14: Subcommand + Subcommands Both Set (Matching-Level AND) ---
    //
    // The parser normalizes inline+children by prepending `subcommand` to `subcommands`
    // chains and clearing `subcommand`. These tests verify the raw matching-level AND
    // behavior when both fields are manually set — a scenario that no longer arises from
    // normal parsing, but validates that the matcher handles it correctly. See mod.rs
    // for e2e parser normalization tests.

    #[test]
    fn match_inline_subcommand_and_children_subcommands_both_pass() {
        // subcommand: ["push"], subcommands: [["push", "origin"]]
        // Actual: "push origin main" → subcommand prefix ["push"] ✓, chain ["push","origin"] ✓
        let r = BashRule {
            program: crate::domain::ProgramName::new("git"),
            conditions: RuleConditions {
                subcommand: vec!["push".to_string()],
                subcommands: vec![vec!["push".to_string(), "origin".to_string()]],
                ..Default::default()
            },
        };
        assert!(r.matches(&seg("git", &["push", "origin", "main"])));
    }

    #[test]
    fn no_match_inline_subcommand_ok_children_subcommands_miss() {
        // subcommand: ["push"] ✓, subcommands: [["push","origin"]] ✗ (upstream ≠ origin)
        let r = BashRule {
            program: crate::domain::ProgramName::new("git"),
            conditions: RuleConditions {
                subcommand: vec!["push".to_string()],
                subcommands: vec![vec!["push".to_string(), "origin".to_string()]],
                ..Default::default()
            },
        };
        assert!(!r.matches(&seg("git", &["push", "upstream", "main"])));
    }

    #[test]
    fn no_match_children_subcommands_ok_inline_subcommand_miss() {
        // subcommand: ["push"] ✗ (pull ≠ push), subcommands: [["pull"]] would pass but moot
        let r = BashRule {
            program: crate::domain::ProgramName::new("git"),
            conditions: RuleConditions {
                subcommand: vec!["push".to_string()],
                subcommands: vec![vec!["pull".to_string()]],
                ..Default::default()
            },
        };
        assert!(!r.matches(&seg("git", &["pull", "origin"])));
    }
}
