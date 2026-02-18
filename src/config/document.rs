//! Config document abstraction layer.
//!
//! `ConfigDocument`, `ConfigSection`, and `ParseNode` wrap the `kdl` crate
//! types so the rest of the config module never touches KDL directly.

/// Parsed KDL document paired with its source text.
///
/// Provides section lookup and node iteration that return [`ParseNode`]
/// wrappers carrying source context for line-number error reporting.
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
    pub(super) fn parse(source: &str) -> Result<Self, super::ConfigError> {
        let doc: kdl::KdlDocument = source
            .parse()
            .map_err(|e: kdl::KdlError| super::ConfigError::ParseError(e.to_string()))?;
        Ok(Self {
            doc,
            source: source.to_string(),
        })
    }

    /// Load and parse a KDL config file.
    pub(super) fn load(path: &std::path::Path) -> Result<Self, super::ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                super::ConfigError::NotFound(path.to_path_buf())
            } else {
                super::ConfigError::ReadError(e)
            }
        })?;
        Self::parse(&content)
    }

    /// Get a named top-level section's children as a borrowed `ConfigSection`.
    ///
    /// `section("bash")` returns the contents of the `bash { … }` block.
    pub(super) fn section(&self, name: &str) -> Option<ConfigSection<'_>> {
        self.doc
            .get(name)
            .and_then(|n| n.children())
            .map(|doc| ConfigSection {
                doc,
                source: &self.source,
            })
    }
}

/// Borrowed view into a KDL section (children block of a top-level node).
///
/// Provides node iteration for parsing tool sections.
pub(super) struct ConfigSection<'a> {
    doc: &'a kdl::KdlDocument,
    source: &'a str,
}

impl<'a> ConfigSection<'a> {
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

    /// Total number of entries (all types, not just strings).
    pub(super) fn entry_count(&self) -> usize {
        self.node.entries().len()
    }

    /// Whether this node has a children block `{ … }`.
    pub(super) fn has_children(&self) -> bool {
        self.node.children().is_some()
    }

    /// Get the children block as a borrowed `ConfigSection` (preserving source).
    pub(super) fn children(&self) -> Option<ConfigSection<'a>> {
        self.node.children().map(|doc| ConfigSection {
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
