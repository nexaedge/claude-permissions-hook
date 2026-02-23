//! Config document abstraction layer.
//!
//! `ConfigDocument` and `ParseNode` wrap the `kdl` crate types so the rest
//! of the config module never touches KDL directly.

/// Parsed KDL document paired with its source text.
///
/// Provides node iteration that returns [`ParseNode`] wrappers carrying
/// source context for line-number error reporting.
pub(super) struct ConfigDocument {
    doc: kdl::KdlDocument,
    source: String,
}

/// Single KDL node with source context for line-number reporting.
pub(super) struct ParseNode<'a> {
    node: &'a kdl::KdlNode,
    source: &'a str,
}

impl ConfigDocument {
    /// Parse a KDL source string into a document.
    pub(super) fn parse(source: &str) -> Result<Self, crate::error::ConfigError> {
        let doc: kdl::KdlDocument = source
            .parse()
            .map_err(|e: kdl::KdlError| {
                crate::error::ConfigError::InvalidSyntax(e.to_string())
            })?;
        Ok(Self {
            doc,
            source: source.to_string(),
        })
    }

    /// Iterate over all top-level nodes in the document.
    pub(super) fn nodes(&self) -> Vec<ParseNode<'_>> {
        self.doc
            .nodes()
            .iter()
            .map(|node| ParseNode {
                node,
                source: &self.source,
            })
            .collect()
    }
}

impl<'a> ParseNode<'a> {
    /// The node's identifier (e.g. `"bash"`, `"deny"`, `"required-flags"`).
    pub(super) fn name(&self) -> &str {
        self.node.name().value()
    }

    /// Collect all string-valued entries from this node.
    pub(super) fn string_values(&self) -> Vec<&'a str> {
        self.node
            .entries()
            .iter()
            .filter_map(|e| e.value().as_string())
            .collect()
    }

    /// Get child nodes, or `None` if there is no children block.
    pub(super) fn children(&self) -> Option<Vec<ParseNode<'a>>> {
        self.node.children().map(|doc| {
            doc.nodes()
                .iter()
                .map(|node| ParseNode {
                    node,
                    source: self.source,
                })
                .collect()
        })
    }

    /// 1-based line number of this node in the original source.
    pub(super) fn line(&self) -> usize {
        let offset = self.node.span().offset();
        self.source[..offset.min(self.source.len())]
            .bytes()
            .filter(|&b| b == b'\n')
            .count()
            + 1
    }
}
