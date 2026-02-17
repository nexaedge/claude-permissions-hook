# Code Review Report - claude-permissions-hook

Timestamp: 2026-02-17
Mode: General
Scope: Full repository review (Rust source, tests, docs, plugin manifest/hooks, npm packaging, CI/release workflows)

## Standards Used (second-brain)
- `/Users/jaisonerick/code/jaisonerick/second-brain/03-resources/programming/standards/rust.md`
- `/Users/jaisonerick/code/jaisonerick/second-brain/03-resources/programming/standards/testing.md`
- `/Users/jaisonerick/code/jaisonerick/second-brain/01-projects/claude-permissions-hook/specs/arch-001-rust-foundation.md`

## Validation Run Results
- `cargo fmt --check`: pass
- `cargo clippy --all-targets -- -D warnings`: pass
- `cargo nextest run`: pass (115 passed, 0 failed)
- `cargo test`: pass (115 passed, 0 failed)
- `cargo test --doc`: pass (0 doctests)

## Findings By Severity

### High

1. Shell wrapper/path bypasses allow denied programs to evade policy checks
- Why it matters: The hook can return no-opinion (`{}`) for commands that still execute denied binaries, which weakens the primary safety contract.
- Evidence:
  - Program extraction uses the first simple command token only (`src/command/mod.rs:66`, `src/command/mod.rs:70`, `src/command/mod.rs:72`).
  - Decision lookup is exact string match against configured program names (`src/decision/mod.rs:55`, `src/decision/mod.rs:57`).
  - Example deny rules explicitly include `rm` (`example-config.kdl:17`, `example-config.kdl:18`).
  - Reproduced behavior with config loaded:
    - `command rm -rf /` -> `{}` (no opinion)
    - `/bin/rm -rf /` -> `{}` (no opinion)
    - `env rm -rf /` -> `{}` (no opinion)
- Practical fix recommendation:
  - Canonicalize extracted executable names before lookup (basename normalization for absolute/relative paths).
  - Add wrapper-unwrapping logic for known launcher/builtin wrappers (`command`, `env`, `nohup`, optionally `sudo` policy-aware behavior).
  - Add explicit regression tests in both `src/command/mod.rs` unit tests and `tests/cli_test.rs` integration tests for wrapper/path cases.

### Medium

2. Plugin hook command never supplies config path, so installed plugin defaults to ask-everything behavior
- Why it matters: Default plugin installation appears unable to activate rule-based behavior without manual hook command modification, which conflicts with product expectations and reduces usefulness.
- Evidence:
  - Hook command in plugin config is fixed to `... claude-permissions-hook hook` without `--config` (`hooks/hooks.json:9`).
  - Evaluator behavior with no config is unconditional `Ask` (`src/decision/mod.rs:16`, `src/decision/mod.rs:19`).
  - README positions plugin install as primary path and claims granular rule-based control (`README.md:3`, `README.md:7`, `README.md:21`), but config passing is only documented via CLI argument examples (`README.md:43`, `README.md:46`).
- Practical fix recommendation:
  - Support config discovery via environment variable and/or default filesystem locations (e.g., XDG + home fallback), so plugin command can remain argument-free.
  - Alternatively include a documented plugin-compatible command strategy that injects `--config` reliably.
  - Add integration test coverage for config auto-discovery behavior.

### Low

3. Test suite does not cover known command-wrapper/path normalization risk area
- Why it matters: Security-sensitive parser/evaluator behavior can regress silently without targeted tests.
- Evidence:
  - Current command tests cover chaining, pipes, subshells, env-var prefixes, and function/case constructs (`src/command/mod.rs:145` onward), but no cases for `command <prog>`, `env <prog>`, or absolute-path binaries.
  - CLI integration tests also omit wrapper/path bypass scenarios (`tests/cli_test.rs:1` onward).
- Practical fix recommendation:
  - Add matrix tests for wrapper/path forms against deny/ask/allow lists.
  - Include at least one integration test proving denied command remains denied under wrapper/path variants.

## Quick Recommended Next Actions
1. Patch command extraction + normalization logic to close wrapper/path bypasses.
2. Add config auto-discovery path for plugin runtime and document it in `README.md`.
3. Add regression tests for wrapper/path scenarios at unit and integration layers.
