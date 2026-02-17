//! KDL abstraction layer.
//!
//! `KdlParse` and `ParseNode` wrap the `kdl` crate types so the rest of the
//! config module never touches KDL iterators, entries, or spans directly.

/// Parsed KDL document paired with its source text.
///
/// Provides section lookup and node iteration that return [`ParseNode`]
/// wrappers carrying source context for line-number error reporting.
pub(super) struct KdlParse<'a> {
    doc: &'a kdl::KdlDocument,
    source: &'a str,
}

/// Single KDL node with source context for line-number reporting.
pub(super) struct ParseNode<'a> {
    node: &'a kdl::KdlNode,
    source: &'a str,
}

impl<'a> KdlParse<'a> {
    /// Create a new `KdlParse` from a document and its original source text.
    pub(super) fn new(doc: &'a kdl::KdlDocument, source: &'a str) -> Self {
        Self { doc, source }
    }

    /// Parse a KDL source string into a document and wrap it.
    ///
    /// Returns a `ConfigError::ParseError` on invalid syntax.
    pub(super) fn parse(source: &str) -> Result<(kdl::KdlDocument, &str), super::ConfigError> {
        let doc: kdl::KdlDocument = source
            .parse()
            .map_err(|e: kdl::KdlError| super::ConfigError::ParseError(e.to_string()))?;
        Ok((doc, source))
    }

    /// Get a named top-level section's children as a new `KdlParse`.
    ///
    /// `section("bash")` returns the contents of the `bash { … }` block.
    pub(super) fn section(&self, name: &str) -> Option<KdlParse<'a>> {
        self.doc
            .get(name)
            .and_then(|n| n.children())
            .map(|doc| KdlParse {
                doc,
                source: self.source,
            })
    }

    /// Iterate over child nodes whose name matches `name`.
    pub(super) fn nodes_named(&self, name: &str) -> Vec<ParseNode<'a>> {
        self.doc
            .nodes()
            .iter()
            .filter(|n| n.name().value() == name)
            .map(|node| ParseNode {
                node,
                source: self.source,
            })
            .collect()
    }

    /// Iterate over all child nodes.
    pub(super) fn nodes(&self) -> Vec<ParseNode<'a>> {
        self.doc
            .nodes()
            .iter()
            .map(|node| ParseNode {
                node,
                source: self.source,
            })
            .collect()
    }
}

impl<'a> ParseNode<'a> {
    /// The node's identifier (e.g. `"deny"`, `"required-flags"`).
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

    /// Whether this node has a children block `{ … }`.
    pub(super) fn has_children(&self) -> bool {
        self.node.children().is_some()
    }

    /// Get the children block as a new `KdlParse` (preserving source).
    pub(super) fn children(&self) -> Option<KdlParse<'a>> {
        self.node.children().map(|doc| KdlParse {
            doc,
            source: self.source,
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
