# Golden Corpus

Behavioral snapshots from the v0.4.0 binary. Each `.json` file is a test case:

```json
{
  "description": "Human-readable description",
  "tags": ["bash", "allow"],
  "config": "bash {\n  allow \"ls\"\n}",
  "input": { "session_id": "...", "tool_name": "Bash", ... },
  "expected_output": { "hookSpecificOutput": { ... } }
}
```

## Fields

- **config** — KDL config string (null = no config file)
- **input** — JSON object piped to stdin (or raw string for error cases)
- **expected_output** — exact JSON expected from stdout

## Adding cases

Add a `.json` file matching the schema above. The diff harness (`tests/diff_harness.rs`)
picks up all `*.json` files automatically — no Rust code changes needed.

## Regenerating

If behavior intentionally changes, regenerate with:

```bash
cargo test --test generate_golden -- --ignored generate_golden_corpus
```

Review the diff carefully before committing.
