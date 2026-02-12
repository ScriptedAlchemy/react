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
  - optional selected runtime callee name fields must be non-empty when present
  - deterministic string-list telemetry fields are validated as non-empty entries, sorted, and duplicate-free
  - transformed/skipped function lists must form an exact disjoint partition of
    `placeholder_transform_candidates`
  - detected component/hook name lists must align with kind-partitioned `react_functions`
  - placeholder component/hook split counts are validated against name lists
    (using `react_functions` kind data, with hook-name fallback heuristics for unmatched names)
  - `placeholder_transform_candidates` must include all detected `react_functions` names
  - each detected react function name must appear in transformed-or-skipped placeholder partitions
  - `react_functions` must be deterministically ordered by location/name/kind and
    must not contain duplicate entries or duplicate name/kind pairs
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
  Static template-expression forms are resolved when interpolations are literal-like
  (e.g. `module[\`requ\${'ire'}\`](\`react/compiler-\${'runtime'}\`)`).
  Static binary-string concatenation forms are also resolved
  (e.g. `module['requ' + 'ire']('react/compiler-' + 'runtime')`).
  Static literal-conditional forms are resolved when branches are foldable
  (e.g. `module[true ? 'require' : 'nope'](true ? 'react/compiler-runtime' : 'nope')`).
  Static literal-driven logical/nullish forms are also resolved
  (e.g. `module[(true && 'require') ?? 'nope']((null ?? 'react/compiler-runtime'))`).
  Static sequence-literal forms are resolved when sequence entries are foldable
  (e.g. `module[(0, 'require')]((0, 'react/compiler-runtime'))`).
  Conditional tests may also use foldable truthy/falsy literals
  (e.g. `module['x' ? 'require' : 'nope'](void false ? 'nope' : 'react/compiler-runtime')`).
  Truthy array/object literal conditions are folded as expected
  (e.g. `module[(({a: 1}) ? 'require' : 'nope')](([1] ? 'react/compiler-runtime' : 'nope'))`).
  Truthy function/class literal conditions are folded as expected
  (e.g. `module[((function(){}) ? 'require' : 'nope')](((class C {}) ? 'react/compiler-runtime' : 'nope'))`).
  Truthy arrow-function/regex literal conditions are folded as expected
  (e.g. `module[((() => 1) ? 'require' : 'nope')](((/x/) ? 'react/compiler-runtime' : 'nope'))`).
  Foldable template-literal conditions are supported too (truthy and falsy branches)
  (e.g. `module[\`${'x'}\` ? 'require' : 'nope'](\`${''}\` ? 'nope' : 'react/compiler-runtime')`).
  Nullish checks treat foldable `void` literals as nullish values
  (e.g. `module[(void false ?? 'require')]((void false ?? 'react/compiler-runtime'))`).
  Foldable numeric truthy/falsy conditions are also supported
  (e.g. `module[(1 && 'require')]((0 || 'react/compiler-runtime'))`).
  Unary numeric forms are folded in the same truthy/falsy logic
  (e.g. `module[((-1) && 'require')]((+0 || 'react/compiler-runtime'))`).
  Unary numeric coercions over foldable booleans/null are supported
  (e.g. `module[(+true && 'require')]((+false || 'react/compiler-runtime'))`).
  Unary numeric coercions over foldable string literals are supported
  (e.g. `module[(+'1' && 'require')]((+'' || 'react/compiler-runtime'))`).
  Radix and whitespace string-numeric coercions are folded too
  (e.g. `module[(+'0b1' && 'require')]((+'  ' || 'react/compiler-runtime'))`).
  Hex/octal/infinity numeric-string coercions are folded too
  (e.g. `module[(+'Infinity' && 'require')]((+'0o0' || 'react/compiler-runtime'))`).
  Unary numeric coercions over foldable conditionals are supported too
  (e.g. `module[((+(true ? true : false)) && 'require')](((+(true ? false : true)) || 'react/compiler-runtime'))`).
  Arithmetic numeric forms are folded before truthy/falsy checks
  (e.g. `module[(((3 - 2) && 'require'))]((((2 - 2) || 'react/compiler-runtime')))`).
  Foldable bigint truthy/falsy conditions are supported as well
  (e.g. `module[(1n && 'require')]((0n || 'react/compiler-runtime'))`).
  Destructured `c` aliases with default patterns are recognized too
  (e.g. `const {c: cache = fallback} = runtime`, `const {['\\x63']: c = fallback} = runtime`, `const {c = fallback} = require(...)`, `const {['\\x63']: c = fallback} = module['\\x72equire'](...)`).
  Assignment-destructure aliases are recognized too
  (e.g. `({c: cache = fallback} = runtime)`, `({c = fallback} = runtime)`, `({['\\x63']: c = fallback} = (0, require)(...))`, `({['\\x63']: c = fallback} = (0, globalThis.module['\\x72equire'])(...))`).
  Escaped/unicode `require` keys are supported across these roots
  (e.g. `global.module['\\u0072equire'](...)`).

You can find usage documentation here: https://react.dev/learn/react-compiler
