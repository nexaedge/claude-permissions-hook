# Claude Code Project Instructions

## Project Overview

A Rust binary that implements Claude Code's PreToolUse hook protocol. Receives JSON on stdin, evaluates permission rules, and returns a decision on stdout.

## Developer Commands

| Action | Command |
|--------|---------|
| Build | `cargo build` |
| Build release | `cargo build --release` |
| Run all tests | `cargo nextest run` |
| Run single test | `cargo nextest run test_name` |
| Run doctests | `cargo test --doc` |
| Lint | `cargo clippy --all-targets` |
| Format | `cargo fmt` |
| Format check | `cargo fmt --check` |

## Project Structure

```
src/
├── main.rs          # Entry point (thin — parses CLI, calls lib)
├── lib.rs           # Re-exports all modules
├── protocol/        # Hook protocol types (input/output)
├── decision/        # Permission mode → decision mapping
└── cli/             # clap subcommands
tests/
└── fixtures/        # Real Claude Code JSON payloads
```

## Coding Standards

- **Edition:** Rust 2021, stable toolchain
- **CLI:** clap 4.x with derive macros
- **Errors:** thiserror for library errors
- **Serialization:** serde + serde_json
- **Testing:** cargo-nextest (process-per-test isolation)
- **Linting:** cargo clippy with zero warnings
- **Formatting:** cargo fmt (rustfmt defaults)

## Key Patterns

- `main.rs` is thin — delegates to library code immediately
- Wire format conventions differ by direction:
  - **Input** (`HookInput`): snake_case fields — matches what Claude Code sends
  - **Output** (`HookOutput`, `PreToolUseOutput`): camelCase fields — matches what Claude Code expects
  - **Enums**: `PermissionMode` is camelCase (`bypassPermissions`), `Decision` is lowercase (`allow`)
- Fail-safe defaults: parse errors or unexpected input default to "ask"
- Never `unwrap()` in library code — return `Result` instead

## Public Repository

This is an open-source project. Never include:
- Credentials, API keys, or tokens
- Private file paths or personal data
- Internal hostnames or URLs
