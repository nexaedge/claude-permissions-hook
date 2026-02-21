use globset::Glob;

pub(crate) use crate::domain::path::PathError;

/// Returns the home directory from `$HOME`.
///
/// Returns `Err(PathError::HomeNotSet)` if `$HOME` is not set.
pub(crate) fn home_dir() -> Result<String, PathError> {
    std::env::var("HOME").map_err(|_| PathError::HomeNotSet("$HOME".to_string()))
}

/// Normalizes a file path to a clean absolute form.
///
/// Delegates to [`crate::domain::path::NormalizedPath::new`].
/// Returns `Err(PathError::HomeNotSet)` if the path starts with `~` and
/// `$HOME` is not set.
pub(crate) fn normalize(path: &str, cwd: &str) -> Result<String, PathError> {
    crate::domain::path::NormalizedPath::new(path, cwd).map(|p| p.0)
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

    // ---- home helper for tests ----

    fn test_home() -> String {
        std::env::var("HOME").unwrap()
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
        let home = test_home();
        assert_eq!(
            normalize("~/project/file", "/any").unwrap(),
            format!("{home}/project/file")
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
