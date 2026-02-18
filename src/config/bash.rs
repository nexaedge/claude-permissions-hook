//! Bash tool configuration.
//!
//! Parses bash-specific rules from the intermediate [`ToolSection`] representation.
//! Handles inline rule parsing (via brush-parser), flag classification,
//! children block interpretation, and command lookup.

use super::rule;
use super::section::{ChildNode, RuleEntry, ToolConfig, ToolSection};
use super::ConfigError;
use crate::command::CommandSegment;
use crate::protocol::Decision;

/// Bash-specific configuration: rules for allow, deny, or ask decisions.
#[derive(Debug, Default)]
pub struct BashConfig {
    pub allow: Vec<rule::BashRule>,
    pub deny: Vec<rule::BashRule>,
    pub ask: Vec<rule::BashRule>,
}

impl ToolConfig for BashConfig {
    const SECTION: &'static str = "bash";

    fn from_section(section: ToolSection) -> Result<Self, ConfigError> {
        Ok(BashConfig {
            allow: parse_rules(section.allow)?,
            deny: parse_rules(section.deny)?,
            ask: parse_rules(section.ask)?,
        })
    }
}

impl BashConfig {
    /// Look up a command segment and return its configured decision.
    ///
    /// Uses `BashRule::matches()` for full condition evaluation (program name,
    /// flags, subcommands, positionals, required arguments).
    /// Precedence: deny > ask > allow. Returns `None` for unlisted programs.
    pub fn lookup(&self, segment: &CommandSegment) -> Option<Decision> {
        if self.deny.iter().any(|r| r.matches(segment)) {
            Some(Decision::Deny)
        } else if self.ask.iter().any(|r| r.matches(segment)) {
            Some(Decision::Ask)
        } else if self.allow.iter().any(|r| r.matches(segment)) {
            Some(Decision::Allow)
        } else {
            None
        }
    }
}

/// Parse a tier's rule entries into BashRules.
pub(super) fn parse_rules(entries: Vec<RuleEntry>) -> Result<Vec<rule::BashRule>, ConfigError> {
    let mut rules = Vec::new();
    for entry in entries {
        let at_line = |e: ConfigError| match e {
            ConfigError::ParseError(msg) => {
                ConfigError::ParseError(format!("line {}: {msg}", entry.line))
            }
            other => other,
        };

        for value in &entry.values {
            let bash_rule = parse_rule_entry(value).map_err(&at_line)?;
            rules.push(bash_rule);
        }

        // Children extend the last parsed rule.
        if let Some(children) = &entry.children {
            if let Some(last_rule) = rules.last_mut() {
                parse_children(children, &mut last_rule.conditions)?;
                // When both inline subcommand and children subcommands exist,
                // children chains are relative to the inline subcommand position.
                // Prepend the inline subcommand to each chain, then clear it.
                // e.g., "git push" { subcommands "origin" } → chains [["push","origin"]]
                normalize_subcommand_chains(&mut last_rule.conditions);
            }
        }
    }
    Ok(rules)
}

/// Parse a single rule entry string into a BashRule.
///
/// Simple program name (no whitespace) -> BashRule with empty conditions.
/// Rule with args -> parse with command::parse(), classify args into conditions.
fn parse_rule_entry(value: &str) -> Result<rule::BashRule, ConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::ParseError("empty rule string".to_string()));
    }

    // Simple program name: no whitespace -> empty conditions
    if trimmed.split_whitespace().nth(1).is_none() {
        return Ok(rule::BashRule {
            program: trimmed.to_string(),
            conditions: rule::RuleConditions::default(),
        });
    }

    // Parse with command::parse() to get program + args
    let segments = crate::command::parse(trimmed)
        .map_err(|e| ConfigError::ParseError(format!("invalid rule '{trimmed}': {e}")))?;

    // Require exactly one command segment
    if segments.len() > 1 {
        return Err(ConfigError::ParseError(format!(
            "rule '{trimmed}' contains multiple commands; use separate rules instead"
        )));
    }

    let segment = segments
        .into_iter()
        .next()
        .ok_or_else(|| ConfigError::ParseError(format!("no program found in rule '{trimmed}'")))?;

    let mut conditions = rule::RuleConditions::default();
    for arg in &segment.args {
        if arg.starts_with('-') {
            conditions.required_flags.insert(arg.clone());
        } else {
            conditions.subcommand.push(arg.clone());
        }
    }

    Ok(rule::BashRule {
        program: segment.program,
        conditions,
    })
}

/// When a rule has both an inline subcommand (from rule string) and children
/// `subcommands` chains, the children are relative to the inline position.
///
/// Prepend the inline subcommand to each children chain, then clear `subcommand`.
/// Example: `"git push" { subcommands "origin" }` → `subcommands [["push","origin"]]`.
fn normalize_subcommand_chains(conditions: &mut rule::RuleConditions) {
    if conditions.subcommand.is_empty() || conditions.subcommands.is_empty() {
        return;
    }
    for chain in &mut conditions.subcommands {
        let mut merged = conditions.subcommand.clone();
        merged.append(chain);
        *chain = merged;
    }
    conditions.subcommand.clear();
}

/// Parse children nodes to extend rule conditions.
///
/// Operates on the intermediate [`ChildNode`] representation — no KDL dependency.
fn parse_children(
    children: &[ChildNode],
    conditions: &mut rule::RuleConditions,
) -> Result<(), ConfigError> {
    for child in children {
        let line = child.line;
        let glob_at_line = |msg: String| ConfigError::ParseError(format!("line {line}: {msg}"));
        let err_at_line = |e: ConfigError| match e {
            ConfigError::ParseError(msg) => ConfigError::ParseError(format!("line {line}: {msg}")),
            other => other,
        };

        match child.name.as_str() {
            "required-flags" => {
                for v in &child.values {
                    conditions.required_flags.insert(rule::normalize_flag(v));
                }
            }
            "optional-flags" => {
                for v in &child.values {
                    conditions.optional_flags.insert(rule::normalize_flag(v));
                }
            }
            "positionals" => {
                for v in &child.values {
                    let pattern = rule::compile_glob(v).map_err(&glob_at_line)?;
                    conditions.positionals.push(pattern);
                }
            }
            "required-arguments" => {
                for v in &child.values {
                    let pattern = parse_argument_pattern(v).map_err(&err_at_line)?;
                    conditions.required_arguments.push(pattern);
                }
            }
            "subcommands" => {
                for v in &child.values {
                    let chain: Vec<String> = v.split_whitespace().map(String::from).collect();
                    conditions.subcommands.push(chain);
                }
            }
            _ => {
                // Named positional matcher (e.g., `files "/*"`, `remotes "linear"`)
                for v in &child.values {
                    let pattern = rule::compile_glob(v).map_err(&glob_at_line)?;
                    conditions.positionals.push(pattern);
                }
            }
        }
    }
    Ok(())
}

/// Parse a `required-arguments` entry: `"--upload-file *"` -> ArgumentPattern.
fn parse_argument_pattern(value: &str) -> Result<rule::ArgumentPattern, ConfigError> {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(ConfigError::ParseError(format!(
            "required-arguments entry must have flag and value pattern: '{value}'"
        )));
    }
    let flag = parts[0].to_string();
    let pattern = rule::compile_glob(parts[1]).map_err(ConfigError::ParseError)?;
    Ok(rule::ArgumentPattern {
        flag,
        value: pattern,
    })
}
