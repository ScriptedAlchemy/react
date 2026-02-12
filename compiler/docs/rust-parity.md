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

1. `REACT_COMPILER_RUST_CLI_BIN` (explicit binary path)
2. existing debug binary (`compiler/rust/target/debug/react_compiler_cli`) when
   `REACT_COMPILER_RUST_USE_PREBUILT_BIN=1`
3. fallback to `cargo +stable run --manifest-path ... -p react_compiler_cli`

For repeated local runs, setting `REACT_COMPILER_RUST_CLI_BIN` to a prebuilt binary
avoids repeated Cargo startup overhead.

## Protocol contract (current)

- Request includes `protocol_version` (currently `1`)
- Response may include `protocol_version` on `ok` and `error`
- Unsupported request versions are rejected with:
  - `code: "unsupported_protocol_version"`
  - `category: "request"`
  - `reason: "invalid_option"`

Bridge response validation currently enforces:

- `status: "ok" | "error"`
- `ok` includes string `code`
- `error` includes string fields:
  `code`, `category`, `reason`, `severity`, `message`
- optional `error.location`, when present, must include numeric:
  `start_line`, `start_column`, `end_line`, `end_column`

## Rust frontend behavior

- Placeholder transforms are always enabled (`apply_placeholder_transforms: true`)
- Rust output is used directly for AST replacement
- Compatibility logger events are emitted for:
  - `CompileSuccess`
  - `CompileError`
  - `PipelineError`
- When requested via logger, Rust debug IR is forwarded through `debugLogIRs`

## Report shape

Parity JSON reports include mismatch counts and section-level diffs:

- code
- eval output
- logs
- error

Report payloads include `firstRun*` / `secondRun*` fields and retain legacy
`babel*` / `rust*` aliases for compatibility with older tooling.

This helps isolate semantic mismatches from formatting/logging noise.

