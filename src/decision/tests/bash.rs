use super::{bash_input, make_config, make_input, rules_of};
use crate::config::Config;
use crate::decision::evaluate;
use crate::protocol::output::Decision;
use serde_json::json;

// ---- Test macros ----

/// Bash command with config → expects a specific Decision variant.
macro_rules! bash_decision_test {
    ($name:ident, cmd: $cmd:expr, mode: $mode:expr,
     allow: [$($a:expr),*], deny: [$($d:expr),*], ask: [$($q:expr),*],
     expect: $decision:expr) => {
        #[test]
        fn $name() {
            let config = make_config(&[$($a),*], &[$($d),*], &[$($q),*]);
            let input = bash_input($cmd, $mode);
            let result = evaluate(&input, Some(&config)).unwrap();
            assert_eq!(result.hook_specific_output.permission_decision, $decision);
        }
    };
}

/// Bash command with config → expects None (no opinion).
macro_rules! bash_none_test {
    ($name:ident, cmd: $cmd:expr, mode: $mode:expr,
     allow: [$($a:expr),*], deny: [$($d:expr),*], ask: [$($q:expr),*]) => {
        #[test]
        fn $name() {
            let config = make_config(&[$($a),*], &[$($d),*], &[$($q),*]);
            let input = bash_input($cmd, $mode);
            assert!(evaluate(&input, Some(&config)).is_none());
        }
    };
}

/// Non-Bash tool with config → expects None.
macro_rules! non_bash_none_test {
    ($name:ident, tool: $tool:expr) => {
        #[test]
        fn $name() {
            let config = make_config(&["git"], &["rm"], &[]);
            let input = make_input($tool, "default", json!({}));
            assert!(evaluate(&input, Some(&config)).is_none());
        }
    };
}

// ---- No config ----

#[test]
fn no_config_bash_tool_returns_ask() {
    let input = bash_input("ls", "default");
    let output = evaluate(&input, None).unwrap();
    assert_eq!(
        output.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn no_config_non_bash_tool_returns_ask() {
    let input = make_input("Read", "default", json!({"file_path": "/tmp/x"}));
    let output = evaluate(&input, None).unwrap();
    assert_eq!(
        output.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

// ---- Non-Bash tools with config → None ----

non_bash_none_test!(config_read_returns_none,  tool: "Read");
non_bash_none_test!(config_write_returns_none, tool: "Write");
non_bash_none_test!(config_edit_returns_none,  tool: "Edit");
non_bash_none_test!(config_glob_returns_none,  tool: "Glob");

// ---- Single command evaluation ----

bash_decision_test!(single_allowed_program,
    cmd: "git status", mode: "default",
    allow: ["git"], deny: [], ask: [],
    expect: Decision::Allow);

bash_decision_test!(single_denied_program,
    cmd: "rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(single_ask_program,
    cmd: "docker run ubuntu", mode: "default",
    allow: [], deny: [], ask: ["docker"],
    expect: Decision::Ask);

bash_none_test!(single_unlisted_program,
    cmd: "ls -la", mode: "default",
    allow: ["git"], deny: ["rm"], ask: ["docker"]);

// ---- Multi-command aggregation ----

bash_decision_test!(both_allow,
    cmd: "git add . && git commit", mode: "default",
    allow: ["git"], deny: [], ask: [],
    expect: Decision::Allow);

bash_decision_test!(allow_plus_deny,
    cmd: "git add && rm -rf /", mode: "default",
    allow: ["git"], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(allow_plus_unlisted,
    cmd: "git status && ls", mode: "default",
    allow: ["git"], deny: [], ask: [],
    expect: Decision::Ask);

bash_none_test!(both_unlisted,
    cmd: "foo && bar", mode: "default",
    allow: ["git"], deny: ["rm"], ask: []);

// ---- Permission mode modifiers (full evaluate) ----

bash_decision_test!(bypass_with_ask_returns_allow,
    cmd: "docker run", mode: "bypassPermissions",
    allow: [], deny: [], ask: ["docker"],
    expect: Decision::Allow);

bash_decision_test!(dont_ask_with_ask_returns_deny,
    cmd: "docker run", mode: "dontAsk",
    allow: [], deny: [], ask: ["docker"],
    expect: Decision::Deny);

bash_decision_test!(default_with_ask_returns_ask,
    cmd: "docker run", mode: "default",
    allow: [], deny: [], ask: ["docker"],
    expect: Decision::Ask);

// ---- Fail-closed edge cases ----

#[test]
fn bash_tool_without_command_field_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    let input = make_input("Bash", "default", json!({"description": "something"}));
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn empty_command_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    let input = bash_input("", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn whitespace_command_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    let input = bash_input("   ", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn parse_error_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    let input = bash_input("git add . &&", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

#[test]
fn no_programs_extracted_returns_ask() {
    let config = make_config(&["git"], &[], &[]);
    // Arithmetic expression parses but yields no program segments
    let input = bash_input("(( x + 1 ))", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}

// ---- Path normalization: absolute paths match basename ----

bash_decision_test!(absolute_path_deny,
    cmd: "/bin/rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(absolute_path_allow,
    cmd: "/usr/bin/git status", mode: "default",
    allow: ["git"], deny: [], ask: [],
    expect: Decision::Allow);

// ---- Wrapper unwrapping ----

bash_decision_test!(wrapper_command_deny,
    cmd: "command rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(wrapper_env_deny,
    cmd: "env rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(wrapper_env_with_opts_deny,
    cmd: "env -i FOO=bar rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(wrapper_nohup_deny,
    cmd: "nohup rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(wrapper_nested_deny,
    cmd: "env command rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(wrapper_absolute_env_deny,
    cmd: "/usr/bin/env rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(wrapper_command_allow,
    cmd: "command git status", mode: "default",
    allow: ["git"], deny: [], ask: [],
    expect: Decision::Allow);

bash_decision_test!(wrapper_env_allow,
    cmd: "env git pull", mode: "default",
    allow: ["git"], deny: [], ask: [],
    expect: Decision::Allow);

bash_decision_test!(wrapper_env_u_deny,
    cmd: "env -u PATH rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(wrapper_exec_a_deny,
    cmd: "exec -a fake rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(wrapper_env_p_deny,
    cmd: "env -P /usr/bin rm -rf /", mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

bash_decision_test!(wrapper_env_split_string_deny,
    cmd: r#"env -S "rm -rf /""#, mode: "default",
    allow: [], deny: ["rm"], ask: [],
    expect: Decision::Deny);

// ---- Conditional rule fallthrough through evaluate() ----

/// Build a Config with conditional rules for decision-layer tests.
fn config_with_conditional_rules(
    allow: Vec<crate::config::rule::BashRule>,
    deny: Vec<crate::config::rule::BashRule>,
    ask: Vec<crate::config::rule::BashRule>,
) -> Config {
    Config {
        bash: Some(crate::config::BashConfig { allow, deny, ask }),
        ..Default::default()
    }
}

fn conditional_deny_rule(program: &str, flags: &[&str]) -> crate::config::rule::BashRule {
    crate::config::rule::BashRule {
        program: crate::domain::ProgramName::new(program),
        conditions: crate::config::rule::RuleConditions {
            required_flags: flags.iter().map(|s| crate::domain::Flag::new(s)).collect(),
            ..Default::default()
        },
    }
}

#[test]
fn conditional_deny_miss_falls_through_to_allow() {
    let config = config_with_conditional_rules(
        rules_of(&["rm"]),                                // allow rm unconditionally
        vec![conditional_deny_rule("rm", &["-r", "-f"])], // deny rm only with -r -f
        vec![],
    );
    // rm file.txt — deny condition misses (no -r -f), allow matches → Allow
    let input = bash_input("rm file.txt", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Allow
    );
}

#[test]
fn conditional_deny_miss_falls_through_to_ask() {
    let config = config_with_conditional_rules(
        vec![],
        vec![conditional_deny_rule("rm", &["-r", "-f"])], // deny rm only with -r -f
        rules_of(&["rm"]),                                // ask rm unconditionally
    );
    // rm file.txt — deny condition misses, ask matches → Ask
    let input = bash_input("rm file.txt", "default");
    let result = evaluate(&input, Some(&config)).unwrap();
    assert_eq!(
        result.hook_specific_output.permission_decision,
        Decision::Ask
    );
}
