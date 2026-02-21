use crate::domain::PathError;

/// Expand `~` and `<home>` in a file pattern at load time.
///
/// Called once when a [`crate::config::files::FileRule`] is constructed,
/// so match time never needs to access `$HOME`.
///
/// - `<cwd>` is **not** expanded here — that happens at match time.
/// - Returns `Err(PathError::HomeNotSet)` if the pattern requires `$HOME`
///   but `$HOME` is not set in the environment.
pub(crate) fn expand_home(pattern: &str) -> Result<String, PathError> {
    if !pattern.contains("<home>") && !pattern.starts_with('~') {
        return Ok(pattern.to_string());
    }
    let home = crate::path::home_dir()?;
    let result = pattern.replace("<home>", &home);
    if let Some(rest) = result.strip_prefix('~') {
        Ok(format!("{home}{rest}"))
    } else {
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_home() -> String {
        std::env::var("HOME").unwrap()
    }

    #[test]
    fn no_home_variable_unchanged() {
        assert_eq!(expand_home("/tmp/**").unwrap(), "/tmp/**");
    }

    #[test]
    fn cwd_variable_unchanged() {
        assert_eq!(expand_home("<cwd>/**").unwrap(), "<cwd>/**");
    }

    #[test]
    fn tilde_expanded() {
        let home = test_home();
        assert_eq!(expand_home("~/.ssh/**").unwrap(), format!("{home}/.ssh/**"));
    }

    #[test]
    fn home_variable_expanded() {
        let home = test_home();
        assert_eq!(
            expand_home("<home>/.config/**").unwrap(),
            format!("{home}/.config/**")
        );
    }

    #[test]
    fn cwd_and_home_variable_partial_expand() {
        let home = test_home();
        // <cwd> stays; <home> is expanded
        assert_eq!(
            expand_home("<cwd>/<home>/mixed").unwrap(),
            format!("<cwd>/{home}/mixed")
        );
    }
}
