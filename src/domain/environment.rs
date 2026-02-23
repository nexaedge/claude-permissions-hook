use std::path::PathBuf;

/// Runtime environment paths available to the hook.
#[derive(Debug)]
pub struct Environment {
    pub home: PathBuf,
    pub cwd: PathBuf,
}
