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
    - `statement_count_after_transform >= statement_count`
    - candidate count = transformed + skipped counts
    - component/hook subtype counts sum to their parent totals
    - detected component + detected hook counts = detected total
  - runtime helper import + callee flag coherence checks:
    - helper import counts are non-decreasing
    - helper-added flag must align with before/after import deltas
    - generated/reused callee flags are mutually exclusive and require transformed output
    - selected pre/post callee names must appear in their respective candidate lists
    - transformed output requires a post-transform runtime callee name
    - optional selected callee name fields must be non-empty when present
  - deterministic string-list telemetry arrays are validated as non-empty entries, sorted, + duplicate-free
  - transformed/skipped placeholder function arrays must be disjoint and compose
    exactly into `placeholder_transform_candidates`
  - `detected_component_functions` / `detected_hook_functions` must align with
    kind-partitioned names from `react_functions`
  - placeholder component/hook split counters are validated against the
    corresponding name arrays (using `react_functions` kinds with hook-name fallback)
  - `placeholder_transform_candidates` must contain every `react_functions.name`
  - every `react_functions.name` must also be represented in transformed or skipped partitions
  - `react_functions` ordering is validated as deterministic
    (location -> name -> kind), with duplicate entries and duplicate name/kind pairs rejected
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
  - static template-expression aliases when all interpolations fold to strings
    (e.g. ``module[`requ${'ire'}`](`react/compiler-${'runtime'}`)``)
  - static binary-string concatenation aliases
    (e.g. `module['requ' + 'ire']('react/compiler-' + 'runtime')`)
  - static conditional-literal aliases when test branches fold
    (e.g. `module[true ? 'require' : 'nope'](true ? 'react/compiler-runtime' : 'nope')`)
  - static logical/nullish aliases when literals determine branch selection
    (e.g. `module[(true && 'require') ?? 'nope']((null ?? 'react/compiler-runtime'))`)
  - static sequence-literal aliases when sequence entries fold
    (e.g. `module[(0, 'require')]((0, 'react/compiler-runtime'))`)
  - conditional aliases with foldable truthy/falsy literal tests
    (e.g. `module['x' ? 'require' : 'nope'](void false ? 'nope' : 'react/compiler-runtime')`)
  - strict equality/inequality literal conditionals
    (e.g. `module[(('require' === 'require') ? 'require' : 'nope')]((('runtime' === 'runtime') ? 'react/compiler-runtime' : 'nope'))`)
  - strict equality follows JavaScript `NaN` semantics (`NaN === NaN` is false),
    so foldable `NaN` comparisons select the same fallback branch as Babel
  - foldable loose equality/inequality conditionals (`==` / `!=`) over static primitives,
    including primitive coercions and `null == undefined` checks
    (e.g. `module[((true == 1) ? 'require' : 'nope')](((null == (void false)) ? 'react/compiler-runtime' : 'nope'))`)
  - foldable BigInt loose equality with static number/string operands
    (e.g. `(1n == 1)`, `(1n == '1')`)
  - foldable BigInt loose equality with radix-prefixed string operands
    (e.g. `(16n == '0x10')`, `(16n == '0b10000')`, `(16n == '0o20')`)
  - uppercase radix prefixes and surrounding whitespace also fold
    (e.g. `(16n == ' 0X10 ')`, `(16n == ' 0B10000 ')`, `(16n == ' 0O20 ')`)
  - BOM-wrapped radix prefixes also fold in BigInt loose equality
    (e.g. `(16n == '\uFEFF0x10\uFEFF')`, `(16n == '\uFEFF0b10000\uFEFF')`, `(16n == '\uFEFF0o20\uFEFF')`)
  - BOM-wrapped radix template strings also fold in BigInt loose equality
    (e.g. ``(16n == `${'\uFEFF0x10\uFEFF'}`)``, ``(16n == `${'\uFEFF0b10000\uFEFF'}`)``, ``(16n == `${'\uFEFF0o20\uFEFF'}`)``)
  - foldable BigInt loose equality with signed decimal string operands
    (e.g. `(16n == '+16')`)
  - space-wrapped signed decimal BigInt strings also fold in loose equality
    (e.g. `(16n == ' +16 ')`)
  - BOM-wrapped signed decimal BigInt strings also fold in loose equality
    (e.g. `(16n == '\uFEFF+16\uFEFF')`)
  - line-separator-wrapped signed decimal BigInt strings also fold in loose equality
    (e.g. `(16n == '\u2028+16\u2029')`)
  - foldable BigInt loose equality with signed decimal template-string operands
    (e.g. ``(16n == `${'+16'}`)``)
  - space-wrapped signed decimal BigInt template strings also fold in loose equality
    (e.g. ``(16n == `${' +16 '}`)``)
  - BOM-wrapped signed decimal BigInt template strings also fold in loose equality
    (e.g. ``(16n == `${'\uFEFF+16\uFEFF'}`)``)
  - line-separator-wrapped signed decimal BigInt template strings also fold in loose equality
    (e.g. ``(16n == `${'\u2028+16\u2029'}`)``)
  - signed decimals with whitespace after the sign follow JS `StringToBigInt` behavior and fold as non-equal
    (e.g. `(16n == '+ 16')` folds as false)
  - signed-decimal template strings with post-sign whitespace also fold as non-equal
    (e.g. ``(16n == `${'+ 16'}`)`` folds as false)
  - signed decimals with JS BOM/Unicode whitespace after the sign also fold as non-equal
    (e.g. `(16n == '+\uFEFF16')` folds as false)
  - signed decimals with post-sign line-separator whitespace also fold as non-equal
    (e.g. `(16n == '+\u202816')` folds as false)
  - signed decimals with post-sign paragraph-separator whitespace also fold as non-equal
    (e.g. `(16n == '+\u202916')` folds as false)
  - signed decimals with post-sign NBSP whitespace also fold as non-equal
    (e.g. `(16n == '+\u00A016')` folds as false)
  - signed decimals with post-sign Ogham-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u168016')` folds as false)
  - signed decimals with post-sign en-quad whitespace also fold as non-equal
    (e.g. `(16n == '+\u200016')` folds as false)
  - signed decimals with post-sign em-quad whitespace also fold as non-equal
    (e.g. `(16n == '+\u200116')` folds as false)
  - signed decimals with post-sign en-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u200216')` folds as false)
  - signed decimals with post-sign three-per-em-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u200416')` folds as false)
  - signed decimals with post-sign four-per-em-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u200516')` folds as false)
  - signed decimals with post-sign six-per-em-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u200616')` folds as false)
  - signed decimals with post-sign figure-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u200716')` folds as false)
  - signed decimals with post-sign punctuation-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u200816')` folds as false)
  - signed decimals with post-sign thin-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u200916')` folds as false)
  - signed decimals with post-sign hair-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u200A16')` folds as false)
  - signed decimals with post-sign narrow-no-break-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u202F16')` folds as false)
  - signed decimals with post-sign medium-mathematical-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u205F16')` folds as false)
  - signed decimals with post-sign ideographic-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u300016')` folds as false)
  - signed decimals with post-sign tab whitespace also fold as non-equal
    (e.g. `(16n == '+\t16')` folds as false)
  - signed decimals with post-sign line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\n16')` folds as false)
  - signed decimals with post-sign carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\r16')` folds as false)
  - signed decimals with post-sign form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\f16')` folds as false)
  - signed decimals with post-sign vertical-tab whitespace also fold as non-equal
    (e.g. `(16n == '+\v16')` folds as false)
  - signed decimals with post-sign vertical-tab+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\v\t16')` folds as false)
  - signed decimals with post-sign vertical-tab+tab+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\v\t\f16')` folds as false)
  - signed decimals with post-sign vertical-tab+tab+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\v\t\r16')` folds as false)
  - signed decimals with post-sign vertical-tab+carriage-return+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\v\r\t16')` folds as false)
  - signed decimals with post-sign vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\v\r\n16')` folds as false)
  - signed decimals with post-sign vertical-tab+carriage-return+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\v\r\f16')` folds as false)
  - signed decimals with post-sign vertical-tab+tab+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\v\t\n16')` folds as false)
  - signed decimals with post-sign vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\v\n\r16')` folds as false)
  - signed decimals with post-sign vertical-tab+line-feed+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\v\n\t16')` folds as false)
  - signed decimals with post-sign vertical-tab+line-feed+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\v\n\f16')` folds as false)
  - signed decimals with post-sign vertical-tab+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\v\f16')` folds as false)
  - signed decimals with post-sign vertical-tab+form-feed+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\v\f\n16')` folds as false)
  - signed decimals with post-sign vertical-tab+form-feed+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\v\f\t16')` folds as false)
  - signed decimals with post-sign vertical-tab+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\v\n16')` folds as false)
  - signed decimals with post-sign vertical-tab+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\v\r16')` folds as false)
  - signed decimals with post-sign carriage-return+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\r\n16')` folds as false)
  - signed decimals with post-sign line-feed+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\n\r16')` folds as false)
  - signed decimals with post-sign tab+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\n16')` folds as false)
  - signed decimals with post-sign tab+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\t\r16')` folds as false)
  - signed decimals with post-sign carriage-return+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\r\t16')` folds as false)
  - signed decimals with post-sign line-feed+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\n\f16')` folds as false)
  - signed decimals with post-sign form-feed+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\f\n16')` folds as false)
  - signed decimals with post-sign carriage-return+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\r\f16')` folds as false)
  - signed decimals with post-sign form-feed+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\f\r16')` folds as false)
  - signed decimals with post-sign tab+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\f16')` folds as false)
  - signed decimals with post-sign form-feed+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\f\t16')` folds as false)
  - signed decimals with post-sign form-feed+vertical-tab whitespace also fold as non-equal
    (e.g. `(16n == '+\f\v16')` folds as false)
  - signed decimals with post-sign form-feed+vertical-tab+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\f\v\r16')` folds as false)
  - signed decimals with post-sign form-feed+vertical-tab+carriage-return+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\f\v\r\t16')` folds as false)
  - signed decimals with post-sign form-feed+vertical-tab+line-feed+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\f\v\n\t16')` folds as false)
  - signed decimals with post-sign form-feed+vertical-tab+tab+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\f\v\t\r16')` folds as false)
  - signed decimals with post-sign form-feed+vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\f\v\n\r16')` folds as false)
  - signed decimals with post-sign form-feed+vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\f\v\r\n16')` folds as false)
  - signed decimals with post-sign form-feed+vertical-tab+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\f\v\n16')` folds as false)
  - signed decimals with post-sign form-feed+vertical-tab+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\f\v\t16')` folds as false)
  - signed decimals with post-sign tab+carriage-return+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\r\n16')` folds as false)
  - signed decimals with post-sign form-feed+carriage-return+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\f\r\n16')` folds as false)
  - signed decimals with post-sign carriage-return+line-feed+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\r\n\t16')` folds as false)
  - signed decimals with post-sign carriage-return+line-feed+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\r\n\f16')` folds as false)
  - signed decimals with post-sign carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
    (e.g. `(16n == '+\r\n\v16')` folds as false)
  - signed decimals with post-sign line-feed+carriage-return+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\n\r\f16')` folds as false)
  - signed decimals with post-sign line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
    (e.g. `(16n == '+\n\r\v16')` folds as false)
  - signed decimals with post-sign tab+carriage-return+line-feed+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\r\n\f16')` folds as false)
  - signed decimals with post-sign tab+line-feed+carriage-return+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\n\r\f16')` folds as false)
  - signed decimals with post-sign tab+line-feed+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\n\f16')` folds as false)
  - signed decimals with post-sign tab+line-feed+vertical-tab whitespace also fold as non-equal
    (e.g. `(16n == '+\t\n\v16')` folds as false)
  - signed decimals with post-sign tab+carriage-return+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\r\f16')` folds as false)
  - signed decimals with post-sign tab+form-feed+vertical-tab whitespace also fold as non-equal
    (e.g. `(16n == '+\t\f\v16')` folds as false)
  - signed decimals with post-sign tab+vertical-tab+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\t\v\r16')` folds as false)
  - signed decimals with post-sign tab+vertical-tab+form-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\v\f16')` folds as false)
  - signed decimals with post-sign tab+vertical-tab+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\v\n16')` folds as false)
  - signed decimals with post-sign tab+form-feed+line-feed whitespace also fold as non-equal
    (e.g. `(16n == '+\t\f\n16')` folds as false)
  - signed decimals with post-sign tab+form-feed+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\t\f\r16')` folds as false)
  - signed decimals with post-sign tab+carriage-return+vertical-tab whitespace also fold as non-equal
    (e.g. `(16n == '+\t\r\v16')` folds as false)
  - signed decimals with post-sign tab+line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
    (e.g. `(16n == '+\t\n\r\v16')` folds as false)
  - signed decimals with post-sign tab+carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
    (e.g. `(16n == '+\t\r\n\v16')` folds as false)
  - signed decimals with post-sign tab+line-feed+carriage-return whitespace also fold as non-equal
    (e.g. `(16n == '+\t\n\r16')` folds as false)
  - signed decimals with post-sign line-feed+carriage-return+tab whitespace also fold as non-equal
    (e.g. `(16n == '+\n\r\t16')` folds as false)
  - signed decimals with post-sign em-space whitespace also fold as non-equal
    (e.g. `(16n == '+\u200316')` folds as false)
  - signed-decimal template strings with post-sign BOM/Unicode whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\uFEFF16'}`)`` folds as false)
  - signed-decimal template strings with post-sign line-separator whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u202816'}`)`` folds as false)
  - signed-decimal template strings with post-sign paragraph-separator whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u202916'}`)`` folds as false)
  - signed-decimal template strings with post-sign NBSP whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u00A016'}`)`` folds as false)
  - signed-decimal template strings with post-sign Ogham-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u168016'}`)`` folds as false)
  - signed-decimal template strings with post-sign en-quad whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200016'}`)`` folds as false)
  - signed-decimal template strings with post-sign em-quad whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200116'}`)`` folds as false)
  - signed-decimal template strings with post-sign en-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200216'}`)`` folds as false)
  - signed-decimal template strings with post-sign three-per-em-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200416'}`)`` folds as false)
  - signed-decimal template strings with post-sign four-per-em-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200516'}`)`` folds as false)
  - signed-decimal template strings with post-sign six-per-em-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200616'}`)`` folds as false)
  - signed-decimal template strings with post-sign figure-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200716'}`)`` folds as false)
  - signed-decimal template strings with post-sign punctuation-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200816'}`)`` folds as false)
  - signed-decimal template strings with post-sign thin-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200916'}`)`` folds as false)
  - signed-decimal template strings with post-sign hair-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200A16'}`)`` folds as false)
  - signed-decimal template strings with post-sign narrow-no-break-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u202F16'}`)`` folds as false)
  - signed-decimal template strings with post-sign medium-mathematical-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u205F16'}`)`` folds as false)
  - signed-decimal template strings with post-sign ideographic-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u300016'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+tab+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\t\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+tab+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\t\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+carriage-return+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\r\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\r\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+carriage-return+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\r\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+tab+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\t\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\n\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+line-feed+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\n\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+line-feed+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\n\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+form-feed+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\f\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+form-feed+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\f\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign vertical-tab+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\v\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign carriage-return+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\r\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign line-feed+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\n\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign carriage-return+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\r\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign line-feed+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\n\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign carriage-return+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\r\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+vertical-tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\v16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+vertical-tab+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\v\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+vertical-tab+carriage-return+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\v\r\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+vertical-tab+line-feed+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\v\n\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+vertical-tab+tab+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\v\t\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\v\n\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\v\r\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+vertical-tab+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\v\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+vertical-tab+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\v\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+carriage-return+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\r\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign form-feed+carriage-return+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\f\r\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign carriage-return+line-feed+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\r\n\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign carriage-return+line-feed+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\r\n\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\r\n\v16'}`)`` folds as false)
  - signed-decimal template strings with post-sign line-feed+carriage-return+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\n\r\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\n\r\v16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+carriage-return+line-feed+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\r\n\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+line-feed+carriage-return+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\n\r\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+line-feed+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\n\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+line-feed+vertical-tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\n\v16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+carriage-return+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\r\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+form-feed+vertical-tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\f\v16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+vertical-tab+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\v\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+vertical-tab+form-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\v\f16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+vertical-tab+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\v\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+form-feed+line-feed whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\f\n16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+form-feed+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\f\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+carriage-return+vertical-tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\r\v16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\n\r\v16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\r\n\v16'}`)`` folds as false)
  - signed-decimal template strings with post-sign tab+line-feed+carriage-return whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\t\n\r16'}`)`` folds as false)
  - signed-decimal template strings with post-sign line-feed+carriage-return+tab whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\n\r\t16'}`)`` folds as false)
  - signed-decimal template strings with post-sign em-space whitespace also fold as non-equal
    (e.g. ``(16n == `${'+\u200316'}`)`` folds as false)
  - signed non-decimal radix strings follow JS `StringToBigInt` behavior and fold as non-equal
    (e.g. `(16n == '+0x10')` folds as false)
  - signed non-decimal radix template strings also follow JS `StringToBigInt` behavior and fold as non-equal
    (e.g. ``(16n == `${'+0x10'}`)`` folds as false)
  - foldable equality conditionals over unary-not booleans
    (e.g. `module[(((!0) === true) ? 'require' : 'nope')]((((!0) === true) ? 'react/compiler-runtime' : 'nope'))`)
  - foldable relational conditionals (`<`, `<=`, `>`, `>=`) over static primitive operands
    (e.g. `module[((('b' > 'a') ? 'require' : 'nope'))]((((1 < 2) ? 'react/compiler-runtime' : 'nope')))` )
  - static BigInt-to-BigInt relational conditionals
    (e.g. `module[(((2n > 1n) ? 'require' : 'nope'))]((((1n < 2n) ? 'react/compiler-runtime' : 'nope')))` )
  - static BigInt-to-Number relational conditionals when number side is a foldable safe integer
    (e.g. `module[(((2 > 1n) ? 'require' : 'nope'))]((((1n < 2) ? 'react/compiler-runtime' : 'nope')))` )
  - foldable fractional BigInt-vs-Number relational conditionals
    (e.g. `module[(((1.5 < 2n) ? 'require' : 'nope'))]((((1n < 1.5) ? 'react/compiler-runtime' : 'nope')))` )
  - BigInt-vs-Number relational conditionals with foldable `Infinity` / `-Infinity` / `NaN` number expressions
    (e.g. `module[(((1n < (+'not-a-number')) ? 'nope' : 'require'))]((((1n < (+'Infinity')) ? 'react/compiler-runtime' : 'nope')))` )
  - unary-minus BigInt truthiness in conditionals
    (e.g. `module[(((-1n) ? 'require' : 'nope'))]((((-1n) ? 'react/compiler-runtime' : 'nope')))` )
  - foldable `typeof` conditionals where operand type is statically known
    (e.g. `module[(((typeof (() => 1)) === 'function') ? 'require' : 'nope')]((((typeof 1) === 'number') ? 'react/compiler-runtime' : 'nope'))`)
  - `typeof` folding also covers foldable sequence/logical/nullish operand wrappers
    (e.g. `typeof ((0, 1))`, `typeof (false || (() => 1))`, `typeof (null ?? 1)`)
  - `typeof` folding also covers static binary-expression operands
    (e.g. `typeof (1 + 2)`, `typeof ('a' + 1)`, `typeof (1 < 2)`)
  - `typeof` folding includes static BigInt binary forms when both operands are BigInt
    (e.g. `typeof (1n + 2n)`)
  - `typeof` folding includes unary-minus BigInt forms
    (e.g. `typeof (-1n)`)
  - truthy array/object literal conditional aliases
    (e.g. `module[(({a: 1}) ? 'require' : 'nope')](([1] ? 'react/compiler-runtime' : 'nope'))`)
  - truthy function/class literal conditional aliases
    (e.g. `module[((function(){}) ? 'require' : 'nope')](((class C {}) ? 'react/compiler-runtime' : 'nope'))`)
  - truthy arrow-function/regex literal conditional aliases
    (e.g. `module[((() => 1) ? 'require' : 'nope')](((/x/) ? 'react/compiler-runtime' : 'nope'))`)
  - template-literal conditional aliases with truthy/falsy branch folding
    (e.g. `module[(`${'x'}` ? 'require' : 'nope')]((`${''}` ? 'nope' : 'react/compiler-runtime'))`)
  - nullish aliases with foldable `void` literals treated as nullish
    (e.g. `module[(void false ?? 'require')]((void false ?? 'react/compiler-runtime'))`)
  - numeric truthy/falsy logical aliases with foldable numeric literals
    (e.g. `module[(1 && 'require')]((0 || 'react/compiler-runtime'))`)
  - unary numeric truthy/falsy logical aliases
    (e.g. `module[((-1) && 'require')]((+0 || 'react/compiler-runtime'))`)
  - unary numeric coercion aliases over foldable booleans/null
    (e.g. `module[(+true && 'require')]((+false || 'react/compiler-runtime'))`)
  - unary numeric coercion aliases over foldable string literals
    (e.g. `module[(+'1' && 'require')]((+'' || 'react/compiler-runtime'))`)
  - large radix numeric string coercions follow JS Number semantics (not `u64`-bounded parsing)
    (e.g. `(+'0x10000000000000000' && 'require')`, `(+'0b10000000000000000000000000000000000000000000000000000000000000000' && 'require')`, `(+'0o2000000000000000000000' && 'require')` remain truthy)
  - unary numeric coercion aliases over invalid numeric strings (`NaN` folding)
    (e.g. `module[(+'1' && 'require')]((+'not-a-number' || 'react/compiler-runtime'))`)
  - unary numeric coercion aliases over foldable template-string literals
    (e.g. `module[(+\`${'1'}\` && 'require')]((+\`${''}\` || 'react/compiler-runtime'))`)
  - unary numeric coercion aliases over invalid/radix template-string values
    (e.g. `module[(+\`${'1'}\` && 'require')]((+\`${'not-a-number'}\` || 'react/compiler-runtime'))`)
  - large-radix template-string coercions follow JS Number semantics too
    (e.g. `module[(+\`${'0b10000000000000000000000000000000000000000000000000000000000000000'}\` && 'require')]((+\`${'0o2000000000000000000000'}\` && 'react/compiler-runtime'))`)
  - unary numeric coercion aliases over radix/whitespace numeric strings
    (e.g. `module[(+'0b1' && 'require')]((+'  ' || 'react/compiler-runtime'))`)
  - unary numeric coercion aliases honor JS BOM trimming for string numerics
    (e.g. `module[((+'\uFEFF1' && 'require'))](((+'\uFEFF1' && 'react/compiler-runtime')))` )
  - unary template-string numeric coercion aliases also honor JS BOM trimming
    (e.g. `module[((+\`${'\uFEFF1'}\` && 'require'))](((+\`${'\uFEFF1'}\` && 'react/compiler-runtime')))` )
  - signed decimal numeric strings also fold as finite numbers
    (e.g. `module[((+'+16' && 'require'))](((+'+16' && 'react/compiler-runtime')))` )
  - space-wrapped signed decimal numeric strings also fold as finite numbers
    (e.g. `module[((+' +16 ' && 'require'))](((+' +16 ' && 'react/compiler-runtime')))` )
  - BOM-wrapped signed decimal numeric strings also fold as finite numbers
    (e.g. `module[((+'\uFEFF+16\uFEFF' && 'require'))](((+'\uFEFF+16\uFEFF' && 'react/compiler-runtime')))` )
  - line-separator-wrapped signed decimal numeric strings also fold as finite numbers
    (e.g. `module[((+'\u2028+16\u2029' && 'require'))](((+'\u2028+16\u2029' && 'react/compiler-runtime')))` )
  - signed decimal numeric-string coercions with post-sign line-separator whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u202816' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign paragraph-separator whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u202916' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign NBSP whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u00A016' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign Ogham-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u168016' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign en-quad whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200016' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign em-quad whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200116' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign en-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200216' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign three-per-em-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200416' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign four-per-em-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200516' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign six-per-em-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200616' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign figure-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200716' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign punctuation-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200816' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign thin-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200916' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign hair-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200A16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign narrow-no-break-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u202F16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign medium-mathematical-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u205F16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign ideographic-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u300016' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+tab+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\t\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\t\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+carriage-return+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\r\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\r\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\r\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\t\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+line-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\n\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+line-feed+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\n\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\n\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\f\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\f\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign vertical-tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\v\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\r\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign line-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\n\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign carriage-return+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\r\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\n\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\r\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\v16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\v\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+carriage-return+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\v\r\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+line-feed+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\v\n\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\v\t\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+line-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\v\n\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\v\r\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\v\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\v\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\r\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign form-feed+carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\f\r\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign carriage-return+line-feed+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\r\n\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign carriage-return+line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\r\n\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign carriage-return+line-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\r\n\v16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign line-feed+carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\n\r\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign line-feed+carriage-return+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\n\r\v16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+carriage-return+line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\r\n\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+line-feed+carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\n\r\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\n\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+line-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\n\v16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\r\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+form-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\f\v16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+vertical-tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\v\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+vertical-tab+form-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\v\f16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+vertical-tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\v\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+form-feed+line-feed whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\f\n16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+form-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\f\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+carriage-return+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\r\v16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+line-feed+carriage-return+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\n\r\v16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+carriage-return+line-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\r\n\v16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign tab+line-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\t\n\r16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign line-feed+carriage-return+tab whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\n\r\t16' || 'react/compiler-runtime'))`)
  - signed decimal numeric-string coercions with post-sign em-space whitespace also fold as `NaN`
    (e.g. `module[(+1 && 'require')]((+'+\u200316' || 'react/compiler-runtime'))`)
  - signed non-decimal radix numeric strings (hex/binary/octal) follow JS `Number` coercion (`NaN`)
    (e.g. `module[(+1 && 'require')]((+'+0x10' || 'react/compiler-runtime'))`,
    `(+'+0b10' || 'react/compiler-runtime')`, `(+'+0o10' || 'react/compiler-runtime')`)
  - signed non-decimal radix template-string numeric coercions also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+0x10'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions also fold as finite numbers
    (e.g. `module[((+\`${'+16'}\` && 'require'))](((+\`${'+16'}\` && 'react/compiler-runtime')))` )
  - space-wrapped signed decimal template-string numeric coercions also fold as finite numbers
    (e.g. `module[((+\`${' +16 '}\` && 'require'))](((+\`${' +16 '}\` && 'react/compiler-runtime')))` )
  - BOM-wrapped signed decimal template-string numeric coercions also fold as finite numbers
    (e.g. `module[((+\`${'\uFEFF+16\uFEFF'}\` && 'require'))](((+\`${'\uFEFF+16\uFEFF'}\` && 'react/compiler-runtime')))` )
  - line-separator-wrapped signed decimal template-string numeric coercions also fold as finite numbers
    (e.g. `module[((+\`${'\u2028+16\u2029'}\` && 'require'))](((+\`${'\u2028+16\u2029'}\` && 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+ 16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign BOM/Unicode whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\uFEFF16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign line-separator whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u202816'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign paragraph-separator whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u202916'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign NBSP whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u00A016'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign Ogham-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u168016'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign en-quad whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200016'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign em-quad whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200116'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign en-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200216'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign three-per-em-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200416'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign four-per-em-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200516'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign six-per-em-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200616'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign figure-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200716'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign punctuation-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200816'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign thin-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200916'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign hair-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200A16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign narrow-no-break-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u202F16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign medium-mathematical-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u205F16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign ideographic-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u300016'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+tab+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+carriage-return+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\r\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\r\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\r\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+line-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\n\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+line-feed+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\n\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\n\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign vertical-tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign line-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign carriage-return+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+carriage-return+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\r\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+line-feed+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\n\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\t\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+line-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\n\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\r\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign form-feed+carriage-return+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\r\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign carriage-return+line-feed+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\n\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign carriage-return+line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\n\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign carriage-return+line-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\n\v16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign line-feed+carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\r\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign line-feed+carriage-return+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\r\v16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+carriage-return+line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\n\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+line-feed+carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\r\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+line-feed+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+line-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\v16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+carriage-return+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+form-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\v16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+vertical-tab+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\v\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+vertical-tab+form-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\v\f16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+vertical-tab+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\v\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+form-feed+line-feed whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\n16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+form-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+carriage-return+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\v16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+line-feed+carriage-return+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\r\v16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+carriage-return+line-feed+vertical-tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\n\v16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign tab+line-feed+carriage-return whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\r16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign line-feed+carriage-return+tab whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\r\t16'}\` || 'react/compiler-runtime')))` )
  - signed decimal template-string numeric coercions with post-sign em-space whitespace also fold as `NaN`
    (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200316'}\` || 'react/compiler-runtime')))` )
  - unary numeric coercion aliases over hex/octal/infinity numeric strings
    (e.g. `module[(+'Infinity' && 'require')]((+'0o0' || 'react/compiler-runtime'))`)
  - unary numeric coercion aliases over foldable conditional expressions
    (e.g. `module[((+(true ? true : false)) && 'require')](((+(true ? false : true)) || 'react/compiler-runtime'))`)
  - arithmetic numeric truthy/falsy logical aliases
    (e.g. `module[(((3 - 2) && 'require'))]((((2 - 2) || 'react/compiler-runtime')))`).
  - arithmetic non-finite numeric logical aliases
    (e.g. `module[(((1 / 0) && 'require'))]((((0 / 0) || 'react/compiler-runtime')))`).
  - bitwise/shift numeric truthy/falsy logical aliases
    (e.g. `module[(((1 << 1) && 'require'))]((((1 ^ 1) || 'react/compiler-runtime')))`).
  - signed-shift/bitand and zero-fill-shift numeric truthy/falsy aliases
    (e.g. `module[(((-2 >> 1) && 'require'))]((((1 & 0) || 'react/compiler-runtime')))` and `module[(((-1 >>> 0) && 'require'))]((((1 >>> 1) || 'react/compiler-runtime')))`).
  - fractional bitwise operands fold with JS int32 coercion semantics
    (e.g. `module[(((1.9 | 0) && 'require'))]((((1.9 ^ 1.9) || 'react/compiler-runtime')))`).
  - shift-count masking follows JS low-5-bit rules in bitwise/shift logical aliases
    (e.g. `module[(((-1 << 33) && 'require'))]((((1 >>> 33) || 'react/compiler-runtime')))`).
  - signed-right-shift masking and negative shift counts also fold via JS shift-count rules
    (e.g. `module[(((-1 >> 33) && 'require'))]((((1 >>> -1) || 'react/compiler-runtime')))`).
  - bitwise int32 wraparound boundaries fold with JS ToInt32 semantics
    (e.g. `module[(((-4294967297 | 0) && 'require'))]((((4294967296 | 0) || 'react/compiler-runtime')))`).
  - large decimal bitwise operands near float precision boundaries fold consistently
    (e.g. `module[(((1e20 | 0) && 'require'))]((((9007199254740992 | 0) || 'react/compiler-runtime')))`).
  - exponential numeric-string bitwise coercions fold with JS ToInt32 behavior too
    (e.g. `module[(((+'1e21' | 0) && 'require'))]((((+'1e30' | 0) || 'react/compiler-runtime')))`).
  - non-finite numeric bitwise coercions fold with JS ToInt32 behavior
    (e.g. `module[(((+'not-a-number' ^ 1) && 'require'))]((((+'Infinity' | 0) || 'react/compiler-runtime')))`).
  - bigint truthy/falsy logical aliases with foldable bigint literals
    (e.g. `module[(1n && 'require')]((0n || 'react/compiler-runtime'))`)
  - object destructure aliases (e.g. `const { c: cache } = runtime`)
  - defaulted object destructure aliases (e.g. `const { c: cache = fallback } = runtime`, `const { ['\\x63']: c = fallback } = runtime`, `const { c = fallback } = require(...)`, `const { ['\\x63']: c = fallback } = module['\\x72equire'](...)`, `const runtime = module['\\x72equire'](...); const { c = fallback } = runtime`)
  - assignment destructure aliases (e.g. `({ c: cache = fallback } = runtime)`, `({ c = fallback } = runtime)`, `({ c = fallback } = require(...))`, `({ ['\\x63']: c = fallback } = (0, require)(...))`, `({ c = fallback } = (0, globalThis.module['\\x72equire'])(...))`, `({ ['\\x63']: c = fallback } = (0, globalThis.module['\\x72equire'])(...))`, `({ ['\\u0063']: cache = fallback } = module['\\x72equire'](...))`, `({ ['\\u0063']: cache = fallback } = window.module['\\x72equire'](...))`, `({ c: cache = fallback } = (0, require)(...))`, `({ ['\\u0063']: cache = fallback } = (0, self.module['\\x72equire'])(...))`, `({ ['\\u0063']: cache = fallback } = (0, global['\\u006dodule']['\\x72equire'])(...))`, `({ ['\\u0063']: cache = fallback } = (0, global.self.module['\\x72equire'])(...))`, `({ ['\\u0063']: cache = fallback } = (0, global.global.self.module['\\x72equire'])(...))`, `({ ['\\u0063']: cache = fallback } = (0, globalThis.globalThis.window.module['\\x72equire'])(...))`, `({ ['\\u0063']: cache = fallback } = (0, globalThis['\\u006dodule']['\\x72equire'])(...))`)
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

