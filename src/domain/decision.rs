/// A permission decision: allow, ask, or deny.
///
/// Pure domain type — serialization is handled by the protocol layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

impl Decision {
    /// Explicit severity ranking: Allow(0) < Ask(1) < Deny(2).
    ///
    /// Used by aggregation to select the most restrictive decision.
    /// Explicit mapping prevents accidental breakage from enum reordering.
    pub fn severity(&self) -> u8 {
        match self {
            Decision::Allow => 0,
            Decision::Ask => 1,
            Decision::Deny => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_severity_allow_less_than_ask_less_than_deny() {
        assert!(Decision::Allow.severity() < Decision::Ask.severity());
        assert!(Decision::Ask.severity() < Decision::Deny.severity());
        assert!(Decision::Allow.severity() < Decision::Deny.severity());

        // max_by_key(severity) should return most restrictive
        let decisions = vec![Decision::Allow, Decision::Deny, Decision::Ask];
        assert_eq!(
            decisions.into_iter().max_by_key(|d| d.severity()),
            Some(Decision::Deny)
        );
    }
}
