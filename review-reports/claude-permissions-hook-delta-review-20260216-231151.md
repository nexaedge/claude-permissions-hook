# Code Review Report - claude-permissions-hook

Timestamp: 2026-02-16 23:11:51 -03
Mode: Delta
Scope: Re-review against `/tmp/claude-permissions-hook-delta-review-20260216-230708.md` plus the provided change summary, focusing on wrapper parsing hardening, config auto-discovery docs/behavior, and new regression tests.

## Standards Used (second-brain)
- `/Users/jaisonerick/code/jaisonerick/second-brain/03-resources/programming/standards/rust.md`
- `/Users/jaisonerick/code/jaisonerick/second-brain/03-resources/programming/standards/testing.md`
- `/Users/jaisonerick/code/jaisonerick/second-brain/01-projects/claude-permissions-hook/specs/arch-001-rust-foundation.md`

## Validation Run Results
- `cargo fmt --check`: pass
- `cargo clippy --all-targets -- -D warnings`: pass
- `cargo test`: pass (161 total: 112 unit + 49 integration)

## Prior Findings Status
| Prior finding | Status | Evidence |
|---|---|---|
| Shell wrapper/path bypasses can evade policy checks | Partial | Basename normalization remains correct (`src/config/mod.rs:75`), and wrapper parsing now handles `env -u/--unset/-C/--chdir` and `exec -a` (`src/command/mod.rs:149`). However, `env` option coverage is still incomplete, allowing bypass via other consuming options (`-P`) and `-S` command strings (see High finding). |
| Plugin hook command lacks config path and defaults to ask-everything | Fixed | Runtime auto-discovery is implemented (`src/cli/hook.rs:15`, `src/cli/hook.rs:49`) and README now documents precedence (`README.md:43`). |
| Missing wrapper/path regression tests | Partial | Strong coverage added for path normalization and wrapper cases (`src/decision/tests.rs:378`, `tests/cli_test.rs:287`), including `env -u` and `exec -a` (`tests/cli_test.rs:325`). Coverage still misses `env -P` and `env -S` behaviors. |

## Findings By Severity

### High

1. `env` wrapper parsing still permits deny-list bypass through unhandled/complex option forms
- Why it matters: denied commands can still execute without hook enforcement by using valid `env` syntax, which is a direct policy bypass in security-sensitive command gating.
- Evidence:
  - `env` consuming-option table is incomplete: `-P` is not treated as consuming a following argument (`src/command/mod.rs:151`).
  - `-S/--split-string` is marked as consuming, but the consumed command string is discarded rather than parsed (`src/command/mod.rs:176`), so command content can be hidden from decision logic.
  - Reproductions with config `bash { deny "rm" }` or `bash { deny "echo" }`:
    - `env -P /usr/bin rm -rf /` -> `{}`
    - `env -S "echo hi"` (with `deny "echo"`) -> `{}`
  - `env -S 'echo hi'` is executable behavior on this platform, confirming the command string is semantically active.
- Practical fix recommendation:
  - Extend wrapper-specific grammar for `env` to include all consuming options supported by target platforms (`-P` at minimum for BSD/macOS).
  - Parse `-S/--split-string` payload into words and evaluate its first executable target (or fail closed to `Ask` if parsing is ambiguous).
  - Add regressions in all layers (`src/command/mod.rs`, `src/decision/tests.rs`, `tests/cli_test.rs`) for `env -P ... <prog>` and `env -S "..."` forms.

## New Findings (Delta)
- New high-severity bypass vector identified: `env -P` and `env -S` forms evade current wrapper extraction despite the previous fix set.

## Quick Recommended Next Actions
1. Harden `env` option parsing for platform variants and `-S` command-string semantics; fail closed on ambiguity.
2. Add regression tests for `env -P` and `env -S` at unit, decision, and integration layers.
3. Re-run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` after parser updates.
