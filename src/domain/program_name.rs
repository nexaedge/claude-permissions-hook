/// A normalized program name (basename only, no path prefix).
///
/// Constructed from a raw string; extracts the basename component
/// so `/usr/bin/rm` and `rm` both produce `ProgramName("rm")`.
///
/// Invariant: the inner string is never empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProgramName(pub(crate) String);

/// Error when a program name cannot be constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyProgramName;

impl std::fmt::Display for EmptyProgramName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "program name is empty")
    }
}

impl std::error::Error for EmptyProgramName {}

impl ProgramName {
    /// Create from a raw program string.
    ///
    /// Extracts the basename if the input contains path separators.
    /// `/usr/bin/rm` → `rm`, `./scripts/deploy.sh` → `deploy.sh`, `git` → `git`.
    ///
    /// Returns `Err` if the resulting basename is empty (e.g., trailing slash `"foo/"`).
    pub fn parse(raw: &str) -> Result<Self, EmptyProgramName> {
        let basename = raw.rsplit('/').next().unwrap_or(raw);
        if basename.is_empty() {
            return Err(EmptyProgramName);
        }
        Ok(ProgramName(basename.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for ProgramName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for ProgramName {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl std::fmt::Display for ProgramName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_name() {
        assert_eq!(ProgramName::parse("git").unwrap().as_str(), "git");
    }

    #[test]
    fn parse_absolute_path_extracts_basename() {
        assert_eq!(ProgramName::parse("/usr/bin/rm").unwrap().as_str(), "rm");
    }

    #[test]
    fn parse_relative_path_extracts_basename() {
        assert_eq!(
            ProgramName::parse("./scripts/deploy.sh").unwrap().as_str(),
            "deploy.sh"
        );
    }

    #[test]
    fn parse_trailing_slash_rejects_empty() {
        assert!(ProgramName::parse("foo/").is_err());
    }

    #[test]
    fn parse_empty_string_rejects() {
        assert!(ProgramName::parse("").is_err());
    }

    #[test]
    fn eq_str_works() {
        assert_eq!(ProgramName::parse("rm").unwrap(), "rm");
        assert_ne!(ProgramName::parse("rm").unwrap(), "mv");
    }

    #[test]
    fn eq_ref_str_works() {
        let s = "rm".to_string();
        assert_eq!(ProgramName::parse("rm").unwrap(), s.as_str());
    }
}
