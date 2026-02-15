# claude-permissions-hook

A permission hook for [Claude Code](https://docs.anthropic.com/en/docs/claude-code) with granular rule-based control. Provides fine-grained, context-aware permission decisions for Claude Code tool calls — allowing safe commands automatically while prompting or blocking risky ones.

## Installation

Install as a Claude Code plugin:

```bash
claude plugin add npm:claude-permissions-hook
```

Or install from npm directly:

```bash
npm install -g claude-permissions-hook
```

## Usage

The hook runs automatically as a Claude Code PreToolUse hook. It reads tool call details from stdin and returns a permission decision (allow, ask, or deny) on stdout.

```bash
# Test the hook manually
echo '{"tool_name":"Bash","tool_input":{"command":"ls"},"permission_mode":"default"}' | claude-permissions-hook hook
```

### Permission Modes

The hook respects Claude Code's permission modes:

| Mode | Behavior |
|------|----------|
| `bypassPermissions` | Allow all tool calls |
| `dontAsk` | Deny all tool calls |
| `default`, `plan`, `acceptEdits` | Ask for confirmation |

## Configuration

Configuration support with YAML-based rules is planned for a future release. Currently, the hook provides basic permission mode handling.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, running tests, and code style guidelines.

### Quick Start

```bash
# Build
cargo build

# Run tests
cargo nextest run

# Lint
cargo clippy --all-targets

# Format
cargo fmt
```

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a pull request.

## License

MIT — see [LICENSE](LICENSE) file.
