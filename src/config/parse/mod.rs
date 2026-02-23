pub(crate) mod bash;
pub(crate) mod files;

use crate::config::document::{ConfigDocument, ParseNode};
use crate::config::ConfigError;

/// Unified KDL intermediate representation for a single config node.
///
/// Replaces the old `RuleEntry` + `ChildNode` pair. Used by both bash and files parsers.
pub(super) struct ConfigNode {
    pub name: String,
    pub arguments: Vec<String>,
    pub body: Option<Vec<ConfigNode>>,
    pub line: usize,
}

impl ConfigNode {
    /// Find a section by name and return its body (child nodes).
    ///
    /// Returns `None` when the section is absent or has no children.
    pub fn body_of<'a>(nodes: &'a [ConfigNode], name: &str) -> Option<&'a [ConfigNode]> {
        nodes
            .iter()
            .find(|n| n.name == name)
            .and_then(|n| n.body.as_deref())
            .filter(|body| !body.is_empty())
    }

    /// Return child nodes, or an empty slice if there is no body.
    pub fn body_nodes(&self) -> &[ConfigNode] {
        self.body.as_deref().unwrap_or(&[])
    }
}

/// Convert a whole config document into a list of top-level `ConfigNode` values.
///
/// Each node represents a section block (e.g. `bash { … }`, `files { … }`). Its `body`
/// contains the rule nodes, whose own `body` fields contain condition/operation nodes.
pub(super) fn section_to_config_nodes(doc: &ConfigDocument) -> Vec<ConfigNode> {
    convert_nodes(&doc.nodes())
}

/// Recursively convert `ParseNode` values into `ConfigNode` values.
fn convert_nodes(nodes: &[ParseNode<'_>]) -> Vec<ConfigNode> {
    nodes
        .iter()
        .map(|node| ConfigNode {
            name: node.name().to_string(),
            arguments: node.string_values().into_iter().map(String::from).collect(),
            body: node.children().map(|children| convert_nodes(&children)),
            line: node.line(),
        })
        .collect()
}

/// Parse a decision tier name into a `Decision`.
///
/// Used by both bash and files parsers. Returns `Err` for unknown tiers.
pub(super) fn parse_tier(name: &str, line: usize) -> Result<crate::domain::Decision, ConfigError> {
    match name {
        "allow" => Ok(crate::domain::Decision::Allow),
        "deny" => Ok(crate::domain::Decision::Deny),
        "ask" => Ok(crate::domain::Decision::Ask),
        other => Err(ConfigError::ParseError(format!(
            "line {line}: unknown tier \"{other}\"; expected allow, deny, or ask"
        ))),
    }
}

/// Test-only: parse raw KDL source as rule-level `ConfigNode` values.
///
/// Wraps source in a synthetic `test { … }` section and returns its children —
/// the same node shape that `parse_bash_nodes` / `parse_file_nodes` expect.
#[cfg(test)]
pub(super) fn parse_section_from_source(source: &str) -> Result<Vec<ConfigNode>, ConfigError> {
    let wrapped = format!("test {{\n{source}\n}}");
    let doc = ConfigDocument::parse(&wrapped)?;
    let all_nodes = doc.nodes();
    let test_node = all_nodes
        .iter()
        .find(|n| n.name() == "test")
        .expect("synthetic test section must exist");
    Ok(convert_nodes(&test_node.children().unwrap_or_default()))
}
