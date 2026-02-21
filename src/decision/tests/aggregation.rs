use crate::decision::aggregation::{aggregate_decisions, apply_mode_modifier};
use crate::protocol::output::Decision;
use crate::protocol::PermissionMode;

/// aggregate_decisions() test case.
macro_rules! aggregate_test {
    ($name:ident, input: [$($val:expr),*], expect: $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(aggregate_decisions(&[$($val),*]), $expected);
        }
    };
}

/// apply_mode_modifier() test case.
macro_rules! mode_modifier_test {
    ($name:ident, decision: $decision:expr, mode: $mode:expr, expect: $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(apply_mode_modifier($decision, &$mode), $expected);
        }
    };
}

// ---- aggregate_decisions() unit tests ----

aggregate_test!(aggregate_empty,             input: [],                                       expect: None);
aggregate_test!(aggregate_all_none,          input: [None, None],                             expect: None);
aggregate_test!(aggregate_allow_and_deny,    input: [Some(Decision::Allow), Some(Decision::Deny)],  expect: Some(Decision::Deny));
aggregate_test!(aggregate_allow_and_none,    input: [Some(Decision::Allow), None],            expect: Some(Decision::Ask));
aggregate_test!(aggregate_deny_and_none,     input: [Some(Decision::Deny), None],             expect: Some(Decision::Deny));

// ---- apply_mode_modifier() unit tests ----

mode_modifier_test!(bypass_allow_stays_allow, decision: Decision::Allow, mode: PermissionMode::BypassPermissions, expect: Decision::Allow);
mode_modifier_test!(bypass_deny_stays_deny,   decision: Decision::Deny,  mode: PermissionMode::BypassPermissions, expect: Decision::Deny);
mode_modifier_test!(dont_ask_allow_stays,     decision: Decision::Allow, mode: PermissionMode::DontAsk,           expect: Decision::Allow);
mode_modifier_test!(dont_ask_deny_stays,      decision: Decision::Deny,  mode: PermissionMode::DontAsk,           expect: Decision::Deny);
