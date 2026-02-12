# Rust Compiler Consistency Workflow

This document describes the current `snap parity` behavior in the Rust-first compiler.

## What parity checks now

`snap parity` now compares two Rust-backed compilation runs (instead of Babel-vs-Rust).
The two runs execute across compiler reload boundaries to help detect stateful nondeterminism.
This acts as a deterministic consistency check and report generator for fixture processing.

If no fixture corpus is present, parity reports `0 fixtures matched` and exits successfully.

> Note: the JSON report still uses legacy field names like `babel` / `rust` for historical
> compatibility, even though both sides are Rust-backed executions.

## Commands

From `compiler/`:

```bash
# Run parity with default options
yarn snap:parity

# Run full parity sweep and fail on any mismatch
yarn snap:parity:full

# Run a fast canary parity set
yarn snap:parity:canary
```

Direct invocation:

```bash
yarn workspace snap run snap parity \
  -p "while-*" \
  --output artifacts/rust-parity-while.json \
  --fail-on-mismatch=false
```

## Useful flags

- `--pattern/-p <glob>`: compare a fixture subset
- `--output/-o <path>`: write JSON report
- `--max-mismatches <n>`: stop after `n` mismatches (`0` = no limit)
- `--include-output`: include full snapshot payloads in JSON
- `--ignore-formatting` (default: `true`): normalize code section formatting
- `--ignore-logs` (default: `true`): ignore logger output differences
- `--skip-build` (default: `false`): reuse existing build outputs

Fixture source path can be overridden with `REACT_COMPILER_FIXTURES_PATH`
(absolute path or path relative to `compiler/`).

## Rust CLI bridge behavior

The Babel bridge resolves the Rust CLI command in this order:

1. `REACT_COMPILER_RUST_CLI_BIN` (explicit binary path/command)
2. existing debug binary file (`compiler/rust/target/debug/react_compiler_cli`) when
   `REACT_COMPILER_RUST_USE_PREBUILT_BIN=1`
3. fallback to `cargo +stable run --manifest-path ... -p react_compiler_cli`

For repeated local runs, setting `REACT_COMPILER_RUST_CLI_BIN` to a prebuilt binary
avoids repeated Cargo startup overhead.
If `REACT_COMPILER_RUST_CLI_BIN` contains path separators, it is resolved to an
absolute path and validated before invocation; bare command names are resolved via `PATH`.
If `REACT_COMPILER_RUST_MANIFEST` is set, it is treated as authoritative and must
resolve to an existing `Cargo.toml` file path.
Explicit path-form overrides support `~` home-directory expansion.
When `REACT_COMPILER_RUST_USE_PREBUILT_BIN=1` and no explicit profile is set,
the bridge checks `target/debug` first, then `target/release`.
`REACT_COMPILER_RUST_PREBUILT_PROFILE` can force a specific prebuilt profile
(`debug` or `release`).
`REACT_COMPILER_RUST_USE_PREBUILT_BIN` accepts case-insensitive truthy values
`1|true|yes|on`.

Additional bridge execution knobs:

- `REACT_COMPILER_RUST_CLI_TIMEOUT_MS` — positive integer timeout in milliseconds
  for each CLI invocation (default: `60000`).
- `REACT_COMPILER_RUST_CLI_MAX_BUFFER_BYTES` — positive integer max stdio buffer
  for each CLI invocation (default: `67108864`).

If manifest discovery fails, the bridge error message includes the candidate
manifest paths that were checked.
Non-zero Rust CLI exits surface whichever stdio stream contains output
(stderr first, then stdout) to improve failure diagnostics.
Bridge invocation errors include the full resolved command + arguments.

## Protocol contract (current)

- Request includes `protocol_version` (currently `1`)
- Response may include `protocol_version` on `ok` and `error`
- `protocol_version` values in responses must be integers and non-negative
- Unsupported request versions are rejected with:
  - `code: "unsupported_protocol_version"`
  - `category: "request"`
  - `reason: "invalid_option"`

Bridge response validation currently enforces:

- `status: "ok" | "error"`
- `ok` includes non-empty string `code`
- optional `ok.react_functions` entries require:
  - `name: non-empty string`
  - `kind: "Component" | "Hook"`
  - optional `loc` with numeric source coordinates
- optional `ok.debug_ir`, when present, must be a string
- `error` includes string fields:
  `code`, `category`, `reason`, `severity`, `message`
- required string fields are validated as non-empty
- validated `error.category` values: `request`, `syntax`, `internal`
- validated `error.severity` values: `error`, `warning`, `hint`, `off`
- optional `error.location`, when present, must include numeric:
  `start_line`, `start_column`, `end_line`, `end_column`
- all validated location payloads must have non-decreasing source ranges
  (`end` cannot precede `start`)

## Rust frontend behavior

- Placeholder transforms are always enabled (`apply_placeholder_transforms: true`)
- Dialect detection behavior:
  - `*.ts`/`*.tsx`/`*.mts`/`*.cts` => TypeScript
  - `*.flow` or `parserOpts.plugins` containing `flow` => Flow
  - `parserOpts.plugins` containing `typescript` => TypeScript
  - when extensions are inconclusive and both parser plugins are present, the first matching plugin in parser order wins
  - otherwise => JavaScript
- Runtime helper alias detection recognizes:
  - direct `require(...)`
  - sequence wrappers like `(0, require)(...)`
  - `module.require(...)` and global-module forms (`globalThis.module.require(...)`, `global.module['require'](...)`, `window.module.require(...)`, `self.module['require'](...)`)
  - sequence-call wrappers for global-module aliases (e.g. `(0, window.module.require)(...)`, `(0, self.module['require'])(...)`)
  - escaped/unicode computed property variants for `module` / `require` keys
  - nested computed chains such as `globalThis['module']['require'](...)`
  - mixed member/computed chains such as `globalThis.module['require'](...)` and `self['module'].require(...)`
  - sequence wrappers around intermediate module roots/chains (e.g. `(0, window.module).require(...)`, `(0, self['module'])['require'](...)`)
  - direct/chain variants on browser-like globals (`window.module.require(...)`, `window['module']['require'](...)`, `self.module.require(...)`, `self['module']['require'](...)`)
  - nested global-root chains before `module` (e.g. `globalThis.window.module.require(...)`, `global.self.module['require'](...)`)
  - repeated global-root chains (e.g. `globalThis.globalThis.window.module.require(...)`, `global.global.self.module['require'](...)`)
  - sequence wrappers over nested global-root chains (e.g. `(0, globalThis.window.module.require)(...)`, `(0, global.self.module['require'])(...)`)
  - computed nested global-root chains (e.g. `globalThis['window'].module.require(...)`, `global['self'].module['require'](...)`)
  - sequence wrappers over computed nested global roots (e.g. `(0, globalThis['window'].module.require)(...)`, `(0, global['self'].module['require'])(...)`)
  - sequence wrappers over computed nested global-root chains (e.g. `(0, globalThis['window'].module.require)(...)`, `(0, global['self'].module['require'])(...)`)
- Rust output is used directly for AST replacement
- Compatibility logger events are emitted for:
  - `CompileSuccess` (per detected React function when locations are available)
  - `CompileError`
  - `PipelineError`
- Rust error categories are normalized to legacy compiler categories for downstream
  lint consumer compatibility (`syntax -> Syntax`, `internal/request -> Invariant`).
- Rust error codes can further refine mapped legacy categories for compatibility
  (e.g. `unsupported_flow_syntax -> UnsupportedSyntax`).
- Rust severity values are preserved where possible for downstream diagnostics
  (`warning`/`hint`/`off` map to legacy severities; request/internal categories remain category-driven).
- Invocation and output-parse pipeline errors include the underlying Rust CLI
  failure cause in the emitted message payload.
- When requested via logger, Rust debug IR is forwarded through `debugLogIRs`

## Report shape

Parity JSON reports include mismatch counts and section-level diffs:

- code
- eval output
- logs
- error

Report payloads include `firstRun*` / `secondRun*` fields and retain legacy
`babel*` / `rust*` aliases for compatibility with older tooling.
Reports also include a `notes` block with `comparisonMode: "rust_consistency"`.

This helps isolate semantic mismatches from formatting/logging noise.

