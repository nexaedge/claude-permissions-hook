use super::rule::bash::BashRule;
use super::rule::files::FileRule;

/// The evaluated permission policy — a flat list of rules per tool category.
///
/// Replaces the old `Config` struct at the domain level. Fields use `Vec`
/// (not `Option<Vec>`) — an absent config section is an empty vec.
#[derive(Debug, Default)]
pub struct Policy {
    pub(crate) bash: Vec<BashRule>,
    pub(crate) files: Vec<FileRule>,
}
