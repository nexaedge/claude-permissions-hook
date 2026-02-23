# Architecture Principles

This document defines the architectural principles, module boundaries, and design
rules for this project. It is the rulebook — treat violations as bugs.

**Document precedence:** This document is the highest-authority reference for the
project. It takes precedence over `CLAUDE.md`, code comments, and any other
documentation. When there is a conflict, this document wins. `CLAUDE.md` covers
developer workflow (commands, formatting, tooling); this document covers architecture,
module boundaries, and design rules.

---

## Philosophy: Rust Is Not Java

Clean architecture, hexagonal architecture, and ports/adapters were popularized in
Java and Node ecosystems where dependency inversion requires interfaces, DI
containers, and runtime polymorphism. Rust has a fundamentally different type system
and ownership model. We take the *principles* of clean architecture — dependency
direction, separation of concerns, boundary isolation — without importing the
*ceremony*.

**What we adopt:**

- Dependencies point inward (adapters depend on domain, never the reverse)
- Each module has a single reason to change
- External formats are translated at the boundary, not leaked inward
- The core logic is pure: no I/O, no serialization, no framework types, no env vars

**What we reject:**

- Trait-based ports/interfaces when there is only one implementation
- Abstract factory patterns or dependency injection containers
- Separate "port" and "adapter" modules when a single module is clear enough
- Type aliases pretending to be abstractions (`type Repository = ...`)

**The Rust-idiomatic replacements:**

- **Enums over trait objects** for closed, known type sets (Decision, PermissionMode,
  FileOperation). The compiler enforces exhaustive handling.
- **Newtypes over validation functions** (ProgramName, Flag, FileTarget). Once
  constructed, always valid. Parse, don't validate.
- **Module visibility over interfaces**. `pub(crate)` and module structure enforce
  boundaries at compile time without traits.
- **Concrete types until proven otherwise**. Extract a trait only when a second
  implementation exists (e.g., a real need for mocking or an alternative backend).

---

## System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                          Claude Code                            │
│                    (external — sends JSON hook)                  │
└────────────────────────────┬────────────────────────────────────┘
                             │ JSON stdin / JSON stdout
┌────────────────────────────▼────────────────────────────────────┐
│                           CLI                                   │
│            (composition root — wires everything)                 │
│                                                                 │
│  ┌───────────────┐   ┌──────────────┐   ┌──────────────────┐   │
│  │ Hook Adapter   │   │  Config      │   │ Decision Engine  │   │
│  │ (JSON wire)    │   │ (KDL files)  │   │ (match + eval)   │   │
│  └───────┬───────┘   └──────┬───────┘   └────────┬─────────┘   │
│          │                  │                     │             │
│          │    ┌─────────────┼─────────────────────┤             │
│          │    │             │                     │             │
│          ▼    ▼             ▼                     ▼             │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    Shell Parser                          │   │
│  │                    (brush AST)                           │   │
│  └────────────────────────┬────────────────────────────────┘   │
│                           │                                     │
│  ┌────────────────────────▼────────────────────────────────┐   │
│  │                       Domain                             │   │
│  │    (shared vocabulary — data types with invariants)      │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

**CLI is the outermost layer.** It is the only piece that touches the external world
(stdin, stdout, filesystem, environment variables). It imports from every other module
and orchestrates the full request/response cycle. No inner module calls CLI — data
flows through CLI's control.

**All modules are side-effect free** (except CLI). No module reads environment
variables, touches the filesystem, or performs I/O. CLI resolves all environment
information (home directory, cwd, config paths) and passes it through as data via
the `Environment` context struct.

---

## Data Flow

The data flows through the system in a single pipeline, orchestrated by CLI:

```
                        ┌───────────┐
                        │Claude Code│
                        └─────┬─────┘
                              │
                         JSON stdin
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  CLI                                                        │
│                                                             │
│  0. Resolve environment ──► Environment { home, cwd, ... }  │
│                                                             │
│  1. Read stdin ──────► json_string                           │
│  2. Read config file ──► config_content (string)             │
│                                                             │
│  3a. Parse input ────────────► hook_adapter                  │
│                                parse_request(json, env)      │
│                                      │                      │
│                                      ▼                      │
│                               Result<PermissionRequest,     │
│                                      HookParseError>        │
│                                                             │
│  3b. Parse config ───────────► config                        │
│                                parse_policy(content, env)    │
│                                      │                      │
│                                      ▼                      │
│                                Result<Policy, ConfigError>   │
│                                                             │
│                  Steps 3a and 3b are independent —           │
│                  neither depends on the other's result.      │
│                                                             │
│  4. Evaluate (if both Ok) ──► decision_engine                │
│     PermissionRequest          evaluate(&request, &policy)  │
│     + Policy                         │                      │
│                                      ▼                      │
│                            Option<PermissionDecision>        │
│                                      │                      │
│  5. Format output ──────────► hook_adapter                   │
│     (all paths converge)       format_response(result)      │
│                                      │                      │
│                                      ▼                      │
│                                 JSON string                  │
│                                 (or "{}" for None)           │
│                                      │                      │
│  6. Write stdout ◄──────────────────┘                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
                              │
                         JSON stdout
                              │
                              ▼
                        ┌───────────┐
                        │Claude Code│
                        └───────────┘
```

Key observations:

- **Hook Adapter exposes facade functions, not internal types.** CLI calls
  `hook_adapter::parse_request()` and `hook_adapter::format_response()`. The internal
  types `HookInput`, `HookOutput`, `ToolUse`, etc. are never exposed outside the
  module. CLI works exclusively with domain types.
- **Hook Adapter does not call Decision Engine.** CLI calls Hook Adapter to parse
  input, then separately calls Decision Engine with that request plus the Policy from
  Config. CLI calls Hook Adapter again to format the result. Hook Adapter and Decision
  Engine never communicate directly.
- **Config does not read files.** CLI reads the config file from disk, then passes
  the string content to `config::parse_policy()`. Config is a pure string-to-domain
  translator.
- **Input parsing and config parsing are independent.** Neither depends on the other's
  result. CLI performs both, then combines the results to determine the next step.
- **Environment is resolved once by CLI.** CLI reads `HOME`, `CWD`, and other env
  vars, bundles them into an `Environment` struct, and passes it down to any module
  that needs external context.

### Step-by-step

1. **CLI resolves environment** → `Environment { home, cwd, ... }`
2. **CLI reads stdin** → raw JSON string
3. **CLI discovers and reads config file** → config content string (or file-level
   error: not found, not readable)
4. **CLI calls `config::parse_policy(content, &env)`** → `Result<Policy, ConfigError>`
   (skipped if step 3 failed — CLI already has the file-level error)
5. **CLI calls `hook_adapter::parse_request(json, &env)`** →
   `Result<PermissionRequest, HookParseError>`

   Steps 4 and 5 are independent — they can execute in either order or concurrently.
   The policy is available before the parse result is needed.

6. **CLI combines results:**
   - Both `Ok` → call `decision_engine::evaluate(&request, &policy)` → step 7
   - Config error (step 3 or 4 failed) → `Some(Ask)` with config error message → step 7
   - Parse `Err(ToolError(UnknownTool))` → `None` → step 7
   - Parse `Err(ToolError(InvalidInput { policy_set, .. }))` → `Some(Ask)` if policy
     has rules for that set, `None` otherwise → step 7
   - Parse `Err` (other — malformed JSON, missing fields) → `Some(Ask)` → step 7
7. **CLI calls `hook_adapter::format_response(result)`** → JSON string (or `"{}"` for `None`)
8. **CLI writes stdout** → JSON back to Claude Code

**Critical: policy is always available for error handling.** Because config loading
(step 4) is independent of input parsing (step 5), the `InvalidInput` error path
can check `policy.bash.is_empty()` or `policy.files.is_empty()` to decide between
`Some(Ask)` and `None`. There is no chicken-and-egg problem.

All output — happy path and error path — goes through `hook_adapter::format_response()`.
CLI never writes raw JSON directly.

---

## Module Map

```
src/
  main.rs                  Entry point (thin, delegates to lib)
  lib.rs                   Crate root, re-exports public API
  error.rs                 Cross-module error types (HookParseError, ConfigError, etc.)

  domain/                  Shared vocabulary — data types with invariants
  config/                  KDL adapter — translates config strings into Policy
  hook_adapter/            Claude Code adapter — JSON wire format ↔ domain types
  decision_engine/         Core logic — matches rules and evaluates decisions
    matcher/               Does a rule match the input?
    evaluator/             What decision does a matched rule produce?
  shell_parser/            Shell parsing utility — bash strings to CommandSegment
  cli/                     Controller — wires modules together, handles I/O
```

---

## Domain Entities

The `domain/` module defines the shared vocabulary — the types that every other
module uses to communicate. Domain types hold data, enforce construction invariants,
and may expose invariant-preserving methods (normalization, canonical comparison).
They do **not** contain matching logic, evaluation logic, or glob processing.

Entities are organized into four logical groups.

### Group 1: Core Decision Types

These are the top-level concepts the system is built around.

```
┌──────────────────────────────────────────┐
│  PermissionDecision (struct)              │
│                                          │
│  ├── decision: Decision (Allow/Ask/Deny) │
│  └── reason: String                      │
│                                          │
│  The composite result of evaluation.     │
│  Wrapped in Option:                      │
│    None = no opinion (deliberate)        │
│    Some = we have a verdict              │
└──────────────────────────────────────────┘
           │
           │ contains
           ▼
┌───────────────────────┐
│  Decision (enum)       │   The raw verdict: Allow, Ask, Deny.
│                       │   Has severity() for ranking.
│                       │   Deny > Ask > Allow
└───────────────────────┘

┌───────────────────────┐
│  PermissionMode        │   How Claude Code is running:
│  (enum)               │   Default, Plan, AcceptEdits,
│                       │   DontAsk, BypassPermissions
└───────────────────────┘
           │
           │ modulates Decision
           │ (e.g. DontAsk turns Ask → Deny)
           ▼
┌───────────────────────┐
│  PolicySet (enum)      │   Bash or File.
│                       │   Selects which field of
│                       │   Policy to evaluate against.
└───────────────────────┘
```

- **`PermissionDecision`** — The composite result: a `Decision` plus a human-readable
  `reason` string. The entire evaluate pipeline returns `Option<PermissionDecision>`:
  `None` means "no opinion" — a deliberate, valid result indicating this hook has
  nothing to say about the request, and the caller (Claude Code) should decide.
  `Some` means "we have a verdict" — the Hook Adapter emits the JSON response.
- **`Decision`** — The raw verdict enum: `Allow`, `Ask`, `Deny`. Ordered by severity
  for aggregation. This is the primitive that rules carry and the engine aggregates.
- **`PermissionMode`** — How Claude Code is currently running. Affects the final
  decision: `BypassPermissions` escalates `Ask` to `Allow`, `DontAsk` downgrades
  `Ask` to `Deny`. `Default`, `Plan`, and `AcceptEdits` leave `Ask` unchanged.
- **`PolicySet`** — `Bash` or `File`. Determines which field of the `Policy` struct
  is relevant for evaluation. When a tool input fails to parse but we know the policy
  set, and config has rules for that set, we fail closed (Ask).

#### "No opinion" vs "Ask"

`None` (no opinion) and `Some(Ask)` are different outcomes:

- **`None`** — We have no rules governing this request. We explicitly choose to say
  nothing, delegating the decision entirely to Claude Code. This is correct when there
  is no config section for the tool type, or the tool is unknown.
- **`Some(Ask)`** — We have rules, and the result of evaluation is to ask the user.
  This is a positive assertion: "we evaluated this and the user should confirm."

### Group 2: Request Types (What Is Being Evaluated)

These represent what Claude Code wants to do. Built by Hook Adapter from JSON input.

```
┌─────────────────────────────────────┐
│  PermissionRequest (struct)          │
│                                     │
│  ├── tool: ToolRequest              │──── What tool is being invoked?
│  ├── cwd: PathBuf                   │──── Working directory
│  ├── mode: PermissionMode           │──── Current permission mode
│  ├── session_id: String             │──── Claude Code session identifier
│  ├── project_path: PathBuf          │──── Project root path
│  └── ... (other hook context)       │──── Mapped from Claude hook params
│                                     │     to stable internal names
└─────────────────────────────────────┘

Claude Code hook parameters are mapped to stable internal field names at the
Hook Adapter boundary. If Claude renames a wire field in the future, only the
adapter changes — PermissionRequest field names are long-lived.
```

```
┌────────────────────────────────────┐
│  ToolRequest (enum)                │
│                                    │
│  ├── Bash {                        │   Shell commands to execute
│  │     segments:                   │   (one per pipeline stage)
│  │       Vec<CommandSegment>       │
│  │   }                             │
│  │                                 │
│  └── File {                        │   File system operation
│       operation: FileOperation,    │   (read, write, edit, glob, grep)
│       targets: Vec<FileTarget>     │   Paths with evaluation context
│     }                              │
└────────────────────────────────────┘

ToolRequest only contains valid, evaluatable requests. Unknown tools and
tool parse errors are NOT variants — they are Err values returned by
hook_adapter::parse_request(). CLI handles them directly:

┌────────────────────────────────────────┐
│  ToolParseError (enum)                  │
│                                        │
│  ├── UnknownTool { tool_name: String } │   Unrecognized tool → no opinion
│  │                                     │
│  └── InvalidInput {                    │   Known policy set but bad input
│       policy_set: PolicySet,           │   → fail closed (Ask) or no opinion
│       reason: String                   │
│     }                                  │
└────────────────────────────────────────┘
```

- **`PermissionRequest`** — The full evaluation context. Contains a successfully
  parsed tool request, working directory, permission mode, session ID, project path,
  and other hook context fields. All Claude hook parameters are mapped to stable
  internal names at the adapter boundary, so these field names survive upstream
  protocol changes. Only exists when `hook_adapter::parse_request()` returns `Ok`.
- **`ToolRequest`** — What Claude Code is asking to do. Only valid, evaluatable tool
  invocations: bash commands or file operations. Unknown tools and parse errors are
  not variants — they are `Err(ToolParseError)` from the Hook Adapter, handled by
  CLI before the decision engine is ever called.
- **`ToolParseError`** — Returned by the Hook Adapter when it cannot produce a valid
  `ToolRequest`. `UnknownTool` means the tool name is unrecognized (CLI returns no
  opinion). `InvalidInput` means the tool is known but its input is malformed (CLI
  returns Ask if the policy set has rules, or no opinion otherwise). See Behavior
  Matrix for the full mapping.
- **`CommandSegment`** — A single program invocation within a shell command. Contains
  a `ProgramName` and its arguments (flags, subcommands, positionals).
- **`FileOperation`** — `Read`, `Write`, `Edit`, `Glob`, `Grep`. Parsed from the tool
  name at the adapter boundary.

### Group 3: Value Objects (Parsed, Validated Primitives)

Newtypes that enforce invariants at construction. Once created, always valid.

```
┌──────────────┐   ┌──────────────┐
│ ProgramName  │   │ Flag         │
│              │   │              │
│ Always a     │   │ Always has   │
│ normalized   │   │ leading      │
│ basename     │   │ dashes       │
│ (non-empty)  │   │ ("-r", etc.) │
└──────────────┘   └──────────────┘

┌──────────────────────────────────────────┐
│  FileTarget (struct)                      │
│                                          │
│  ├── raw_path: String                    │──── Original path (for display/reasons)
│  ├── normalized_path: PathBuf            │──── Absolute, cleaned (for matching)
│  ├── cwd: PathBuf                        │──── Working directory (for <cwd>)
│  ├── project_path: PathBuf               │──── Project root
│  └── ... (other evaluation context)      │
│                                          │
│  The subject of file rule evaluation.    │
│  Built by hook_adapter, stored directly  │
│  in ToolRequest::File.                   │
└──────────────────────────────────────────┘

┌──────────────────────────────────────────┐
│  Environment (struct)                     │
│                                          │
│  ├── home: PathBuf                       │
│  ├── cwd: PathBuf                        │
│  └── ... (other resolved env values)     │
│                                          │
│  Resolved once by CLI, passed to modules │
│  that need external context.             │
└──────────────────────────────────────────┘
```

- **`ProgramName`** — Basename-normalized executable name. `/usr/bin/git` → `git`.
  Non-empty invariant enforced at construction.
- **`Flag`** — A CLI flag with leading dashes. Combined short flags are expanded:
  `-rf` → `["-r", "-f"]`. Always has the dash prefix.
- **`FileTarget`** — The subject of file rule evaluation. A self-contained struct
  that bundles the file path (raw for display, normalized for matching) with the
  evaluation context needed by the matcher and evaluator: working directory (for
  `<cwd>` placeholder expansion), project path, and any other context from the hook
  input that file rules may need. Built by the Hook Adapter at parse time — the
  adapter has all the context (cwd, project path, etc.) from the same JSON input.
  Stored directly in `ToolRequest::File`.
- **`Environment`** — Resolved environment context. CLI reads `HOME`, `CWD`, and other
  env vars once, bundles them into this struct, and passes it to any module function
  that needs external context. Inner modules never read env vars directly.

### Group 4: Policy Types (Rules From Config)

These represent the user's permission rules. Built by Config from KDL strings.
Policy types describe what a rule looks like — they do not contain matching or
evaluation logic.

```
┌──────────────────────────────────────┐
│  Policy (struct)                      │
│                                       │
│  ├── bash: Vec<BashRule>              │
│  └── files: Vec<FileRule>             │
└──────────────────────────────────────┘

Vec semantics:
  empty         → no rules for this policy set (no opinion)
  non-empty     → rules exist; anything not matched → Ask
```

```
           ┌──────────────────────────┐
           │                          │
           ▼                          ▼
┌────────────────────┐    ┌─────────────────────┐
│  BashRule           │    │  FileRule            │
│                    │    │                     │
│  decision          │    │  decision           │
│  program           │    │  path_pattern       │
│  conditions:       │    │  operations (set)   │
│    required_flags  │    │                     │
│    optional_flags  │    │                     │
│    subcommands     │    │                     │
│    positionals     │    │                     │
│    arguments       │    │                     │
└────────────────────┘    └─────────────────────┘
           │                          │
           ▼                          ▼
┌────────────────────┐    ┌─────────────────────┐
│  BashConditions     │    │  PathPattern        │
│                    │    │                     │
│  Filters beyond    │    │  raw: String        │
│  program match:    │    │  (NOT PathBuf —     │
│  flags, args,      │    │  globs aren't paths)│
│  subcommands       │    │  Supports <cwd>     │
└────────────────────┘    └─────────────────────┘
```

- **`Policy`** — The complete set of user-defined rules. An empty `Vec` means the
  user chose not to define rules for this policy set — the result is `None` (no
  opinion). This is distinct from "no config file found" (which is an error handled
  by CLI — see Behavior Matrix). A non-empty `Vec` means rules exist; anything not
  matched defaults to `Ask`. Using `Vec` instead of `Option<Vec>` avoids the
  unrepresentable `Some(vec![])` state — emptiness is just `.is_empty()`.
- **`BashRule`** — Describes a rule for a program: name, optional conditions, and a
  `Decision`. Pure data container — matching logic lives in `decision_engine::matcher`.
- **`FileRule`** — Describes a rule for file paths: a glob pattern, optional operation
  filter, and a `Decision`. Pure data container — matching and glob expansion live in
  `decision_engine::matcher`.
- **`PathPattern`** — A glob pattern for file paths. Stores the raw pattern text as a
  `String` — **not `PathBuf`**, since glob patterns (e.g., `<cwd>/**/*.rs`) are not
  filesystem paths. Supports `<cwd>` as a dynamic placeholder — but the actual
  expansion, compilation, and matching against real `PathBuf` values happen in the
  matcher, not here.
- **`BashConditions`** — The constraint data on a BashRule beyond program name:
  required flags, optional flags, subcommand chains, positional patterns, and required
  argument key-value pairs. Pure data — the matcher interprets these.

---

## Module Responsibilities

### `domain/` — The Shared Vocabulary

**Purpose:** Define the types that other modules use to communicate. This is the
shared language of the system — nothing more.

**Rules:**

1. **No I/O.** No file reads, no stdin, no network, no environment variables.
2. **No serialization annotations.** No `#[derive(Serialize, Deserialize)]`. If a
   type needs serialization, that happens in the adapter that owns the wire format.
   (See "Serde and Domain Types" below for rationale.)
3. **No dependencies on sibling modules.** Domain never imports from `config`,
   `hook_adapter`, `decision_engine`, or `cli`.
4. **No heavy dependencies.** Domain depends only on `std` and lightweight utility
   crates (`thiserror`). Glob matching (`globset`) belongs in the decision engine's
   matcher, not in domain types.
5. **Use `Path`/`PathBuf` for all paths.** Never use `&str` or `String` for file
   system paths. Rust's path types handle OS-specific separators and encoding. The
   only exception is display strings (e.g., `FileTarget::raw_path`) used in reason
   messages, which are `String` because they represent the user-provided form, not a
   filesystem path.
6. **Construction, accessors, and invariant-preserving methods.** `impl` blocks on
   domain types may contain:
   - Construction/parsing (`ProgramName::new()`, `Flag::from()`)
   - Simple accessors (`Decision::severity()`, `PathPattern::raw()`)
   - Invariant-preserving transformations (normalization, canonical comparison)
   - Display implementations
   What they must NOT contain: matching logic (does this rule match that input?),
   collection-level operations (iterate over rules), or orchestration logic (sequence
   of evaluation steps). Those belong in `decision_engine`.
7. **No matching or evaluation logic.** `matches()`, `lookup()`, `evaluate()` — these
   all belong in `decision_engine`. Domain types describe rules; the engine decides
   whether they match.
8. **No application orchestration.** The sequence of checking, aggregating, and
   applying modifiers is engine logic, not type logic.

**Litmus test:** Can the method be implemented purely from `&self` and its direct
fields, and does it preserve the type's invariants? If yes, it can live in domain.
If it needs a glob matcher, a collection of rules, or external context — it belongs
in `decision_engine`.

### `config/` — The Configuration Adapter

**Purpose:** Translate KDL configuration text into `Policy`. This module
owns everything about KDL — parsing, validation, error reporting, document structure.
No other module touches KDL.

The public interface is a function, not a struct: given the config file's **string
content** and an `Environment`, return a `Policy`. The config module never reads
files — CLI reads the file from disk and passes the string content. There is no
`Config` struct that leaks outside — the module's internal types (`ConfigDocument`,
`ParseNode`, `ConfigNode`) are implementation details.

**Contains:**

- `parse_policy(content, &env)` function: KDL string + environment → `Policy`
- KDL document abstraction (internal)
- Parsing logic: KDL nodes into `BashRule`, `FileRule`
- Normalization helpers: home expansion (using `env.home`), flag normalization
- Missing or empty sections produce an empty `Vec` in the resulting `Policy`

**Does NOT contain:** File I/O. CLI discovers the config path, reads the file, and
passes the string content. This keeps the config module pure — it is a string-to-
domain translator, nothing more.

**Internal structure:** KDL-specific code is isolated within the module so that
adding a second config format (TOML, YAML) would mean adding a sibling parser, not
touching the KDL code. The module is easy to read because KDL concerns don't bleed
into the public interface.

**Rules:**

1. **Depends on `domain` and `shell_parser`.** Config reads domain types (BashRule,
   FileRule, etc.) and produces instances of them. It uses `shell_parser` to parse
   inline command strings in config rules. It never imports from `hook_adapter`,
   `decision_engine`, or `cli`.
2. **Does not export its own types for external consumption.** The output is a
   `Policy` (domain type). No config-internal type leaks outside.
3. **KDL is fully contained.** If we switch config formats, only this module changes.
   The rest of the system sees the same `Policy`.
4. **No I/O.** Does not read files or environment variables. Receives string content
   and `Environment` as parameters.

### `hook_adapter/` — The Claude Code Adapter

**Purpose:** Translate between Claude Code's JSON hook wire format and domain types.
This module owns the contract with Claude Code — field names, casing conventions,
envelope structure. No other module knows about Claude Code's protocol.

The name `hook_adapter` reflects its role: it adapts between the external hook
interface and the library's internal domain. If Claude Code changes field names,
casing, or envelope structure, only this module changes.

**Public interface — facade functions only:**

```rust
/// Parse raw JSON into a PermissionRequest. All wire format details are internal.
pub fn parse_request(json: &str, env: &Environment) -> Result<PermissionRequest, ...>

/// Format a decision result into JSON for Claude Code. None → "{}".
pub fn format_response(result: Option<&PermissionDecision>) -> String
```

No internal types (`HookInput`, `HookOutput`, `ToolUse`, `BashToolUse`, `FileToolUse`)
are exposed. CLI interacts with this module exclusively through these two functions
and receives/provides only domain types.

**Contains (all internal):**

- `HookInput`: deserialized from JSON, converted to `PermissionRequest`
- `HookOutput`: constructed from `Option<PermissionDecision>`, serialized to JSON
- Tool parsing: raw tool input → `ToolRequest` variants
- Wire field mapping: Claude hook param names → stable domain field names

**Important:** Hook Adapter is a pure translator. It does not call Decision Engine or
Config. It does not read environment variables — CLI passes the `Environment` struct
as a parameter.

**Rules:**

1. **Depends on `domain` and `shell_parser`.** Reads and produces domain types. Uses
   `shell_parser` to convert raw command strings into `CommandSegment`. Never imports
   from `config`, `decision_engine`, or `cli`.
2. **Does not export internal types.** The wire types (`HookInput`, `HookOutput`,
   `ToolUse`, etc.) are `pub(crate)` or private. Other modules work exclusively with
   domain types through the facade functions.
3. **Serialization annotations live here, not in domain.** `#[derive(Deserialize)]`
   on `HookInput`, `#[derive(Serialize)]` on `HookOutput` — these are wire format
   concerns.
4. **Never re-exports domain types.** Consumers import domain types from `domain`,
   not from `hook_adapter`.
5. **No environment access.** Does not read env vars. Receives `Environment` as a
   parameter.
6. **Maps wire names to stable internal names.** Claude Code's field names are mapped
   to `PermissionRequest`'s stable field names here. If Claude renames a field
   upstream, only this module's mapping changes.

**If Claude Code changes their protocol:** Only this module changes. Domain types,
config, and decision logic remain untouched.

### `decision_engine/` — The Core Logic

**Purpose:** Given a `PermissionRequest` and a `Policy` (both domain types), evaluate
which rules match, aggregate the results, apply permission mode modifiers, and produce
a final `PermissionDecision`.

**Interface:**

```rust
pub fn evaluate(
    request: &PermissionRequest,
    policy: &Policy,
) -> Option<PermissionDecision>
```

Two domain types in, one result out. The engine never touches config loading, JSON
parsing, I/O, or environment variables.

**Internal structure:**

```
decision_engine/
  mod.rs              Entry point: evaluate(), aggregation, mode modifiers
  matcher/
    mod.rs            Dispatch to rule-type-specific matchers
    bash.rs           matches(&BashRule, &CommandSegment) → bool
    file.rs           matches(&FileRule, &FileTarget) → bool
  evaluator/
    mod.rs            Dispatch to rule-type-specific evaluators
    bash.rs           evaluate(&[BashRule], &CommandSegment) → Option<Decision>
    file.rs           evaluate(&[FileRule], &FileTarget) → Option<Decision>
```

The engine is split into two sub-modules:

- **`matcher/`** — Does a single rule match the input? Each rule type has its own
  file. `matcher::bash::matches()` takes a `BashRule` and a `CommandSegment` and
  returns whether the rule matches. `matcher::file::matches()` takes a `FileRule`
  and a `FileTarget` (which bundles the resolved path with cwd, project path, and
  other evaluation context) and returns whether the rule matches. Glob compilation
  and pattern matching (`globset`) live here, not in domain.
- **`evaluator/`** — What decision does a rule produce for this input? Each rule type
  has its own file. The evaluator calls the matcher internally, then returns the
  rule's `Decision` if it matched. The evaluator works per-rule; the top-level
  `evaluate()` iterates over all rules and aggregates.

The top-level `evaluate()` orchestrates:
1. Select the right policy set (bash rules or file rules) based on `ToolRequest`
2. For each item (command segment or file target), call the evaluator for each rule
3. Aggregate per-item decisions (most restrictive wins: Deny > Ask > Allow)
4. Apply permission mode modifiers
5. Build reason string
6. Return `Option<PermissionDecision>`

**File evaluation context:** `ToolRequest::File` already contains `Vec<FileTarget>`,
where each `FileTarget` bundles the resolved path with all evaluation context (cwd,
project path, etc.). The Hook Adapter builds these at parse time — the decision engine
receives them ready to use. No assembly step, no side effects, no env var reads.

**Rules:**

1. **Depends on `domain` and optionally `shell_parser`.** The engine receives domain
   types and returns domain types. It may use `shell_parser` to evaluate a command
   against a policy command pattern without duplicating parsing logic. It never
   touches JSON, KDL, stdin, stdout, or environment variables.
2. **Owns all matching logic.** Whether a rule matches an input is decided here, not
   in domain. Domain types describe rules; the engine interprets them.
3. **Owns all evaluation logic.** The sequence "match rules, determine decisions,
   aggregate, apply modifiers" is engine logic.
4. **Owns glob processing.** `globset` and any pattern compilation lives in the
   matcher, not in domain types.
5. **Pure functions.** No I/O, no side effects. Given the same inputs, always produces
   the same output. This makes it trivially testable.

### `shell_parser/` — Shell AST Parser

**Purpose:** Parse raw bash command strings into structured `CommandSegment` domain
types. This module bridges from raw shell syntax to the domain vocabulary.

**Contains:**

- `parse()` function: bash string → `Vec<CommandSegment>`
- AST walking logic (via `brush-parser`): handles pipes, sequences, subshells,
  compound commands
- Transparent wrapper handling: `env`, `command`, `exec`, `nohup`, `builtin`
  are unwrapped to find the actual program
- Flag expansion: combined short flags like `-rf` → `["-r", "-f"]`

**Rules:**

1. **Depends only on `domain`.** Produces `CommandSegment`, `ProgramName`, and `Flag`
   domain types.
2. **Shared utility.** Used by `hook_adapter` (to parse tool input), `config` (to
   parse inline rule strings), and optionally `decision_engine` (to compare commands
   against policy patterns). It is `pub(crate)` — not an architectural layer, just a
   leaf parser.

### `cli/` — The Controller

**Purpose:** Wire everything together. Resolve environment, discover and load config,
read input, call the engine, write output. This is the composition root and the
outermost layer of the application.

**Contains:**

- Environment resolution (`Environment` struct populated from env vars)
- Config discovery: if no config path is provided, search common locations
  (`$CLAUDE_PERMISSIONS_HOOK_CONFIG`, `~/.config/claude-permissions-hook/config.kdl`)
- Config file reading: CLI reads the file from disk, passes string content to
  `config::parse_policy()`
- Stdin/stdout handling
- Error-to-output mapping (see Behavior Matrix)
- The `run()` function that orchestrates the full flow

**Rules:**

1. **The only module that imports from multiple siblings.** CLI imports from `config`,
   `hook_adapter`, `decision_engine`, and `domain`.
2. **The only module that reads environment variables.** Home directory, cwd, config
   paths — all resolved here into `Environment` and passed as parameters.
3. **The only module that performs I/O.** Reading stdin, writing stdout, reading config
   files from disk — all done here. Inner modules (including config) receive only
   in-memory data.
4. **Minimal logic.** The controller coordinates, it doesn't decide. If you're writing
   an `if` that isn't about I/O routing, it probably belongs in the engine.
5. **Error handling at the boundary.** All `Result` types from inner modules are
   handled here — converted to appropriate output (see Behavior Matrix).

---

## Dependency Rules

```
┌────────────────────────────────────────────────────────────┐
│                           CLI                               │
│                   (composition root)                        │
│    imports: hook_adapter, config, decision_engine, domain   │
└──────┬──────────────┬──────────────────┬───────────────────┘
       │              │                  │
       ▼              ▼                  ▼
┌──────────────┐ ┌──────────┐ ┌──────────────────┐
│ Hook Adapter  │ │  Config  │ │ Decision Engine  │
│ (JSON wire)   │ │  (KDL)   │ │ (match + eval)   │
└──────┬───────┘ └────┬─────┘ └────────┬─────────┘
       │              │                │
       │    ┌─────────┤                │
       │    │         │                │
       ▼    ▼         ▼                ▼
┌──────────────────────────────────────────────┐
│              Shell Parser                     │
│              (brush AST)                      │
└──────────────────────┬───────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────┐
│                         Domain                                │
│          (depends on nothing — only std + thiserror)           │
└──────────────────────────────────────────────────────────────┘
```

1. **Domain depends on nothing** (only `std` and lightweight utilities like
   `thiserror`). No `globset`, no `serde`, no `kdl`, no `brush-parser`.
   Cross-module error types live in `crate::error`, not in domain.
2. **Shell Parser depends only on domain.** Produces domain types. Used as a shared
   utility by hook_adapter, config, and optionally decision_engine.
3. **Adapters depend on domain and shell_parser.** `config`, `hook_adapter`, and
   `decision_engine` each import from `domain` (and optionally `shell_parser`) but
   never from each other.
4. **CLI depends on everything.** It is the composition root — the outermost layer
   that wires all modules together and handles all I/O and environment access.
5. **No lateral dependencies between adapters.** `config` never imports `hook_adapter`.
   `decision_engine` never imports `config`. If two modules need to share a type,
   that type belongs in `domain`. If two modules need shared parsing logic, it
   belongs in `shell_parser`.

If you find yourself wanting module A to import from module B (where neither is
`domain`, `shell_parser`, or `cli`), that's a signal: either move the shared type to
`domain`, or rethink the module boundary.

---

## Contract-Level API Signatures

### Domain

The domain module exports all shared types. Key public items:

```rust
use std::path::{Path, PathBuf};

// --- Core decision types ---

/// The raw verdict. Ordered by severity: Deny > Ask > Allow.
pub enum Decision { Allow, Ask, Deny }

/// Composite result: verdict + human-readable explanation.
pub struct PermissionDecision {
    pub decision: Decision,
    pub reason: String,
}

/// Claude Code's current permission mode.
pub enum PermissionMode { Default, Plan, AcceptEdits, DontAsk, BypassPermissions }

/// Selects which Policy field to evaluate against.
pub enum PolicySet { Bash, File }

// --- Request types ---

/// Full evaluation context, built by hook_adapter from JSON input.
/// Only exists when parse_request() returns Ok — tool parse errors
/// and unknown tools are Err(HookParseError), handled by CLI.
pub struct PermissionRequest {
    pub tool: ToolRequest,
    pub cwd: PathBuf,
    pub mode: PermissionMode,
    pub session_id: String,
    pub project_path: PathBuf,
    // ... other stable hook context fields
}

/// What Claude Code is asking to do. Only valid, evaluatable requests.
/// Unknown tools and parse errors are Err(ToolParseError), not variants.
pub enum ToolRequest {
    Bash { segments: Vec<CommandSegment> },
    File { operation: FileOperation, targets: Vec<FileTarget> },
}

pub enum FileOperation { Read, Write, Edit, Glob, Grep }
pub struct CommandSegment { pub program: ProgramName, pub args: Vec<String> }

// --- Value objects ---

pub struct ProgramName(/* non-empty basename */);
pub struct Flag(/* always has leading dashes */);

/// The subject of file rule evaluation. Built by hook_adapter at parse time.
/// Bundles the file path with all context needed to evaluate file rules.
pub struct FileTarget {
    pub raw_path: String,          // Original path (for display/reasons)
    pub normalized_path: PathBuf,  // Absolute, cleaned (for matching)
    pub cwd: PathBuf,              // Working directory (for <cwd> expansion)
    pub project_path: PathBuf,     // Project root
    // ... other evaluation context as needed
}

/// Resolved environment context. Constructed by CLI, passed to modules.
/// All path-related fields use PathBuf — never raw strings.
pub struct Environment {
    pub home: PathBuf,
    pub cwd: PathBuf,
    // ... other resolved env values as needed
}

// --- Policy types ---

pub struct Policy {
    pub bash: Vec<BashRule>,   // empty = no opinion for bash tools
    pub files: Vec<FileRule>,  // empty = no opinion for file tools
}

pub struct BashRule { pub decision: Decision, pub program: ProgramName, pub conditions: BashConditions }
pub struct FileRule { pub decision: Decision, pub path_pattern: PathPattern, pub operations: HashSet<FileOperation> }
pub struct BashConditions { /* required_flags, optional_flags, subcommands, positionals, arguments */ }
pub struct PathPattern { /* raw: String — NOT PathBuf, globs are not filesystem paths */ }
```

**Domain does NOT contain error types.** Cross-module errors live in `crate::error`
(see below). Domain holds only the core semantic vocabulary.

**Path type convention:** All path-related fields and parameters use `PathBuf`
(owned) or `&Path` (borrowed) — never `&str` or `String`. Rust's `Path`/`PathBuf`
handle OS-specific path separators, encoding, and normalization correctly. Exceptions:
- `FileTarget::raw_path`: `String` — the user-provided display form, not a filesystem
  path for matching.
- `PathPattern::raw`: `String` — a glob pattern (e.g., `<cwd>/**/*.rs`), not a
  filesystem path. Compiled to a matcher in `decision_engine`, never used as a `Path`.

### Error Types (`crate::error`)

Cross-module errors live in `src/error.rs`, not in domain. This prevents domain from
becoming a grab bag of operational concerns and avoids coupling domain to specific
adapter failure modes. Each error type is returned by one module and handled by CLI.

```rust
use std::path::PathBuf;
use crate::domain::PolicySet;

/// Error returned by hook_adapter when the tool cannot be parsed into a
/// valid ToolRequest. CLI handles these directly (see Behavior Matrix).
pub enum ToolParseError {
    /// Unrecognized tool name — CLI returns no opinion.
    UnknownTool { tool_name: String },
    /// Known policy set but malformed input — CLI returns Ask or no opinion.
    InvalidInput { policy_set: PolicySet, reason: String },
}

/// Error returned by hook_adapter::parse_request when JSON is structurally
/// invalid or missing required envelope fields.
pub enum HookParseError {
    InvalidJson(String),
    MissingField(String),
    /// Tool input could not produce a valid ToolRequest.
    ToolError(ToolParseError),
    // ... other structural failures
}

/// Error returned by config::parse_policy when the KDL content is invalid.
pub enum ConfigError {
    InvalidSyntax(String),
    // ... other parse-level failures
}
```

Note: `ConfigError` no longer contains `FileNotReadable` — file I/O errors are
handled by CLI directly (it reads the file). `ConfigError` only covers parsing
failures on the string content.

### Hook Adapter

```rust
/// Parse raw JSON hook input into a PermissionRequest with a valid ToolRequest.
/// Returns Err(HookParseError) if:
///   - JSON is malformed or missing required envelope fields
///   - Tool is unknown (HookParseError::ToolError(UnknownTool))
///   - Tool input is malformed (HookParseError::ToolError(InvalidInput))
/// CLI matches on the error variant to decide the response (see Behavior Matrix).
pub fn parse_request(json: &str, env: &Environment) -> Result<PermissionRequest, HookParseError>

/// Format a decision result into JSON for Claude Code.
/// None → "{}" (empty JSON object — no opinion).
/// Some → full hookSpecificOutput JSON envelope.
pub fn format_response(result: Option<&PermissionDecision>) -> String
```

### Config

```rust
/// Parse KDL config content into a Policy.
/// CLI reads the file from disk; this function only parses the string content.
/// Returns Err(ConfigError) if the content contains invalid KDL.
/// Missing or empty sections produce an empty Vec in the resulting Policy.
pub fn parse_policy(content: &str, env: &Environment) -> Result<Policy, ConfigError>
```

Config discovery (which path to use) and file reading are CLI's responsibility.
The config module receives only string content — it never touches the filesystem.

### Decision Engine

```rust
/// Evaluate a permission request against a policy.
/// Returns None when we have no opinion (unknown tool, no policy set for tool type).
/// Returns Some when we have a verdict (allow, ask, or deny with reason).
/// Pure function — no I/O, no env access, no side effects.
pub fn evaluate(
    request: &PermissionRequest,
    policy: &Policy,
) -> Option<PermissionDecision>
```

---

## Behavior Matrix

Single normative table covering all fallback, error, and edge-case branches.

| Scenario | Result | Reason |
|---|---|---|
| **Config: no path provided** | CLI searches common locations | `$CLAUDE_PERMISSIONS_HOOK_CONFIG`, then `~/.config/claude-permissions-hook/config.kdl` |
| **Config: no file found anywhere** | `Some(Ask)` | "No configuration file found at [searched paths]. Configure permissions to control tool access." |
| **Config: file found but read error** | `Some(Ask)` | "Failed to read config: [error]" (CLI handles I/O error directly) |
| **Config: file found but invalid KDL** | `Some(Ask)` | "Config parse error: [details]" (from `ConfigError`) |
| **Config: empty section (e.g. `bash {}`)** | Empty `Vec` in Policy | Treated as if the section was absent |
| **Policy: empty rules for tool's policy set** | `None` | No opinion — user chose not to define rules for this tool type |
| **Policy: rules exist but none match** | `Some(Ask)` | "No matching rule found for [tool details]" |
| **Tool: unknown tool type** | `None` | `Err(ToolError(UnknownTool))` — CLI returns no opinion |
| **Tool: known type but invalid input** | `Some(Ask)` if policy has rules for that set; `None` otherwise | `Err(ToolError(InvalidInput))` — CLI checks policy and decides |
| **Aggregation: multiple items, mixed decisions** | Most restrictive wins | `Deny > Ask > Allow` |
| **Mode: BypassPermissions + Ask** | Escalates to `Allow` | Reason preserved from original Ask |
| **Mode: DontAsk + Ask** | Downgrades to `Deny` | Reason preserved from original Ask |
| **Mode: BypassPermissions/DontAsk + Allow/Deny** | Unchanged | Config-level Allow/Deny are absolute |
| **Mode: Default/Plan/AcceptEdits + any** | Unchanged | These modes don't modify decisions |
| **JSON parse: malformed stdin** | `Some(Ask)` | "Failed to parse hook input: [error]" |

---

## Determinism and Tie-Break Rules

### Same-severity matches

When multiple rules match the same item with the same `Decision` severity, the
**first matching rule wins** (rules are evaluated in config file order). This
determines which rule's conditions appear in the reason string.

### Aggregation across items

When a tool request contains multiple items (e.g., a pipe with three commands, or a
glob matching five files), each item is evaluated independently. The final decision
is the most restrictive across all items (`Deny > Ask > Allow`).

### Reason string generation

The reason string always identifies the specific item and rule that drove the
decision:

- For single-item requests: the reason comes from the matching (or default) rule
- For multi-item requests where items agree: the reason summarizes the unanimous
  decision
- For multi-item requests where items disagree: the reason identifies the most
  restrictive item and explains why it dominated

---

## Design Patterns

### Parse, Don't Validate

Every boundary (JSON input, KDL config, shell commands) parses raw data into typed
domain representations. After construction, domain types are guaranteed valid:

- `ProgramName` is always a normalized basename
- `Flag` always has leading dashes
- `FileTarget` always carries a valid normalized path and its full evaluation context
- `CommandSegment` always has a valid program name
- `ToolRequest` only contains valid, evaluatable requests (no error variants)

The engine never re-validates. If it receives a `ProgramName`, it trusts it.

### Fail-Safe Defaults

When the system encounters an error or unexpected state, it defaults to the safest
option — `Ask` with a descriptive reason. This gives the user visibility into the
problem while preventing silent permission grants. See the Behavior Matrix for the
complete list of error-to-decision mappings.

This applies to error states only. "No opinion" (`None`) is not a fallback — it is
a deliberate, correct result when the system has no rules governing a request.

### Severity-Based Aggregation

When multiple rules or multiple items (commands in a pipe, files in a glob) produce
different decisions, the most restrictive wins:

```
Deny > Ask > Allow
```

This is simple, predictable, and safe.

### Enums for Closed Sets

`Decision`, `PermissionMode`, `FileOperation`, `PolicySet` are enums, not traits.
The set of variants is fixed and known. Pattern matching is exhaustive — the compiler
catches missing cases when a variant is added. This is safer and more idiomatic than
trait-based polymorphism for types we own entirely.

### Be Pragmatic, Be Simple

Every type, module, and abstraction must earn its existence. Code is a liability —
the less of it, the better.

- **Don't create types just because.** A `struct Wrapper(String)` is only justified
  if it prevents misuse or carries an invariant. If a plain `String` or `&str` is
  clear in context, use it.
- **Don't create modules just because.** If a module would contain a single file with
  a single type, it's probably not a module — it's a file.
- **Don't split what doesn't need splitting.** If two things always change together
  and are always used together, they belong together. Splitting them adds indirection
  without adding clarity.
- **Prefer fewer files with clear names over many small files.** A 200-line file with
  three related types is better than three 70-line files that force readers to jump
  between tabs.
- **Question every layer.** If removing a layer would make the code simpler and no
  less correct, remove it. Layers exist to isolate change, not to satisfy diagrams.
- **Type aliases are not abstractions.** `type BashConfig = Vec<BashRule>` adds a name
  but no invariant, no method, no encapsulation. Use it only when the alias genuinely
  improves readability at call sites.

The goal is not architectural purity. The goal is code that is easy to read, easy to
change, and hard to misuse.

---

## Anti-Patterns to Avoid

### Don't add traits for single implementations

If there's only one way to load config (KDL files), don't create a `trait ConfigLoader`.
Add the trait when a second implementation actually exists.

### Don't put matching or evaluation logic in domain

Domain types are data containers. They describe what a rule looks like, not how to
check if it matches. `matches()` belongs in `decision_engine::matcher`, not on
`BashRule` or `FileRule`. The domain holds the data; the engine interprets it.

### Don't leak wire format into domain

No `#[serde(rename)]` on domain types. No `camelCase` or `snake_case` concerns in
domain. Serialization is the adapter's job. See "Serde and Domain Types" below for
the full rationale.

### Don't read environment variables in inner modules

Only CLI reads env vars (`HOME`, `CWD`, config paths). All other modules receive
the `Environment` struct as a parameter. This keeps inner modules pure and testable.

### Don't re-export across module boundaries

Each module exports its own types. If `decision_engine` needs `Decision`, it imports
`crate::domain::Decision`, not `crate::hook_adapter::Decision`. Re-exports blur
boundaries and create false dependency paths.

### Don't over-abstract for hypothetical futures

We support one config format (KDL) and one hook protocol (Claude Code). We don't
abstract over hypothetical YAML configs or hypothetical VS Code plugins. When that
need arises, we refactor. Until then, concrete is correct.

---

## Design Decisions and Rationale

### Serde and Domain Types

Rust coding standards generally recommend `#[derive(Serialize, Deserialize)]` on
public data types. This project intentionally departs from that convention for domain
types. The rationale:

1. **Domain types are internal vocabulary, not wire types.** They are never serialized
   to JSON, KDL, or any external format. Adapters own the wire format and translate
   at the boundary.
2. **Serde derives create implicit coupling.** If domain types have `Serialize`, a
   future contributor might serialize them directly instead of going through the
   adapter, leaking internal structure into the wire format.
3. **Serde pulls in dependencies.** Domain's dependency on only `std` + `thiserror`
   is a deliberate architectural constraint. Adding `serde` would weaken it.
4. **Wire format concerns live in adapters.** `HookInput`/`HookOutput` (in
   `hook_adapter`) have serde derives. Config's internal KDL types have whatever
   parsing they need. These are the right places for serialization.

If a domain type genuinely needs serialization (e.g., for debug logging or test
snapshots), add it to the specific type with a comment explaining why — don't blanket
`derive(Serialize)` across all domain types.

### API Boundary Ownership Conventions

Public API functions use a mix of borrowed and owned parameter types. The rationale:

- **`parse_request(json: &str, ...)`** — Takes `&str` because the caller (CLI)
  already owns the JSON string from reading stdin. Copying it into an owned `String`
  would be wasteful — the function only needs to read the data, not store it.
- **`parse_policy(content: &str, ...)`** — Same reasoning: CLI owns the string from
  reading the config file.
- **`format_response(result: Option<&PermissionDecision>)`** — Takes a reference
  because the caller still owns the decision and may use it after formatting (e.g.,
  for logging).
- **`evaluate(request: &PermissionRequest, policy: &Policy)`** — Both are borrowed
  because the engine only reads them.
- **`Environment` fields are `PathBuf` (owned)** — The struct is constructed once by
  CLI and passed by reference to callees. It owns its data because it outlives all
  the functions that borrow from it.

The general rule: **functions borrow when they only need to read; structs own when
they need to outlive the scope that created them.** This is standard Rust ownership
practice. At public API boundaries, prefer `&T` parameters when the callee doesn't
need ownership, even though this creates a lifetime dependency — in this codebase,
lifetimes are simple (CLI owns everything, callees borrow) so the ergonomic cost is
negligible.
