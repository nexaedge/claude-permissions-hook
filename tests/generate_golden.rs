// Golden corpus generator.
// Run with: cargo test -p claude-permissions-hook --test generate_golden -- --ignored generate_golden_corpus
// Generates tests/golden/*.json from current binary behavior.

mod common;

use common::{bash_input_json, make_input_json, run_hook, run_hook_with_config};
use std::path::PathBuf;

struct GoldenCase {
    name: &'static str,
    description: &'static str,
    tags: Vec<&'static str>,
    config: Option<&'static str>,
    input: String,
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn generate_case(case: &GoldenCase) {
    let (stdout, _, exit_code) = if let Some(config) = case.config {
        run_hook_with_config(&case.input, config)
    } else {
        run_hook(&case.input)
    };
    assert_eq!(exit_code, 0, "exit code must be 0 for case: {}", case.name);

    let expected_output: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output must be valid JSON");
    let input_value: serde_json::Value =
        serde_json::from_str(&case.input).unwrap_or(serde_json::Value::String(case.input.clone()));

    let golden = serde_json::json!({
        "description": case.description,
        "tags": case.tags,
        "config": case.config,
        "input": input_value,
        "expected_output": expected_output
    });

    let path = golden_dir().join(format!("{}.json", case.name));
    let formatted = serde_json::to_string_pretty(&golden).unwrap();
    std::fs::write(&path, format!("{formatted}\n")).unwrap();
    eprintln!("  wrote {}", path.display());
}

const BASH_CONFIG: &str = r#"bash { allow "git"; deny "rm"; ask "docker"; }"#;

const FILE_CONFIG: &str = r#"
bash { allow "git"; deny "rm"; ask "docker"; }
files {
    deny "~/.ssh/**" "read" "write" "edit"
    "<cwd>/**" {
        allow "read" "glob" "grep"
        ask "write" "edit"
    }
}
"#;

#[test]
#[ignore]
fn generate_golden_corpus() {
    let dir = golden_dir();
    std::fs::create_dir_all(&dir).unwrap();
    eprintln!("Generating golden corpus in {}", dir.display());

    let cases = vec![
        // Bash decisions
        GoldenCase {
            name: "bash-allow-simple",
            description: "Bash allow: git status matches allow rule",
            tags: vec!["bash", "allow"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("git status", "default"),
        },
        GoldenCase {
            name: "bash-deny-simple",
            description: "Bash deny: rm -rf / matches deny rule",
            tags: vec!["bash", "deny"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("rm -rf /", "default"),
        },
        GoldenCase {
            name: "bash-ask-simple",
            description: "Bash ask: docker run matches ask rule",
            tags: vec!["bash", "ask"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("docker run", "default"),
        },
        GoldenCase {
            name: "bash-unlisted",
            description: "Bash unlisted: python not in config returns empty JSON",
            tags: vec!["bash", "unlisted"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("python script.py", "default"),
        },
        GoldenCase {
            name: "bash-no-config",
            description: "Bash without config: ask (no config = fail-closed)",
            tags: vec!["bash", "no-config"],
            config: None,
            input: bash_input_json("ls -la", "default"),
        },
        // Multi-command
        GoldenCase {
            name: "bash-multi-chain-deny",
            description: "Multi-command chain: deny wins over allow",
            tags: vec!["bash", "multi-command", "deny"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("git status && rm -rf /", "default"),
        },
        GoldenCase {
            name: "bash-multi-pipe-deny",
            description: "Multi-command pipe: deny wins over allow",
            tags: vec!["bash", "multi-command", "deny"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("git log | rm -rf /", "default"),
        },
        // Error cases
        GoldenCase {
            name: "bash-fail-closed-parse-error",
            description: "Bash parse error: incomplete command fails closed to ask",
            tags: vec!["bash", "error", "fail-closed"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("git add . &&", "default"),
        },
        GoldenCase {
            name: "bash-empty-command",
            description: "Bash empty command: fails closed to ask",
            tags: vec!["bash", "error", "fail-closed"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("", "default"),
        },
        GoldenCase {
            name: "malformed-input",
            description: "Malformed JSON input: fails closed to ask",
            tags: vec!["error", "fail-closed"],
            config: None,
            input: "this is not json at all".to_string(),
        },
        GoldenCase {
            name: "empty-stdin",
            description: "Empty stdin: fails closed to ask",
            tags: vec!["error", "fail-closed"],
            config: None,
            input: String::new(),
        },
        GoldenCase {
            name: "missing-tool-input",
            description: "Bash with missing command field: fails closed to ask",
            tags: vec!["bash", "error", "fail-closed"],
            config: Some(BASH_CONFIG),
            input: make_input_json(
                "Bash",
                "default",
                serde_json::json!({"not_command": "value"}),
            ),
        },
        GoldenCase {
            name: "config-parse-error",
            description: "Invalid KDL config: fails closed to ask",
            tags: vec!["error", "fail-closed", "config"],
            config: Some("invalid {{ kdl {{ syntax"),
            input: bash_input_json("git status", "default"),
        },
        // Mode modifiers
        GoldenCase {
            name: "mode-bypass",
            description: "bypassPermissions converts ask to allow",
            tags: vec!["bash", "mode", "bypass"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("docker run", "bypassPermissions"),
        },
        GoldenCase {
            name: "mode-dontask",
            description: "dontAsk converts ask to deny",
            tags: vec!["bash", "mode", "dontask"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("docker run", "dontAsk"),
        },
        // File tools
        GoldenCase {
            name: "file-read-allow",
            description: "Read file in CWD: allow by file config",
            tags: vec!["file", "read", "allow"],
            config: Some(FILE_CONFIG),
            input: make_input_json(
                "Read",
                "default",
                serde_json::json!({"file_path": "/tmp/test/src/main.rs"}),
            ),
        },
        GoldenCase {
            name: "file-read-deny",
            description: "Read ~/.ssh: deny by file config",
            tags: vec!["file", "read", "deny"],
            config: Some(FILE_CONFIG),
            input: make_input_json(
                "Read",
                "default",
                serde_json::json!({"file_path": "~/.ssh/id_rsa"}),
            ),
        },
        GoldenCase {
            name: "file-write-ask",
            description: "Write in CWD: ask by file config tier ordering",
            tags: vec!["file", "write", "ask"],
            config: Some(FILE_CONFIG),
            input: make_input_json(
                "Write",
                "default",
                serde_json::json!({"file_path": "/tmp/test/new.rs", "content": "data"}),
            ),
        },
        GoldenCase {
            name: "non-bash-with-bash-config",
            description: "Non-bash tool with bash-only config: no opinion (empty JSON)",
            tags: vec!["file", "unlisted", "routing"],
            config: Some(BASH_CONFIG),
            input: make_input_json(
                "Read",
                "default",
                serde_json::json!({"file_path": "/tmp/x"}),
            ),
        },
        // Wrappers
        GoldenCase {
            name: "bash-wrapper-env-deny",
            description: "env wrapper: env rm -rf / still denied",
            tags: vec!["bash", "wrapper", "deny"],
            config: Some(BASH_CONFIG),
            input: bash_input_json("env rm -rf /", "default"),
        },
    ];

    for case in &cases {
        generate_case(case);
    }

    eprintln!("Generated {} golden files", cases.len());
}
