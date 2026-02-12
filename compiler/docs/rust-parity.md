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
absolute path and validated before invocation; bare command names are resolved and
validated via `PATH` before invocation.
If `REACT_COMPILER_RUST_MANIFEST` is set, it is treated as authoritative and must
resolve to an existing `Cargo.toml` file path.
Explicit path-form overrides support `~` home-directory expansion.
Auto-discovered manifest candidates must also resolve to actual files.
When `REACT_COMPILER_RUST_USE_PREBUILT_BIN=1` and no explicit profile is set,
the bridge checks `target/debug` first, then `target/release`.
`REACT_COMPILER_RUST_PREBUILT_PROFILE` can force a specific prebuilt profile
(`debug` or `release`).
When a specific prebuilt profile is forced and no matching binary exists, the
bridge fails fast instead of silently falling back to Cargo invocation.
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
- Request-side bridge validation requires non-negative integer `protocol_version`
  when explicitly provided.
- numeric string request versions (e.g. `"1"`) are coerced and validated.
- request payload shape validation requires:
  - `source` is a string
  - optional `filename` is non-empty string
  - optional `is_module` / `apply_placeholder_transforms` / `emit_debug_ir` are booleans
  - optional `dialect` is one of `javascript | typescript | flow`
- Response may include `protocol_version` on `ok` and `error`
- `protocol_version` values in responses must be integers and non-negative
- Unsupported request versions are rejected with:
  - `code: "unsupported_protocol_version"`
  - `category: "request"`
  - `reason: "invalid_option"`

Bridge response validation currently enforces:

- `status: "ok" | "error"`
- `ok` includes non-empty string `code`
- `ok.react_functions` is required and entries require:
  - `name: non-empty string`
  - `kind: "Component" | "Hook"`
  - optional `loc` with numeric source coordinates
- strict `ok` metadata telemetry validation:
  - non-negative integer counters for statement/transform metrics
  - boolean state flags for helper import/callee reuse generation state
  - string-array telemetry payloads for candidate/skipped/transformed names
  - count fields must match their corresponding array lengths
  - `detected_react_functions` must match `react_functions.length`
  - aggregate consistency checks:
    - candidate count = transformed + skipped counts
    - component/hook subtype counts sum to their parent totals
    - detected component + detected hook counts = detected total
  - runtime helper import + callee flag coherence checks:
    - helper import counts are non-decreasing
    - helper-added flag must align with before/after import deltas
    - generated/reused callee flags are mutually exclusive and require transformed output
    - selected pre/post callee names must appear in their respective candidate lists
    - transformed output requires a post-transform runtime callee name
  - deterministic string-list telemetry arrays are validated as sorted + duplicate-free
  - transformed/skipped placeholder function arrays must be disjoint and compose
    exactly into `placeholder_transform_candidates`
  - `detected_component_functions` / `detected_hook_functions` must align with
    kind-partitioned names from `react_functions`
  - `placeholder_transform_status` enum validation:
    `disabled | no_candidates | transformed | blocked_missing_runtime_callee | no_op`
    with count/status coherence checks
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
- When compatibility logger debug hooks are enabled, Rust frontend emits:
  - `RustFrontendDebugIR` (string debug IR from Rust CLI)
  - `RustFrontendCompileMetadata` (serialized Rust-side transform/detection telemetry)
- Dialect detection behavior:
  - `*.ts`/`*.tsx`/`*.mts`/`*.cts` => TypeScript
  - `*.flow` or `parserOpts.plugins` containing `flow` => Flow
  - `parserOpts.plugins` containing `typescript` => TypeScript
  - when extensions are inconclusive and both parser plugins are present, the first matching plugin in parser order wins
  - otherwise => JavaScript
- Runtime helper alias detection recognizes:
  - direct `require(...)`
  - object destructure aliases (e.g. `const { c: cache } = runtime`)
  - defaulted object destructure aliases (e.g. `const { c: cache = fallback } = runtime`, `const { ['c']: cache = fallback } = runtime`, `const { ['\\x63']: cache = fallback } = runtime`)
  - assignment destructure aliases (e.g. `({ c: cache = fallback } = runtime)`, `({ c: cache = fallback } = require(...))`, `({ ['\\u0063']: cache = fallback } = module['\\x72equire'](...))`)
  - sequence wrappers like `(0, require)(...)`
  - `module.require(...)` and global-module forms (`globalThis.module.require(...)`, `global.module['require'](...)`, `window.module.require(...)`, `self.module['require'](...)`)
  - sequence-call wrappers for global-module aliases (e.g. `(0, window.module.require)(...)`, `(0, self.module['require'])(...)`)
  - escaped/unicode computed property variants for `module` / `require` keys
  - nested computed chains such as `globalThis['module']['require'](...)`
  - mixed member/computed chains such as `globalThis.module['require'](...)` and `self['module'].require(...)`
  - sequence wrappers around intermediate module roots/chains (e.g. `(0, window.module).require(...)`, `(0, self['module'])['require'](...)`)
  - direct/chain variants on browser-like globals (`window.module.require(...)`, `window['module']['require'](...)`, `self.module.require(...)`, `self['module']['require'](...)`)
  - mixed browser-global chain variants (e.g. `globalThis.window['module']['require'](...)`)
  - window member/computed require variants (e.g. `window.module['require'](...)`, `globalThis.window.module['require'](...)`)
  - escaped/unicode require-key variants in those chains (e.g. `window.module['\\x72equire'](...)`, `global.module['\\u0072equire'](...)`, `globalThis.module['\\x72equire'](...)`)
  - nested global-root chains before `module` (e.g. `globalThis.window.module.require(...)`, `global.self.module['require'](...)`)
  - repeated global-root chains (e.g. `globalThis.globalThis.window.module.require(...)`, `global.global.self.module['require'](...)`)
  - sequence wrappers over repeated global-root chains (e.g. `(0, globalThis.globalThis.window.module.require)(...)`, `(0, global.global.self.module['require'])(...)`)
  - sequence wrappers over nested global-root chains (e.g. `(0, globalThis.window.module.require)(...)`, `(0, global.self.module['require'])(...)`)
  - escaped/unicode module+require-key variants in sequence wrappers on global roots (e.g. `(0, global['\\x6dodule']['require'])(...)`, `(0, globalThis['\\u006dodule']['\\u0072equire'])(...)`)
  - computed nested global-root chains (e.g. `globalThis['window'].module.require(...)`, `global['self'].module['require'](...)`)
  - sequence wrappers over computed nested global roots (e.g. `(0, globalThis['window'].module.require)(...)`, `(0, global['self'].module['require'])(...)`)
  - fully computed nested global-root chains (e.g. `globalThis['window']['module']['require'](...)`, `global['self']['module']['require'](...)`)
  - mixed computed module-require combinations on nested roots (e.g. `globalThis.window['module'].require(...)`, `global['self']['module'].require(...)`)
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

