# Known Issues

## Command-Wrapper Bypass (Bash Engine)

**Severity:** Medium
**Since:** v0.1.0 (original implementation)
**Tracking:** Requires option-aware argument parsing per launcher

### Description

The bash engine unwraps a hardcoded set of transparent launchers (`command`, `env`, `nohup`, `exec`, `builtin`) to evaluate the underlying program. Launchers not in this list (e.g. `sudo`, `nice`, `time`, `xargs`) are treated as the program name itself — the wrapped target is not inspected.

### Impact

A config with `allow "sudo"` and `deny "rm"` will **allow** `sudo rm -rf /` because the decision is made on `sudo` (allowed), not on `rm` (denied).

### Workaround

Do not rely on `deny` rules to block programs when they may be invoked via an unlisted launcher. Instead:

- Deny the launcher itself: `deny "sudo"`
- Or list all target programs explicitly without relying on wrapper transparency

### Fix

Requires option-aware argument parsing for each launcher to correctly identify the wrapped program. Out of scope for the v0.5.0 architecture refactor.
