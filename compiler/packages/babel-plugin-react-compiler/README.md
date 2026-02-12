# babel-plugin-react-compiler

React Compiler is a compiler that optimizes React applications, ensuring that only the minimal parts of components and hooks will re-render when state changes. The compiler also validates that components and hooks follow the Rules of React.

This package contains the React Compiler Babel plugin used in projects that run Babel transforms.
The plugin is now a thin bridge to the Rust React Compiler backend.

## Notes for internal consumers

- Import compiler types/helpers from `babel-plugin-react-compiler/src` (root export surface).
- Avoid deep imports into internal source subpaths.
- Rust CLI execution can be configured with:
  - `REACT_COMPILER_RUST_CLI_BIN` (explicit binary path, or bare command name resolved/validated via PATH), or
  - `REACT_COMPILER_RUST_USE_PREBUILT_BIN=1` (truthy values `1|true|yes|on`; use prebuilt `react_compiler_cli` when present).
  - `REACT_COMPILER_RUST_PREBUILT_PROFILE` (`debug` or `release`) to force a prebuilt profile when using prebuilt mode.
  - Without a forced prebuilt profile, the bridge checks `target/debug` then `target/release`.
  - With a forced prebuilt profile, missing binaries are treated as hard errors (no Cargo fallback).
  - `REACT_COMPILER_RUST_MANIFEST` (authoritative manifest override; must resolve to an existing `Cargo.toml` file path).
  - Path-form overrides support `~` home-directory expansion.
- Optional Rust CLI timeout override:
  - `REACT_COMPILER_RUST_CLI_TIMEOUT_MS` (positive integer milliseconds, default `60000`).
- Optional Rust CLI stdout/stderr buffer override:
  - `REACT_COMPILER_RUST_CLI_MAX_BUFFER_BYTES` (positive integer bytes, default `67108864`).
- Compatibility logger integration:
  - bridge emits `CompileSuccess`, `CompileError`, and `PipelineError` events
  - `CompileSuccess` is emitted per detected React function with source location metadata when available
  - each `CompileSuccess` event also includes transform telemetry (`statementCount`, `statementCountAfterTransform`, `placeholderTransformsApplied`, `placeholderTransformStatus`, `detectedReactFunctions`)
  - when `logger.debugLogIRs` is present, bridge emits both `RustFrontendDebugIR` and a structured `RustFrontendCompileMetadata` debug payload
  - Rust error categories are normalized for downstream lint consumers (`syntax -> Syntax`, `internal/request -> Invariant`).
  - Rust warning/hint/off severities are preserved in mapped compile-error detail when available.
  - compile-error detail payloads include the Rust error `code` for downstream handling.
  - explicit event type aliases are exported for consumers (`CompileSuccessEvent`, `CompileErrorEvent`, `CompileDiagnosticEvent`, `PipelineErrorEvent`).
- Rust bridge response validation accepts:
  - `ok` payload metadata contract fields with strict type/count checks
    (numeric counters, boolean state flags, string-array telemetry lists)
  - telemetry count fields must match their corresponding list lengths
  - telemetry aggregate relationships must hold (candidate = transformed + skipped,
    component/hook splits sum correctly, detected component + hook = detected total)
  - transformed statement count must be monotonic (`statementCountAfterTransform >= statementCount`)
  - runtime helper import/callee flags are cross-validated for coherence
    (import count monotonicity, helper-added delta checks, generated vs reused exclusivity)
  - runtime callee names/candidates are cross-validated
    (selected callee names must appear in candidate lists; transformed output requires post-transform callee)
  - deterministic string-list telemetry fields are validated as non-empty entries, sorted, and duplicate-free
  - transformed/skipped function lists must form an exact disjoint partition of
    `placeholder_transform_candidates`
  - detected component/hook name lists must align with kind-partitioned `react_functions`
  - `placeholder_transform_status` must be one of
    `disabled | no_candidates | transformed | blocked_missing_runtime_callee | no_op`,
    with status/count consistency checks
  - `error.category` in `{request, syntax, internal}`
  - `error.severity` in `{error, warning, hint, off}`
  - required response string fields must be present and non-empty
  - request payload validation for runtime consumers:
    `source` string required; optional `filename` non-empty string;
    optional flags (`is_module`, `apply_placeholder_transforms`, `emit_debug_ir`) must be booleans
  - explicitly provided request `protocol_version` must be a non-negative integer
    (numeric strings are coerced and validated)
- Dialect selection:
  - TypeScript extensions (`.ts`, `.tsx`, `.mts`, `.cts`) map to Rust TypeScript mode
  - Flow file hints (`.flow` extension or Babel `flow` parser plugin) map to Rust Flow mode
  - all other inputs default to JavaScript mode
- Runtime helper alias reuse supports CommonJS/global patterns including
  `require(...)`, `module.require(...)`, and global module roots
  (`globalThis`, `global`, `window`, `self`) across member/computed/sequence chains.
  Destructured `c` aliases with default patterns are recognized too
  (e.g. `const {c: cache = fallback} = runtime`, `const {['c']: cache = fallback} = runtime`).
  Assignment-destructure aliases are recognized too
  (e.g. `({c: cache = fallback} = runtime)`, `({c: cache = fallback} = require(...))`).
  Escaped/unicode `require` keys are supported across these roots
  (e.g. `global.module['\\u0072equire'](...)`).

You can find usage documentation here: https://react.dev/learn/react-compiler
