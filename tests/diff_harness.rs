// Differential replay harness.
// Reads all tests/golden/*.json files, replays each against the candidate binary,
// and compares canonical JSON output against expected_output.
// Adding new test cases is just adding a JSON file — no Rust code changes needed.

mod common;

use common::{run_hook, run_hook_with_config};
use std::path::PathBuf;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

struct GoldenFile {
    name: String,
    description: String,
    config: Option<String>,
    input: String,
    expected_output: serde_json::Value,
}

fn load_golden_files() -> Vec<GoldenFile> {
    let dir = golden_dir();
    let mut files: Vec<GoldenFile> = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("failed to read golden directory") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("invalid JSON in {}: {e}", path.display()));

        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let description = value["description"]
            .as_str()
            .unwrap_or("(no description)")
            .to_string();
        let config = value["config"].as_str().map(|s| s.to_string());

        // Input can be a JSON object (valid hook input) or a raw string (error case)
        let input = if value["input"].is_string() {
            value["input"].as_str().unwrap().to_string()
        } else {
            serde_json::to_string(&value["input"]).unwrap()
        };

        let expected_output = value["expected_output"].clone();

        files.push(GoldenFile {
            name,
            description,
            config,
            input,
            expected_output,
        });
    }

    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// Canonicalize JSON for comparison: sorted keys, no trailing whitespace.
fn canonical_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap()
}

#[test]
fn golden_replay_all() {
    let files = load_golden_files();
    assert!(
        files.len() >= 15,
        "expected at least 15 golden files, found {}",
        files.len()
    );

    let mut failures: Vec<String> = Vec::new();

    for golden in &files {
        let (stdout, _, exit_code) = if let Some(ref config) = golden.config {
            run_hook_with_config(&golden.input, config)
        } else {
            run_hook(&golden.input)
        };

        if exit_code != 0 {
            failures.push(format!(
                "[{}] exit code {exit_code} (expected 0): {}",
                golden.name, golden.description
            ));
            continue;
        }

        let actual: serde_json::Value = match serde_json::from_str(stdout.trim()) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "[{}] invalid JSON output: {e}\n  stdout: {stdout}",
                    golden.name
                ));
                continue;
            }
        };

        if canonical_json(&actual) != canonical_json(&golden.expected_output) {
            failures.push(format!(
                "[{}] {}\n  expected: {}\n  actual:   {}",
                golden.name,
                golden.description,
                canonical_json(&golden.expected_output),
                canonical_json(&actual)
            ));
        }
    }

    if !failures.is_empty() {
        let msg = format!(
            "{} golden replay failure(s):\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
        panic!("{msg}");
    }
}
