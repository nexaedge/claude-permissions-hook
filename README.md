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
echo '{"sessionId":"s","transcriptPath":"/tmp/t","cwd":"/tmp","permissionMode":"default","hookEventName":"PreToolUse","toolName":"Bash","toolInput":{"command":"git status"},"toolUseId":"t"}' | claude-permissions-hook hook --config my-config.kdl
```

### Permission Modes

The hook respects Claude Code's permission modes. `allow` and `deny` from config are absolute. `ask` is modulated by mode:

| Config Decision | `bypassPermissions` | `dontAsk` | `default` / `plan` / `acceptEdits` |
|---|---|---|---|
| allow | allow | allow | allow |
| deny | deny | deny | deny |
| ask | allow | deny | ask |
| unlisted | — | — | — |

Unlisted programs (not in any config list) return no opinion — Claude handles them natively.

## Configuration

Pass a KDL config file via `--config`:

```bash
claude-permissions-hook hook --config ~/.config/claude-permissions.kdl
```

### Config Format (KDL)

```kdl
bash {
    allow "git" "cargo" "npm" "node" "ls" "cat" "echo"
    deny "rm" "shutdown" "reboot"
    ask "docker" "kubectl" "curl"
}
```

- **allow** — auto-approve these programs
- **deny** — always block these programs
- **ask** — prompt for confirmation (modulated by permission mode)
- **unlisted** — programs not in any list get no opinion from the hook

Lookup precedence: deny > ask > allow.

### Multi-Command Handling

For chained commands (`&&`, `||`, `;`, `|`), the hook evaluates each program and takes the most restrictive decision. If any program is denied, the whole command is denied.

Without `--config`, the hook returns `ask` for everything, prompting you to set up a config file.

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
