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

    #[cfg(test)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> String {
        std::env::var("HOME").unwrap()
    }

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
}
