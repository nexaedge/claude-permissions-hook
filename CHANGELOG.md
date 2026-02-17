# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-02-17

### Miscellaneous

- bump version to 0.2.0 ([43f809f](https://github.com/nexaedge/claude-permissions-hook/commit/43f809f670c35a79b67676bc3585eac9e979d2a7))

### Other Changes

- Remove review-reports from git and add to .gitignore ([864210a](https://github.com/nexaedge/claude-permissions-hook/commit/864210a6f5e02c2daeed691cea97db2bc3c0bf9c))
- Close wrapper/path bypass vectors and add config auto-discovery ([f37381f](https://github.com/nexaedge/claude-permissions-hook/commit/f37381f1e5278c0fc9b5798566d7ecd286e1b54e))
- Address review report 5183ca22fdb9 ([a81064a](https://github.com/nexaedge/claude-permissions-hook/commit/a81064a2d9d7249d7ea7850c76230a905b2926ca))
- Expand E2E test coverage with decision matrix and add example config ([cd1de9f](https://github.com/nexaedge/claude-permissions-hook/commit/cd1de9f78474fa7dec71c353851607a5b79e5141))
- Expect snake_case JSON fields from Claude Code hooks protocol ([1dc8a1f](https://github.com/nexaedge/claude-permissions-hook/commit/1dc8a1f0b0a90eecb6ddcceaebd5a78a267b9b8a))
- Improve hook output messages with app name and contextual reasons ([2fe1acf](https://github.com/nexaedge/claude-permissions-hook/commit/2fe1acfc47145597570aa4d200b2d329f9f89f43))
- Harden fail-closed behavior, improve code quality from review cycle ([0add71f](https://github.com/nexaedge/claude-permissions-hook/commit/0add71fa1608fab5d01bf927383f8b1533c2b5af))
- Implement decision matrix with config-based evaluation and macro test patterns ([47f994c](https://github.com/nexaedge/claude-permissions-hook/commit/47f994c075bcc3e6b0abb9bdc5a1ab05027c0192))
- Add command parser module using brush-parser AST ([665d642](https://github.com/nexaedge/claude-permissions-hook/commit/665d6425b8b9516e6389e7a9897266e4512faeb5))
- Add config module with KDL parsing and --config CLI flag ([890ab5d](https://github.com/nexaedge/claude-permissions-hook/commit/890ab5df40fc782e200d9954e35827978b7aaa25))
- Switch npm publishing from token to OIDC trusted publishing ([57c2088](https://github.com/nexaedge/claude-permissions-hook/commit/57c208810ab0e88b8ea57cc2a8b4aba59f10b0b9))
- Fix release workflow: rename binaries to avoid filename conflicts ([064cfcf](https://github.com/nexaedge/claude-permissions-hook/commit/064cfcfbef1933642d8ec4fd7a61dee22b49ff42))

## [0.1.0] - 2026-02-16

### Other Changes

- Fix release workflow: add id-token permission for npm provenance ([d6a3dae](https://github.com/nexaedge/claude-permissions-hook/commit/d6a3dae03222db9f5dc3707b7744c1a5c34c2ab3))
- Update repository URLs from jaisonerick to nexaedge org ([8559972](https://github.com/nexaedge/claude-permissions-hook/commit/8559972ff461a068cad1b0bf2f2a7b582921fa06))
- Fix .gitignore to include base package bin stub ([de59fc5](https://github.com/nexaedge/claude-permissions-hook/commit/de59fc521f5a1473680dec1879d998139312e9b2))
- Add plugin metadata, npm distribution packages, and CI/CD pipelines ([d4ac08f](https://github.com/nexaedge/claude-permissions-hook/commit/d4ac08f88605f598714cfaad65cb0d2f1673294c))
- Add integration test suite with fixture-based CLI testing ([24fe517](https://github.com/nexaedge/claude-permissions-hook/commit/24fe517333420e06d4ee7b18d6d6e16f392b9c19))
- Implement hook subcommand stdin/stdout handler ([4434bcd](https://github.com/nexaedge/claude-permissions-hook/commit/4434bcd00b725e2f9fe1a072c601bc2d5813eb39))
- Implement permission mode decision logic ([b38ac44](https://github.com/nexaedge/claude-permissions-hook/commit/b38ac4417e479e01faf06b77df95036f25bcb4c9))
- Implement hook protocol types with full PreToolUse I/O support ([c05edf6](https://github.com/nexaedge/claude-permissions-hook/commit/c05edf64a5718757f4c4e905f8a945c3ea98ef56))
- Initial project setup: Rust binary with module stubs and open-source scaffolding ([bc923f0](https://github.com/nexaedge/claude-permissions-hook/commit/bc923f096434d8b25afcf1407fd35988a141e040))

