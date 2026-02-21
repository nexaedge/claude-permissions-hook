/// A normalized flag (always with leading dashes).
///
/// Constructed from a raw flag string; normalizes dash prefix so that
/// bare single-char (`r`) becomes `-r` and multi-char (`force`) becomes `--force`.
/// Already-dashed flags (`-r`, `--force`) are kept as-is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Flag(pub(crate) String);

impl Flag {
    /// Normalize a flag string.
    ///
    /// - Already starts with `-` → kept as-is
    /// - Single character `"r"` → `"-r"`
    /// - Multi-character `"force"` → `"--force"`
    pub fn new(raw: &str) -> Self {
        let normalized = if raw.starts_with('-') {
            raw.to_string()
        } else if raw.len() == 1 {
            format!("-{raw}")
        } else {
            format!("--{raw}")
        };
        Flag(normalized)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for Flag {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Flag {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl std::fmt::Display for Flag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_already_dashed_single_unchanged() {
        assert_eq!(Flag::new("-r").as_str(), "-r");
    }

    #[test]
    fn new_already_dashed_long_unchanged() {
        assert_eq!(Flag::new("--force").as_str(), "--force");
    }

    #[test]
    fn new_bare_single_char_gets_dash() {
        assert_eq!(Flag::new("r").as_str(), "-r");
        assert_eq!(Flag::new("f").as_str(), "-f");
    }

    #[test]
    fn new_bare_multi_char_gets_double_dash() {
        assert_eq!(Flag::new("force").as_str(), "--force");
        assert_eq!(Flag::new("verbose").as_str(), "--verbose");
    }

    #[test]
    fn eq_str_works() {
        assert_eq!(Flag::new("-r"), "-r");
        assert_ne!(Flag::new("-r"), "-f");
    }
}
