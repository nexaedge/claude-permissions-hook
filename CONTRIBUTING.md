# Contributing to claude-permissions-hook

Thank you for considering contributing! This guide will help you get started.

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [cargo-nextest](https://nexte.st/) for running tests

### Getting Started

```bash
git clone https://github.com/nexaedge/claude-permissions-hook.git
cd claude-permissions-hook
cargo build
```

### Install cargo-nextest

```bash
cargo install cargo-nextest
```

### Enable Commit Message Validation

This project uses a local git hook to validate commit messages:

```bash
git config core.hooksPath .githooks
```

## Commit Messages

This project follows [Conventional Commits](https://www.conventionalcommits.org/). Every commit message must match:

```
type(scope): description
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`, `ci`

**Examples:**

```
feat: add KDL config parser
fix(protocol): handle empty stdin gracefully
docs: update CONTRIBUTING guide
refactor(decision): simplify rule matching logic
chore: update dependencies
ci: split CI into parallel jobs
```

The local git hook (`.githooks/commit-msg`) and CI both enforce this format. Merge commits and `release:` prefixed messages are allowed through automatically.

## Running Tests

```bash
# Run all tests
cargo nextest run

# Run a specific test
cargo nextest run test_name

# Run doctests
cargo test --doc
```

## Code Style

This project uses standard Rust tooling for code quality:

- **Formatting:** `cargo fmt` (rustfmt with default config)
- **Linting:** `cargo clippy --all-targets`

Please ensure your code passes both before submitting a PR:

```bash
cargo fmt --check
cargo clippy --all-targets
```

## Pull Request Process

1. Fork the repository and create a feature branch
2. Write tests for new functionality
3. Ensure all tests pass: `cargo nextest run`
4. Ensure no lint warnings: `cargo clippy --all-targets`
5. Ensure formatting is correct: `cargo fmt --check`
6. Submit a pull request with a clear description of the changes

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
