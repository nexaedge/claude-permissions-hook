//! Shared tool section types and parsing.
//!
//! Converts KDL sections into tool-agnostic intermediate types.
//! Tool modules consume these without any KDL dependency.

use super::document::{ConfigDocument, ConfigSection};
use super::ConfigError;

/// Trait for tool-specific configuration.
///
/// Each tool (bash, write, etc.) implements this to define its section
/// name and how to build config from the intermediate representation.
pub(crate) trait ToolConfig: Default + std::fmt::Debug {
    /// KDL section name (e.g., `"bash"`, `"write"`).
    const SECTION: &'static str;

    /// Build tool config from the parsed intermediate section.
    fn from_section(section: ToolSection) -> Result<Self, ConfigError>;
}

/// Intermediate representation of a tool's configuration section.
///
/// Contains tiered rule entries (allow/deny/ask) with no KDL dependency.
pub(crate) struct ToolSection {
    pub allow: Vec<RuleEntry>,
    pub deny: Vec<RuleEntry>,
    pub ask: Vec<RuleEntry>,
}

/// A single rule entry from a config node.
///
/// Represents one `allow "..."`, `deny "..."`, or `ask "..."` node with
/// its string values and optional children block.
pub(crate) struct RuleEntry {
    /// String values (e.g., `["git", "cargo"]` from `allow "git" "cargo"`).
    pub values: Vec<String>,
    /// Parsed children block, if present.
    pub children: Option<Vec<ChildNode>>,
    /// 1-based line number in the source file.
    pub line: usize,
}

/// A child node within a rule's children block.
///
/// Represents nodes like `required-flags "r" "f"` or `positionals "/*"`.
pub(crate) struct ChildNode {
    /// Node name (e.g., `"required-flags"`, `"positionals"`).
    pub name: String,
    /// String values from the node.
    pub values: Vec<String>,
    /// 1-based line number in the source file.
    pub line: usize,
}

/// Parse a tool section from KDL into the tool's config type.
///
/// Looks up the section by `T::SECTION`, parses it into [`ToolSection`],
/// and delegates to [`ToolConfig::from_section`]. Returns `T::default()`
/// when the section is absent.
pub(super) fn parse_tool<T: ToolConfig>(kdl: &ConfigDocument) -> Result<T, ConfigError> {
    match kdl.section(T::SECTION) {
        Some(section_kdl) => {
            let section = parse_section(&section_kdl)?;
            T::from_section(section)
        }
        None => Ok(T::default()),
    }
}

fn parse_section(kdl: &ConfigSection) -> Result<ToolSection, ConfigError> {
    Ok(ToolSection {
        allow: collect_entries(kdl, "allow")?,
        deny: collect_entries(kdl, "deny")?,
        ask: collect_entries(kdl, "ask")?,
    })
}

/// Collect all rule entries from nodes with the given tier name.
///
/// Validates structural constraints common to all tools:
/// - Children block requires exactly one string entry
/// - Children block without any entry is rejected
fn collect_entries(kdl: &ConfigSection, tier: &str) -> Result<Vec<RuleEntry>, ConfigError> {
    let mut entries = Vec::new();
    for node in kdl.nodes_named(tier) {
        let line = node.line();
        let values: Vec<String> = node.string_values().into_iter().map(String::from).collect();
        let has_children = node.has_children();

        if has_children && values.is_empty() {
            return Err(ConfigError::ParseError(format!(
                "line {line}: {tier} node has a children block but no program entry"
            )));
        }

        if has_children && values.len() > 1 {
            return Err(ConfigError::ParseError(format!(
                "line {line}: {tier} node has a children block with multiple entries; \
                 use separate nodes instead"
            )));
        }

        let children = node.children().map(|children_kdl| {
            children_kdl
                .nodes()
                .into_iter()
                .map(|child| ChildNode {
                    name: child.name().to_string(),
                    values: child.string_values().into_iter().map(String::from).collect(),
                    line: child.line(),
                })
                .collect()
        });

        entries.push(RuleEntry {
            values,
            children,
            line,
        });
    }
    Ok(entries)
}

/// Test-only: parse raw KDL source directly into a ToolSection.
///
/// Wraps the source in a synthetic `test { … }` section so that
/// `parse_section` can operate on the children block.
#[cfg(test)]
pub(super) fn parse_from_source(source: &str) -> Result<ToolSection, ConfigError> {
    let wrapped = format!("test {{\n{source}\n}}");
    let kdl = ConfigDocument::parse(&wrapped)?;
    let section = kdl.section("test").expect("synthetic test section must exist");
    parse_section(&section)
}
