use crate::config::rule::RuleConditions;

/// When a rule has both an inline subcommand (from rule string) and children
/// `subcommands` chains, the children are relative to the inline position.
///
/// Prepend the inline subcommand to each children chain, then clear `subcommand`.
/// Example: `"git push" { subcommands "origin" }` → `subcommands [["push","origin"]]`.
pub(crate) fn normalize_subcommand_chains(conditions: &mut RuleConditions) {
    if conditions.subcommand.is_empty() || conditions.subcommands.is_empty() {
        return;
    }
    for chain in &mut conditions.subcommands {
        let mut merged = conditions.subcommand.clone();
        merged.append(chain);
        *chain = merged;
    }
    conditions.subcommand.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_subcommand_chains_empty_subcommand_noop() {
        let mut conditions = RuleConditions::default();
        conditions.subcommands.push(vec!["origin".to_string()]);
        normalize_subcommand_chains(&mut conditions);
        // No inline subcommand → chains unchanged
        assert_eq!(conditions.subcommands, vec![vec!["origin".to_string()]]);
    }

    #[test]
    fn normalize_subcommand_chains_empty_subcommands_noop() {
        let mut conditions = RuleConditions::default();
        conditions.subcommand.push("push".to_string());
        normalize_subcommand_chains(&mut conditions);
        // No children chains → subcommand unchanged
        assert_eq!(conditions.subcommand, vec!["push".to_string()]);
        assert!(conditions.subcommands.is_empty());
    }

    #[test]
    fn normalize_subcommand_chains_prepends_and_clears() {
        let mut conditions = RuleConditions {
            subcommand: vec!["push".to_string()],
            subcommands: vec![vec!["origin".to_string()]],
            ..Default::default()
        };
        normalize_subcommand_chains(&mut conditions);
        assert!(conditions.subcommand.is_empty());
        assert_eq!(
            conditions.subcommands,
            vec![vec!["push".to_string(), "origin".to_string()]]
        );
    }

    #[test]
    fn normalize_subcommand_chains_multiple_chains() {
        let mut conditions = RuleConditions {
            subcommand: vec!["push".to_string()],
            subcommands: vec![vec!["origin".to_string()], vec!["upstream".to_string()]],
            ..Default::default()
        };
        normalize_subcommand_chains(&mut conditions);
        assert!(conditions.subcommand.is_empty());
        assert_eq!(
            conditions.subcommands,
            vec![
                vec!["push".to_string(), "origin".to_string()],
                vec!["push".to_string(), "upstream".to_string()],
            ]
        );
    }
}
