use globset::Glob;

/// Error from path normalization or expansion.
#[derive(Debug, thiserror::Error)]
pub enum PathError {
    /// `$HOME` is not set but is required to expand `~` or `<home>` in a path.
    #[error("$HOME not set, cannot expand '~' in path: {0}")]
    HomeNotSet(String),
}

/// A normalized absolute file path.
///
/// Tilde expanded, relative paths resolved against cwd,
/// `..` and `.` components collapsed (logical, no filesystem access).
///
/// Constructed via [`NormalizedPath::new`] which returns `Result` — never panics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NormalizedPath(pub(crate) String);

impl NormalizedPath {
    /// Normalize a path against the given cwd.
    ///
    /// Steps:
    /// 1. Expand leading `~` to `$HOME`
    /// 2. Prepend `cwd` if path is relative
    /// 3. Collapse `..` and `.` components logically (no filesystem access)
    /// 4. Collapse duplicate `/` separators
    /// 5. Remove trailing `/`
    ///
    /// Returns `Err(PathError::HomeNotSet)` if the path starts with `~` and
    /// `$HOME` is not set.
    pub fn new(raw: &str, cwd: &str) -> Result<Self, PathError> {
        // Step 1: Expand tilde
        let path = if let Some(rest) = raw.strip_prefix('~') {
            let home = std::env::var("HOME").map_err(|_| PathError::HomeNotSet(raw.to_string()))?;
            format!("{home}{rest}")
        } else {
            raw.to_string()
        };

        // Step 2: Make absolute
        let path = if path.starts_with('/') {
            path
        } else {
            format!("{cwd}/{path}")
        };

        // Step 3 & 4: Split on `/`, collapse `..` and empty components
        let mut components: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {} // skip empty segments and current-dir markers
                ".." => {
                    components.pop();
                }
                other => components.push(other),
            }
        }

        // Step 5: Rebuild as absolute path
        let result = if components.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", components.join("/"))
        };

        Ok(NormalizedPath(result))
    }

    #[allow(dead_code)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Returns the home directory from `$HOME`.
///
/// Returns `Err(PathError::HomeNotSet)` if `$HOME` is not set.
pub(crate) fn home_dir() -> Result<String, PathError> {
    std::env::var("HOME").map_err(|_| PathError::HomeNotSet("$HOME".to_string()))
}

/// Normalizes a file path to a clean absolute form.
///
/// Delegates to [`NormalizedPath::new`].
/// Returns `Err(PathError::HomeNotSet)` if the path starts with `~` and
/// `$HOME` is not set.
pub(crate) fn normalize(path: &str, cwd: &str) -> Result<String, PathError> {
    NormalizedPath::new(path, cwd).map(|p| p.0)
}

/// Tests whether a normalized path matches an expanded glob pattern.
///
/// Uses `globset::Glob` for matching. Case-sensitive, `**` matches path
/// separators (globset default).
///
/// Returns `Err` if the pattern is an invalid glob (caller should treat as
/// fail-closed).
pub(crate) fn matches(path: &str, expanded_pattern: &str) -> Result<bool, String> {
    let glob = Glob::new(expanded_pattern).map_err(|e| format!("invalid glob pattern: {e}"))?;
    let matcher = glob.compile_matcher();
    Ok(matcher.is_match(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> String {
        std::env::var("HOME").unwrap()
    }

    // ---- NormalizedPath tests ----

    #[test]
    fn new_absolute_path_unchanged() {
        assert_eq!(
            NormalizedPath::new("/absolute/path", "/any")
                .unwrap()
                .as_str(),
            "/absolute/path"
        );
    }

    #[test]
    fn new_relative_prepends_cwd() {
        assert_eq!(
            NormalizedPath::new("relative/file", "/home/user")
                .unwrap()
                .as_str(),
            "/home/user/relative/file"
        );
    }

    #[test]
    fn new_tilde_expands_home() {
        let h = home();
        assert_eq!(
            NormalizedPath::new("~/project/file", "/any")
                .unwrap()
                .as_str(),
            format!("{h}/project/file")
        );
    }

    #[test]
    fn new_dotdot_collapses() {
        assert_eq!(
            NormalizedPath::new("/foo/bar/../baz", "/any")
                .unwrap()
                .as_str(),
            "/foo/baz"
        );
    }

    #[test]
    fn new_duplicate_slashes_collapsed() {
        assert_eq!(
            NormalizedPath::new("/foo//bar///baz", "/any")
                .unwrap()
                .as_str(),
            "/foo/bar/baz"
        );
    }

    #[test]
    fn new_trailing_slash_removed() {
        assert_eq!(
            NormalizedPath::new("/path/", "/any").unwrap().as_str(),
            "/path"
        );
    }

    #[test]
    fn new_root_stays_root() {
        assert_eq!(NormalizedPath::new("/", "/any").unwrap().as_str(), "/");
    }

    // ---- normalize tests ----

    #[test]
    fn normalize_absolute_path_unchanged() {
        assert_eq!(
            normalize("/absolute/path", "/any").unwrap(),
            "/absolute/path"
        );
    }

    #[test]
    fn normalize_relative_prepends_cwd() {
        assert_eq!(
            normalize("relative/file", "/home/user").unwrap(),
            "/home/user/relative/file"
        );
    }

    #[test]
    fn normalize_dot_relative_prepends_cwd() {
        assert_eq!(
            normalize("./src/main.rs", "/project").unwrap(),
            "/project/src/main.rs"
        );
    }

    #[test]
    fn normalize_tilde_expands_home() {
        let h = home();
        assert_eq!(
            normalize("~/project/file", "/any").unwrap(),
            format!("{h}/project/file")
        );
    }

    #[test]
    fn normalize_dotdot_collapses() {
        assert_eq!(normalize("/foo/bar/../baz", "/any").unwrap(), "/foo/baz");
    }

    #[test]
    fn normalize_nested_dotdot_collapses() {
        assert_eq!(normalize("/foo/bar/../../baz", "/any").unwrap(), "/baz");
    }

    #[test]
    fn normalize_duplicate_slashes() {
        assert_eq!(
            normalize("/foo//bar///baz", "/any").unwrap(),
            "/foo/bar/baz"
        );
    }

    #[test]
    fn normalize_relative_dotdot() {
        assert_eq!(
            normalize("../other/file", "/home/user/project").unwrap(),
            "/home/user/other/file"
        );
    }

    #[test]
    fn normalize_trailing_slash_removed() {
        assert_eq!(normalize("/path/", "/any").unwrap(), "/path");
    }

    // ---- matches tests ----

    #[test]
    fn matches_star_pattern() {
        assert_eq!(matches("/foo/bar.rs", "/foo/*.rs"), Ok(true));
    }

    #[test]
    fn matches_double_star_pattern() {
        assert_eq!(matches("/foo/bar/baz.rs", "/foo/**/*.rs"), Ok(true));
    }

    #[test]
    fn matches_non_matching_pattern() {
        assert_eq!(matches("/foo/bar.rs", "/baz/*.rs"), Ok(false));
    }

    #[test]
    fn matches_question_mark_no_match() {
        assert_eq!(matches("/foo/bar.rs", "/foo/?.rs"), Ok(false));
    }

    #[test]
    fn matches_question_mark_single_char() {
        assert_eq!(matches("/foo/b.rs", "/foo/?.rs"), Ok(true));
    }

    #[test]
    fn matches_exact_path() {
        assert_eq!(matches("/foo/bar", "/foo/bar"), Ok(true));
    }

    #[test]
    fn matches_character_class_positive() {
        assert_eq!(matches("/foo/bar.rs", "/foo/[ab]ar.rs"), Ok(true));
    }

    #[test]
    fn matches_character_class_negative() {
        assert_eq!(matches("/foo/car.rs", "/foo/[ab]ar.rs"), Ok(false));
    }

    #[test]
    fn matches_invalid_pattern_returns_err() {
        let result = matches("/foo/bar", "[invalid");
        assert!(result.is_err());
    }
}
