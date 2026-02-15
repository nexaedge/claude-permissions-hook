# Contributing to claude-permissions-hook

Thank you for considering contributing! This guide will help you get started.

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [cargo-nextest](https://nexte.st/) for running tests

### Getting Started

```bash
git clone https://github.com/jaisonerick/claude-permissions-hook.git
cd claude-permissions-hook
cargo build
```

### Install cargo-nextest

```bash
cargo install cargo-nextest
```

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

## Commit Messages

Write clear, descriptive commit messages. Focus on the "why" rather than the "what."

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
