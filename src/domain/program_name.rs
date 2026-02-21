/// A normalized program name (basename only, no path prefix).
///
/// Constructed from a raw string; extracts the basename component
/// so `/usr/bin/rm` and `rm` both produce `ProgramName("rm")`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProgramName(pub(crate) String);

impl ProgramName {
    /// Create from a raw program string.
    ///
    /// Extracts the basename if the input contains path separators.
    /// `/usr/bin/rm` → `rm`, `./scripts/deploy.sh` → `deploy.sh`, `git` → `git`.
    pub fn new(raw: &str) -> Self {
        let basename = raw.rsplit('/').next().unwrap_or(raw);
        ProgramName(basename.to_string())
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
    fn new_simple_name_unchanged() {
        assert_eq!(ProgramName::new("git").as_str(), "git");
    }

    #[test]
    fn new_absolute_path_extracts_basename() {
        assert_eq!(ProgramName::new("/usr/bin/rm").as_str(), "rm");
    }

    #[test]
    fn new_relative_path_extracts_basename() {
        assert_eq!(
            ProgramName::new("./scripts/deploy.sh").as_str(),
            "deploy.sh"
        );
    }

    #[test]
    fn new_trailing_slash_yields_empty() {
        // rsplit on "foo/" yields ["", "foo"] → takes first → ""
        assert_eq!(ProgramName::new("foo/").as_str(), "");
    }

    #[test]
    fn eq_str_works() {
        assert_eq!(ProgramName::new("rm"), "rm");
        assert_ne!(ProgramName::new("rm"), "mv");
    }

    #[test]
    fn eq_ref_str_works() {
        let s = "rm".to_string();
        assert_eq!(ProgramName::new("rm"), s.as_str());
    }
}
