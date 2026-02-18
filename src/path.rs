use globset::Glob;

/// Returns the home directory from `$HOME`.
///
/// # Panics
///
/// Panics if `$HOME` is not set. This is reasonable for a macOS/Linux CLI tool
/// where `$HOME` is always defined.
fn home_dir() -> String {
    std::env::var("HOME").expect("$HOME environment variable must be set")
}

/// Normalizes a file path to a clean absolute form.
///
/// Steps:
/// 1. Expand leading `~` to home directory
/// 2. Prepend `cwd` if path is relative
/// 3. Collapse `..` components logically (no filesystem access)
/// 4. Collapse duplicate `/` separators
/// 5. Remove trailing `/`
pub fn normalize(path: &str, cwd: &str) -> String {
    // Step 1: Expand tilde
    let path = if let Some(rest) = path.strip_prefix('~') {
        format!("{}{rest}", home_dir())
    } else {
        path.to_string()
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
    if components.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", components.join("/"))
    }
}

/// Expands built-in variables in a config pattern.
///
/// Expansion order:
/// 1. `<cwd>` → CWD value
/// 2. `<home>` → home directory (`$HOME`)
/// 3. Leading `~` → home directory (only at start of pattern)
pub fn expand_pattern(pattern: &str, cwd: &str) -> String {
    let home = home_dir();
    let result = pattern.replace("<cwd>", cwd);
    let result = result.replace("<home>", &home);
    if let Some(rest) = result.strip_prefix('~') {
        format!("{home}{rest}")
    } else {
        result
    }
}

/// Tests whether a normalized path matches an expanded glob pattern.
///
/// Uses `globset::Glob` for matching. Case-sensitive, `**` matches path
/// separators (globset default).
///
/// Returns `Err` if the pattern is an invalid glob (caller should treat as
/// fail-closed).
pub fn matches(path: &str, expanded_pattern: &str) -> Result<bool, String> {
    let glob = Glob::new(expanded_pattern).map_err(|e| format!("invalid glob pattern: {e}"))?;
    let matcher = glob.compile_matcher();
    Ok(matcher.is_match(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- home_dir helper for tests ----

    fn test_home() -> String {
        std::env::var("HOME").unwrap()
    }

    // ---- normalize tests ----

    #[test]
    fn normalize_absolute_path_unchanged() {
        assert_eq!(normalize("/absolute/path", "/any"), "/absolute/path");
    }

    #[test]
    fn normalize_relative_prepends_cwd() {
        assert_eq!(
            normalize("relative/file", "/home/user"),
            "/home/user/relative/file"
        );
    }

    #[test]
    fn normalize_dot_relative_prepends_cwd() {
        assert_eq!(
            normalize("./src/main.rs", "/project"),
            "/project/src/main.rs"
        );
    }

    #[test]
    fn normalize_tilde_expands_home() {
        let home = test_home();
        assert_eq!(
            normalize("~/project/file", "/any"),
            format!("{home}/project/file")
        );
    }

    #[test]
    fn normalize_dotdot_collapses() {
        assert_eq!(normalize("/foo/bar/../baz", "/any"), "/foo/baz");
    }

    #[test]
    fn normalize_nested_dotdot_collapses() {
        assert_eq!(normalize("/foo/bar/../../baz", "/any"), "/baz");
    }

    #[test]
    fn normalize_duplicate_slashes() {
        assert_eq!(normalize("/foo//bar///baz", "/any"), "/foo/bar/baz");
    }

    #[test]
    fn normalize_relative_dotdot() {
        assert_eq!(
            normalize("../other/file", "/home/user/project"),
            "/home/user/other/file"
        );
    }

    #[test]
    fn normalize_trailing_slash_removed() {
        assert_eq!(normalize("/path/", "/any"), "/path");
    }

    // ---- expand_pattern tests ----

    #[test]
    fn expand_cwd_variable() {
        assert_eq!(
            expand_pattern("<cwd>/src/**", "/project"),
            "/project/src/**"
        );
    }

    #[test]
    fn expand_home_variable() {
        let home = test_home();
        assert_eq!(
            expand_pattern("<home>/.ssh/**", "/any"),
            format!("{home}/.ssh/**")
        );
    }

    #[test]
    fn expand_tilde_at_start() {
        let home = test_home();
        assert_eq!(expand_pattern("~/.env", "/any"), format!("{home}/.env"));
    }

    #[test]
    fn expand_both_cwd_and_home() {
        let home = test_home();
        assert_eq!(
            expand_pattern("<cwd>/<home>/mixed", "/project"),
            format!("/project/{home}/mixed")
        );
    }

    #[test]
    fn expand_no_variables_unchanged() {
        assert_eq!(
            expand_pattern("/no/variables/**", "/any"),
            "/no/variables/**"
        );
    }

    #[test]
    fn expand_mid_tilde_unchanged() {
        assert_eq!(expand_pattern("mid~string", "/any"), "mid~string");
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
