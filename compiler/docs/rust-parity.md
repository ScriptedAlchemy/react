# Strict Rust Parity Workflow

This document describes how to compare compiler output between:

- the current Babel pipeline baseline, and
- strict Rust frontend mode (`compilerEngine: 'rust'` + `REACT_COMPILER_RUST_STRICT=1`),

using the `snap parity` command.

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

You can also call `snap parity` directly:

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
- `--include-output`: include full Babel/Rust snapshot payloads in JSON
- `--ignore-formatting` (default: `true`): normalize code section formatting
- `--ignore-logs` (default: `true`): ignore logger output differences
- `--skip-build` (default: `false`): reuse existing build outputs

## Rust CLI bridge execution

The Babel plugin Rust bridge resolves the compiler CLI command in this order:

1. `REACT_COMPILER_RUST_CLI_BIN` (explicit binary path),
2. existing debug binary (`compiler/rust/target/debug/react_compiler_cli`),
3. `cargo +stable run --manifest-path ... -p react_compiler_cli`.

For repeated local parity runs, setting `REACT_COMPILER_RUST_CLI_BIN` to a prebuilt
binary avoids repeated Cargo invocation overhead.

## Report shape

Parity JSON includes:

- fixture counts and mismatch counts,
- per-mismatch booleans for:
  - raw mismatch,
  - normalized mismatch,
  - code section mismatch,
  - eval output mismatch,
  - logs mismatch,
  - error mismatch,
- per-mismatch section diff payloads (when mismatched):
  - `codeSectionDiff`
  - `evalSectionDiff`
  - `logsSectionDiff`
  - `errorSectionDiff`
  Each section payload includes raw Babel/Rust text plus normalized strings used for parity comparison.
- aggregate `mismatchSummary` counts by category.

This allows faster triage by separating semantic differences from
formatting/logging noise.
