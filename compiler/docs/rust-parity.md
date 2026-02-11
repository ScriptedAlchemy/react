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
2. existing debug binary (`compiler/rust/target/debug/react_compiler_cli`) **only when**
   `REACT_COMPILER_RUST_USE_PREBUILT_BIN=1`,
3. otherwise `cargo +stable run --manifest-path ... -p react_compiler_cli`.

For repeated local parity runs, setting `REACT_COMPILER_RUST_CLI_BIN` to a prebuilt
binary avoids repeated Cargo invocation overhead while avoiding stale binary ambiguity.

## Rust CLI protocol contract

The JS bridge and Rust CLI communicate via a versioned JSON protocol.

- Request field: `protocol_version` (current value: `1`).
- Response field: `protocol_version` (returned on both `ok` and `error` responses).
- Unsupported request versions are rejected by CLI with:
  - `code: "unsupported_protocol_version"`
  - `category: "request"`
  - `reason: "invalid_option"`.

Bridge compatibility behavior:

- The JS bridge sends `protocol_version: 1` by default.
- The bridge validates response protocol version when present.
- For backward compatibility with older local binaries, a missing
  `protocol_version` response field is tolerated.

Bridge response-shape guardrails:

- Response must be a JSON object with `status: "ok" | "error"`.
- `ok` responses must include required typed metadata fields:
  - string `code`,
  - non-negative integer count fields (statement counts, detected counts,
    transform counts, runtime candidate counts),
  - required string arrays for detected/transform/runtime-candidate names,
  - boolean runtime helper/runtime callee flags.
- `ok` responses additionally enforce derived consistency invariants, including:
  - count fields matching corresponding array lengths,
  - component/hook subtotal counts matching their totals,
  - candidate count = transformed + skipped,
  - transform candidate/detected/runtime candidate name arrays are duplicate-free,
  - runtime helper import `after >= before` and `*_added` matching count delta,
  - runtime callee/namespace candidate arrays must be duplicate-free and
    disjoint (both before and after transform).
- `placeholder_transform_status` is validated against the known status set:
  `disabled | no_candidates | transformed | blocked_missing_runtime_callee | no_op`.
- `error` responses must include string fields:
  `code`, `category`, `reason`, `severity`, `message`.
- Source locations in both `ok` (`react_functions[*].loc`) and `error`
  (`location`) payloads must be either null/omitted or objects with
  non-negative integer `start_line`, `start_column`, `end_line`, `end_column`.

For strict-rust debugging, `BabelPlugin` emits
`RustFrontendProtocolVersion` via `logger.debugLogIRs`.

## Strict-rust frontend transform toggle

Strict rust mode keeps frontend placeholder transforms disabled by default.
To enable Rust frontend placeholder transforms in strict mode, set:

- `REACT_COMPILER_RUST_PLACEHOLDER_TRANSFORMS=1`

When enabled (and only when `REACT_COMPILER_RUST_STRICT=1`), the Babel bridge
sends `apply_placeholder_transforms: true` to the Rust CLI request payload and
accepts Rust frontend output as the strict-rust program replacement input.
This allows strict mode to execute Rust-side placeholder transform behavior
end-to-end.

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

