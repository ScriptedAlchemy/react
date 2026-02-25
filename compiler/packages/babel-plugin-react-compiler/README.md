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
  Strict equality/inequality literal comparisons in conditionals are folded too
  (e.g. `module[(('require' === 'require') ? 'require' : 'nope')]((('runtime' === 'runtime') ? 'react/compiler-runtime' : 'nope'))`).
  Strict equality folding also preserves JavaScript `NaN` behavior (`NaN === NaN` is false),
  allowing deterministic fallback branch selection in foldable conditionals.
  Foldable loose equality/inequality (`==` / `!=`) conditionals are also recognized,
  including primitive coercions and `null == undefined` checks
  (e.g. `module[((true == 1) ? 'require' : 'nope')](((null == (void false)) ? 'react/compiler-runtime' : 'nope'))`).
  Foldable BigInt loose equality with static number/string operands is supported too
  (e.g. `(1n == 1)`, `(1n == '1')`).
  Radix-prefixed BigInt strings are also folded in loose equality
  (e.g. `(16n == '0x10')`, `(16n == '0b10000')`, `(16n == '0o20')`).
  Uppercase prefixes and surrounding whitespace are folded too
  (e.g. `(16n == ' 0X10 ')`, `(16n == ' 0B10000 ')`, `(16n == ' 0O20 ')`).
  BOM-wrapped radix-prefixed BigInt strings are also folded
  (e.g. `(16n == '\uFEFF0x10\uFEFF')`, `(16n == '\uFEFF0b10000\uFEFF')`, `(16n == '\uFEFF0o20\uFEFF')`).
  BOM-wrapped radix-prefixed BigInt template strings are folded too
  (e.g. ``(16n == `${'\uFEFF0x10\uFEFF'}`)``, ``(16n == `${'\uFEFF0b10000\uFEFF'}`)``, ``(16n == `${'\uFEFF0o20\uFEFF'}`)``).
  Signed decimal BigInt strings are folded too (e.g. `(16n == '+16')`).
  Space-wrapped signed decimal BigInt strings are folded too (e.g. `(16n == ' +16 ')`).
  BOM-wrapped signed decimal BigInt strings are folded too
  (e.g. `(16n == '\uFEFF+16\uFEFF')`).
  Line-separator-wrapped signed decimal BigInt strings are folded too
  (e.g. `(16n == '\u2028+16\u2029')`).
  Signed-decimal BigInt template strings are folded too (e.g. ``(16n == `${'+16'}`)``).
  Space-wrapped signed-decimal BigInt template strings are folded too
  (e.g. ``(16n == `${' +16 '}`)``).
  BOM-wrapped signed-decimal BigInt template strings are folded too
  (e.g. ``(16n == `${'\uFEFF+16\uFEFF'}`)``).
  Line-separator-wrapped signed-decimal BigInt template strings are folded too
  (e.g. ``(16n == `${'\u2028+16\u2029'}`)``).
  Signed decimals with whitespace after the sign follow JavaScript behavior and fold as non-equal
  (e.g. `(16n == '+ 16')` is folded as false).
  Signed-decimal template strings with post-sign whitespace also fold as non-equal
  (e.g. ``(16n == `${'+ 16'}`)`` is folded as false).
  Signed decimals with JS BOM/Unicode whitespace after the sign also fold as non-equal
  (e.g. `(16n == '+\uFEFF16')` is folded as false).
  Signed decimals with post-sign line-separator whitespace also fold as non-equal
  (e.g. `(16n == '+\u202816')` is folded as false).
  Signed decimals with post-sign paragraph-separator whitespace also fold as non-equal
  (e.g. `(16n == '+\u202916')` is folded as false).
  Signed decimals with post-sign NBSP whitespace also fold as non-equal
  (e.g. `(16n == '+\u00A016')` is folded as false).
  Signed decimals with post-sign Ogham-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u168016')` is folded as false).
  Signed decimals with post-sign en-quad whitespace also fold as non-equal
  (e.g. `(16n == '+\u200016')` is folded as false).
  Signed decimals with post-sign em-quad whitespace also fold as non-equal
  (e.g. `(16n == '+\u200116')` is folded as false).
  Signed decimals with post-sign en-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u200216')` is folded as false).
  Signed decimals with post-sign three-per-em-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u200416')` is folded as false).
  Signed decimals with post-sign four-per-em-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u200516')` is folded as false).
  Signed decimals with post-sign six-per-em-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u200616')` is folded as false).
  Signed decimals with post-sign figure-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u200716')` is folded as false).
  Signed decimals with post-sign punctuation-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u200816')` is folded as false).
  Signed decimals with post-sign thin-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u200916')` is folded as false).
  Signed decimals with post-sign hair-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u200A16')` is folded as false).
  Signed decimals with post-sign narrow-no-break-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u202F16')` is folded as false).
  Signed decimals with post-sign medium-mathematical-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u205F16')` is folded as false).
  Signed decimals with post-sign ideographic-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u300016')` is folded as false).
  Signed decimals with post-sign tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t16')` is folded as false).
  Signed decimals with post-sign line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\n16')` is folded as false).
  Signed decimals with post-sign carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\r16')` is folded as false).
  Signed decimals with post-sign form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f16')` is folded as false).
  Signed decimals with post-sign vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\v16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t\f16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t\r16')` is folded as false).
  Signed decimals with post-sign vertical-tab+carriage-return+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\v\r\t16')` is folded as false).
  Signed decimals with post-sign vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\r\n16')` is folded as false).
  Signed decimals with post-sign vertical-tab+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\r\f16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t\n16')` is folded as false).
  Signed decimals with post-sign vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\v\n\r16')` is folded as false).
  Signed decimals with post-sign vertical-tab+line-feed+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\v\n\t16')` is folded as false).
  Signed decimals with post-sign vertical-tab+line-feed+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\n\f16')` is folded as false).
  Signed decimals with post-sign vertical-tab+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\f16')` is folded as false).
  Signed decimals with post-sign vertical-tab+form-feed+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\f\n16')` is folded as false).
  Signed decimals with post-sign vertical-tab+form-feed+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\v\f\t16')` is folded as false).
  Signed decimals with post-sign vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\n16')` is folded as false).
  Signed decimals with post-sign vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\v\r16')` is folded as false).
  Signed decimals with post-sign carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\r\n16')` is folded as false).
  Signed decimals with post-sign line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\n\r16')` is folded as false).
  Signed decimals with post-sign tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\n16')` is folded as false).
  Signed decimals with post-sign tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\t\r16')` is folded as false).
  Signed decimals with post-sign carriage-return+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\r\t16')` is folded as false).
  Signed decimals with post-sign line-feed+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\n\f16')` is folded as false).
  Signed decimals with post-sign form-feed+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f\n16')` is folded as false).
  Signed decimals with post-sign carriage-return+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\r\f16')` is folded as false).
  Signed decimals with post-sign form-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\f\r16')` is folded as false).
  Signed decimals with post-sign tab+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f16')` is folded as false).
  Signed decimals with post-sign form-feed+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\f\t16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\r16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+carriage-return+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\r\t16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+line-feed+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\n\t16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\t\r16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\t\n16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\t\n\r16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+line-feed+tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\n\t\r16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+line-feed+carriage-return+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\n\r\t16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\t\r\n16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+carriage-return+tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\r\t\n16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+carriage-return+line-feed+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\r\n\t16')` is folded as false).
  Signed decimals with post-sign vertical-tab+form-feed+carriage-return+line-feed+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\v\f\r\n\t16')` is folded as false).
  Signed decimals with post-sign vertical-tab+form-feed+carriage-return+tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\f\r\t\n16')` is folded as false).
  Signed decimals with post-sign vertical-tab+form-feed+line-feed+carriage-return+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\v\f\n\r\t16')` is folded as false).
  Signed decimals with post-sign vertical-tab+form-feed+line-feed+tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\v\f\n\t\r16')` is folded as false).
  Signed decimals with post-sign vertical-tab+form-feed+tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\f\t\r\n16')` is folded as false).
  Signed decimals with post-sign vertical-tab+form-feed+tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\v\f\t\n\r16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\n\r16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\r\n16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\n16')` is folded as false).
  Signed decimals with post-sign form-feed+vertical-tab+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\f\v\t16')` is folded as false).
  Signed decimals with post-sign tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\r\n16')` is folded as false).
  Signed decimals with post-sign form-feed+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f\r\n16')` is folded as false).
  Signed decimals with post-sign carriage-return+line-feed+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\r\n\t16')` is folded as false).
  Signed decimals with post-sign carriage-return+line-feed+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\r\n\f16')` is folded as false).
  Signed decimals with post-sign carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\r\n\v16')` is folded as false).
  Signed decimals with post-sign line-feed+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\n\r\f16')` is folded as false).
  Signed decimals with post-sign line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\n\r\v16')` is folded as false).
  Signed decimals with post-sign tab+carriage-return+line-feed+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\r\n\f16')` is folded as false).
  Signed decimals with post-sign tab+line-feed+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\n\r\f16')` is folded as false).
  Signed decimals with post-sign tab+line-feed+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\n\f16')` is folded as false).
  Signed decimals with post-sign tab+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t\n\v16')` is folded as false).
  Signed decimals with post-sign tab+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\r\f16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\v16')` is folded as false).
  Signed decimals with post-sign tab+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\t\v\r16')` is folded as false).
  Signed decimals with post-sign tab+vertical-tab+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\v\f16')` is folded as false).
  Signed decimals with post-sign tab+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\v\n16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\n16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\n\v16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\v\n16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\v\n\r16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\r16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\r\v16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\v\r16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\r\n16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\r\n\v16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+carriage-return+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\r\v\n16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\n\r\v16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+line-feed+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\n\v\r16')` is folded as false).
  Signed decimals with post-sign form-feed+tab+carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\f\t\r\n\v16')` is folded as false).
  Signed decimals with post-sign form-feed+tab+carriage-return+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f\t\r\v\n16')` is folded as false).
  Signed decimals with post-sign form-feed+tab+line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\f\t\n\r\v16')` is folded as false).
  Signed decimals with post-sign form-feed+tab+line-feed+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\f\t\n\v\r16')` is folded as false).
  Signed decimals with post-sign form-feed+tab+vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\f\t\v\r\n16')` is folded as false).
  Signed decimals with post-sign form-feed+tab+vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\f\t\v\n\r16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab+form-feed+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t\f\n\r16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab+form-feed+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t\f\r\n16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab+line-feed+form-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t\n\f\r16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab+line-feed+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t\n\r\f16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab+carriage-return+form-feed+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t\r\f\n16')` is folded as false).
  Signed decimals with post-sign vertical-tab+tab+carriage-return+line-feed+form-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\v\t\r\n\f16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\v\r\n16')` is folded as false).
  Signed decimals with post-sign tab+form-feed+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\t\f\n\r16')` is folded as false).
  Signed decimals with post-sign tab+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t\r\v16')` is folded as false).
  Signed decimals with post-sign tab+line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t\n\r\v16')` is folded as false).
  Signed decimals with post-sign tab+carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. `(16n == '+\t\r\n\v16')` is folded as false).
  Signed decimals with post-sign tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. `(16n == '+\t\n\r16')` is folded as false).
  Signed decimals with post-sign line-feed+carriage-return+tab whitespace also fold as non-equal
  (e.g. `(16n == '+\n\r\t16')` is folded as false).
  Signed decimals with post-sign em-space whitespace also fold as non-equal
  (e.g. `(16n == '+\u200316')` is folded as false).
  Signed-decimal template strings with post-sign BOM/Unicode whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\uFEFF16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign line-separator whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u202816'}`)`` is folded as false).
  Signed-decimal template strings with post-sign paragraph-separator whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u202916'}`)`` is folded as false).
  Signed-decimal template strings with post-sign NBSP whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u00A016'}`)`` is folded as false).
  Signed-decimal template strings with post-sign Ogham-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u168016'}`)`` is folded as false).
  Signed-decimal template strings with post-sign en-quad whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200016'}`)`` is folded as false).
  Signed-decimal template strings with post-sign em-quad whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200116'}`)`` is folded as false).
  Signed-decimal template strings with post-sign en-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200216'}`)`` is folded as false).
  Signed-decimal template strings with post-sign three-per-em-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200416'}`)`` is folded as false).
  Signed-decimal template strings with post-sign four-per-em-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200516'}`)`` is folded as false).
  Signed-decimal template strings with post-sign six-per-em-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200616'}`)`` is folded as false).
  Signed-decimal template strings with post-sign figure-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200716'}`)`` is folded as false).
  Signed-decimal template strings with post-sign punctuation-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200816'}`)`` is folded as false).
  Signed-decimal template strings with post-sign thin-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200916'}`)`` is folded as false).
  Signed-decimal template strings with post-sign hair-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200A16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign narrow-no-break-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u202F16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign medium-mathematical-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u205F16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign ideographic-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u300016'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+carriage-return+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\r\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\r\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+line-feed+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\n\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+line-feed+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\n\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+form-feed+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\f\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+form-feed+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\f\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign carriage-return+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\r\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign line-feed+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\n\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign carriage-return+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\r\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+carriage-return+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\r\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+line-feed+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\n\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\t\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\t\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\t\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+line-feed+tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\n\t\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+line-feed+carriage-return+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\n\r\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\t\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+carriage-return+tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\r\t\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+carriage-return+line-feed+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\r\n\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+form-feed+carriage-return+line-feed+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\f\r\n\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+form-feed+carriage-return+tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\f\r\t\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+form-feed+line-feed+carriage-return+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\f\n\r\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+form-feed+line-feed+tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\f\n\t\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+form-feed+tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\f\t\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+form-feed+tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\f\t\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+vertical-tab+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\v\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign carriage-return+line-feed+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\r\n\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign carriage-return+line-feed+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\r\n\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\r\n\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign line-feed+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\n\r\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\n\r\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+carriage-return+line-feed+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\r\n\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+line-feed+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\n\r\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+line-feed+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\n\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\n\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\r\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\v\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+vertical-tab+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\v\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\v\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\n\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\v\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\v\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\r\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\v\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\r\n\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+carriage-return+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\r\v\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\n\r\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+line-feed+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\n\v\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+tab+carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\t\r\n\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+tab+carriage-return+vertical-tab+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\t\r\v\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+tab+line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\t\n\r\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+tab+line-feed+vertical-tab+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\t\n\v\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+tab+vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\t\v\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign form-feed+tab+vertical-tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\f\t\v\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab+form-feed+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t\f\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab+form-feed+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t\f\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab+line-feed+form-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t\n\f\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab+line-feed+carriage-return+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t\n\r\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab+carriage-return+form-feed+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t\r\f\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign vertical-tab+tab+carriage-return+line-feed+form-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\v\t\r\n\f16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+vertical-tab+carriage-return+line-feed whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\v\r\n16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+form-feed+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\f\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\r\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+line-feed+carriage-return+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\n\r\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+carriage-return+line-feed+vertical-tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\r\n\v16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign tab+line-feed+carriage-return whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\t\n\r16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign line-feed+carriage-return+tab whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\n\r\t16'}`)`` is folded as false).
  Signed-decimal template strings with post-sign em-space whitespace also fold as non-equal
  (e.g. ``(16n == `${'+\u200316'}`)`` is folded as false).
  Signed non-decimal radix strings follow JavaScript behavior and do not fold as equal
  (e.g. `(16n == '+0x10')` is folded as false).
  Signed non-decimal radix template strings also follow JavaScript behavior and
  do not fold as equal (e.g. ``(16n == `${'+0x10'}`)`` is folded as false).
  Equality conditionals over foldable unary-not booleans are also folded
  (e.g. `module[(((!0) === true) ? 'require' : 'nope')]((((!0) === true) ? 'react/compiler-runtime' : 'nope'))`).
  Foldable relational comparisons (`<`, `<=`, `>`, `>=`) are also supported for
  static string and numeric comparisons used in conditional alias selection.
  Static BigInt-to-BigInt relational comparisons are folded too
  (e.g. `(1n < 2n)`, `(2n > 1n)` in conditional alias selection).
  Static BigInt-to-Number relational comparisons are folded when the number side is a
  statically safe integer (e.g. `(1n < 2)`, `(2 > 1n)`).
  Foldable fractional BigInt-vs-Number comparisons are also supported
  (e.g. `(1n < 1.5)`, `(1.5 < 2n)`).
  Foldable BigInt-vs-Number comparisons also handle `Infinity`, `-Infinity`, and `NaN`
  number expressions (e.g. `(1n < +'Infinity')`, `(1n < +'not-a-number')`).
  Unary-minus BigInt truthiness in conditionals is folded too
  (e.g. `((-1n) ? 'require' : 'nope')`).
  Foldable `typeof`-based conditionals are also supported when operand type is
  statically known (e.g. `module[(((typeof (() => 1)) === 'function') ? 'require' : 'nope')]((((typeof 1) === 'number') ? 'react/compiler-runtime' : 'nope'))`).
  This includes foldable sequence/logical/nullish wrappers inside `typeof`
  (e.g. `typeof ((0, 1))`, `typeof (false || (() => 1))`, `typeof (null ?? 1)`).
  Foldable `typeof` checks over static binary expressions are also handled,
  including arithmetic/string and comparison forms
  (e.g. `typeof (1 + 2)`, `typeof ('a' + 1)`, `typeof (1 < 2)`).
  BigInt binary forms are included when both operands are statically BigInt
  (e.g. `typeof (1n + 2n)` resolves to `'bigint'`).
  Unary-minus BigInt `typeof` checks are folded as `'bigint'` too
  (e.g. `typeof (-1n)`).
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
  Large radix numeric strings are folded using JavaScript number semantics too
  (e.g. `(+'0x10000000000000000' && 'require')`, `(+'0b10000000000000000000000000000000000000000000000000000000000000000' && 'require')`, `(+'0o2000000000000000000000' && 'require')` stay truthy instead of folding as `NaN`).
  Invalid numeric strings are folded as `NaN` in unary coercions
  (e.g. `module[(+'1' && 'require')]((+'not-a-number' || 'react/compiler-runtime'))`).
  Unary numeric coercions over foldable template-string literals are supported
  (e.g. `module[(+\`${'1'}\` && 'require')]((+\`${''}\` || 'react/compiler-runtime'))`).
  Template-string numeric coercions also cover invalid/radix values
  (e.g. `module[(+\`${'1'}\` && 'require')]((+\`${'not-a-number'}\` || 'react/compiler-runtime'))`).
  Large radix template-string coercions follow the same JS number semantics
  (e.g. `module[(+\`${'0b10000000000000000000000000000000000000000000000000000000000000000'}\` && 'require')]((+\`${'0o2000000000000000000000'}\` && 'react/compiler-runtime'))`).
  Radix and whitespace string-numeric coercions are folded too
  (e.g. `module[(+'0b1' && 'require')]((+'  ' || 'react/compiler-runtime'))`).
  String numeric coercions also honor JS BOM trimming (e.g. `+'\uFEFF1'` is folded as `1`)
  in logical alias checks.
  Template-string numeric coercions also honor BOM trimming
  (e.g. `module[((+\`${'\uFEFF1'}\` && 'require'))](((+\`${'\uFEFF1'}\` && 'react/compiler-runtime')))`).
  Signed decimal numeric strings also fold as finite numbers
  (e.g. `module[((+'+16' && 'require'))](((+'+16' && 'react/compiler-runtime')))`).
  Space-wrapped signed decimal numeric strings also fold as finite numbers
  (e.g. `module[((+' +16 ' && 'require'))](((+' +16 ' && 'react/compiler-runtime')))`).
  BOM-wrapped signed decimal numeric strings also fold as finite numbers
  (e.g. `module[((+'\uFEFF+16\uFEFF' && 'require'))](((+'\uFEFF+16\uFEFF' && 'react/compiler-runtime')))`).
  Line-separator-wrapped signed decimal numeric strings also fold as finite numbers
  (e.g. `module[((+'\u2028+16\u2029' && 'require'))](((+'\u2028+16\u2029' && 'react/compiler-runtime')))`).
  Signed decimal numeric-string coercions with post-sign line-separator whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u202816' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign paragraph-separator whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u202916' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign NBSP whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u00A016' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign Ogham-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u168016' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign en-quad whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200016' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign em-quad whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200116' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign en-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200216' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign three-per-em-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200416' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign four-per-em-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200516' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign six-per-em-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200616' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign figure-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200716' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign punctuation-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200816' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign thin-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200916' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign hair-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200A16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign narrow-no-break-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u202F16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign medium-mathematical-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u205F16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign ideographic-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u300016' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\r\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\r\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\n\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\n\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\f\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\f\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\r\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\n\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\r\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\r\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\n\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\t\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\t\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\t\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+line-feed+tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\n\t\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+line-feed+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\n\r\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\t\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+carriage-return+tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\r\t\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+carriage-return+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\r\n\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+carriage-return+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\f\r\n\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+carriage-return+tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\f\r\t\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+line-feed+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\f\n\r\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+line-feed+tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\f\n\t\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\f\t\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+form-feed+tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\f\t\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+vertical-tab+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\v\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign carriage-return+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\r\n\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign carriage-return+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\r\n\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign carriage-return+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\r\n\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign line-feed+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\n\r\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign line-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\n\r\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+carriage-return+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\r\n\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+line-feed+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\n\r\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\n\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\n\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\r\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\v\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+vertical-tab+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\v\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\v\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\n\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\v\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+vertical-tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\v\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\r\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\v\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+carriage-return+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\r\n\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+carriage-return+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\r\v\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+line-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\n\r\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+line-feed+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\n\v\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+tab+carriage-return+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\t\r\n\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+tab+carriage-return+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\t\r\v\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+tab+line-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\t\n\r\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+tab+line-feed+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\t\n\v\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+tab+vertical-tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\t\v\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign form-feed+tab+vertical-tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\f\t\v\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab+form-feed+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t\f\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab+form-feed+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t\f\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab+line-feed+form-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t\n\f\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab+line-feed+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t\n\r\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab+carriage-return+form-feed+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t\r\f\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign vertical-tab+tab+carriage-return+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\v\t\r\n\f16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+vertical-tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\v\r\n16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+form-feed+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\f\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\r\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+line-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\n\r\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+carriage-return+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\r\n\v16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\t\n\r16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign line-feed+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\n\r\t16' || 'react/compiler-runtime'))`).
  Signed decimal numeric-string coercions with post-sign em-space whitespace
  also fold as `NaN`
  (e.g. `module[(+1 && 'require')]((+'+\u200316' || 'react/compiler-runtime'))`).
  Signed non-decimal radix numeric strings (hex/binary/octal) follow JS `Number`
  behavior and coerce to `NaN`
  (e.g. `module[(+1 && 'require')]((+'+0x10' || 'react/compiler-runtime'))`,
  `(+'+0b10' || 'react/compiler-runtime')`, `(+'+0o10' || 'react/compiler-runtime')`).
  Signed non-decimal radix template-string numeric coercions also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+0x10'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions also fold as finite numbers
  (e.g. `module[((+\`${'+16'}\` && 'require'))](((+\`${'+16'}\` && 'react/compiler-runtime')))`).
  Space-wrapped signed decimal template-string numeric coercions also fold as finite numbers
  (e.g. `module[((+\`${' +16 '}\` && 'require'))](((+\`${' +16 '}\` && 'react/compiler-runtime')))`).
  BOM-wrapped signed decimal template-string numeric coercions also fold as finite numbers
  (e.g. `module[((+\`${'\uFEFF+16\uFEFF'}\` && 'require'))](((+\`${'\uFEFF+16\uFEFF'}\` && 'react/compiler-runtime')))`).
  Line-separator-wrapped signed decimal template-string numeric coercions also fold as finite numbers
  (e.g. `module[((+\`${'\u2028+16\u2029'}\` && 'require'))](((+\`${'\u2028+16\u2029'}\` && 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign whitespace also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+ 16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign BOM/Unicode whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\uFEFF16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign line-separator whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u202816'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign paragraph-separator whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u202916'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign NBSP whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u00A016'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign Ogham-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u168016'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign en-quad whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200016'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign em-quad whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200116'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign en-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200216'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign three-per-em-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200416'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign four-per-em-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200516'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign six-per-em-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200616'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign figure-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200716'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign punctuation-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200816'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign thin-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200916'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign hair-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200A16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign narrow-no-break-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u202F16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign medium-mathematical-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u205F16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign ideographic-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u300016'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\r\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\r\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\n\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\n\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\r\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\n\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\t\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\t\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\t\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+line-feed+tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\n\t\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+line-feed+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\n\r\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\t\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+carriage-return+tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\r\t\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+carriage-return+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\r\n\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+carriage-return+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\r\n\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+carriage-return+tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\r\t\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+line-feed+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\n\r\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+line-feed+tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\n\t\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\t\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+form-feed+tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\f\t\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+vertical-tab+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\v\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign carriage-return+line-feed+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\n\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign carriage-return+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\n\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign carriage-return+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\r\n\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign line-feed+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\r\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign line-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\r\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+carriage-return+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\n\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+line-feed+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\r\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\v\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+vertical-tab+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\v\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\v\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\n\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\v\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+vertical-tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\v\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\r\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\v\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+carriage-return+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\r\n\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+carriage-return+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\r\v\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+line-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\n\r\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+line-feed+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\n\v\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+tab+carriage-return+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\t\r\n\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+tab+carriage-return+vertical-tab+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\t\r\v\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+tab+line-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\t\n\r\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+tab+line-feed+vertical-tab+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\t\n\v\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+tab+vertical-tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\t\v\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign form-feed+tab+vertical-tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\f\t\v\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab+form-feed+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\f\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab+form-feed+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\f\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab+line-feed+form-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\n\f\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab+line-feed+carriage-return+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\n\r\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab+carriage-return+form-feed+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\r\f\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign vertical-tab+tab+carriage-return+line-feed+form-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\v\t\r\n\f16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+vertical-tab+carriage-return+line-feed whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\v\r\n16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+form-feed+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\f\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+line-feed+carriage-return+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\r\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+carriage-return+line-feed+vertical-tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\r\n\v16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign tab+line-feed+carriage-return whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\t\n\r16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign line-feed+carriage-return+tab whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\n\r\t16'}\` || 'react/compiler-runtime')))`).
  Signed decimal template-string numeric coercions with post-sign em-space whitespace
  also fold as `NaN`
  (e.g. `module[((+\`${'1'}\` && 'require'))](((+\`${'+\u200316'}\` || 'react/compiler-runtime')))`).
  Hex/octal/infinity numeric-string coercions are folded too
  (e.g. `module[(+'Infinity' && 'require')]((+'0o0' || 'react/compiler-runtime'))`).
  Unary numeric coercions over foldable conditionals are supported too
  (e.g. `module[((+(true ? true : false)) && 'require')](((+(true ? false : true)) || 'react/compiler-runtime'))`).
  Arithmetic numeric forms are folded before truthy/falsy checks
  (e.g. `module[(((3 - 2) && 'require'))]((((2 - 2) || 'react/compiler-runtime')))`).
  Non-finite arithmetic results in those logical folds are handled too
  (e.g. `module[(((1 / 0) && 'require'))]((((0 / 0) || 'react/compiler-runtime')))`).
  Bitwise/shift numeric forms are folded before truthy/falsy checks too
  (e.g. `module[(((1 << 1) && 'require'))]((((1 ^ 1) || 'react/compiler-runtime')))`).
  Signed-shift/bitand fallbacks and zero-fill shift branches are also folded
  (e.g. `module[(((-2 >> 1) && 'require'))]((((1 & 0) || 'react/compiler-runtime')))` and `module[(((-1 >>> 0) && 'require'))]((((1 >>> 1) || 'react/compiler-runtime')))`).
  Fractional bitwise inputs are folded using JavaScript’s int32 coercions
  (e.g. `module[(((1.9 | 0) && 'require'))]((((1.9 ^ 1.9) || 'react/compiler-runtime')))`).
  Shift-count masking follows JavaScript’s low-5-bit behavior too
  (e.g. `module[(((-1 << 33) && 'require'))]((((1 >>> 33) || 'react/compiler-runtime')))`).
  Signed-right-shift masking and negative shift counts are folded too
  (e.g. `module[(((-1 >> 33) && 'require'))]((((1 >>> -1) || 'react/compiler-runtime')))`).
  Bitwise int32 wraparound boundaries are folded with JavaScript semantics
  (e.g. `module[(((-4294967297 | 0) && 'require'))]((((4294967296 | 0) || 'react/compiler-runtime')))`).
  Large decimal bitwise operands near float precision boundaries are folded too
  (e.g. `module[(((1e20 | 0) && 'require'))]((((9007199254740992 | 0) || 'react/compiler-runtime')))`).
  Exponential numeric-string bitwise coercions are folded with JS ToInt32 semantics too
  (e.g. `module[(((+'1e21' | 0) && 'require'))]((((+'1e30' | 0) || 'react/compiler-runtime')))`).
  Non-finite numeric bitwise coercions are folded with JS ToInt32 semantics
  (e.g. `module[(((+'not-a-number' ^ 1) && 'require'))]((((+'Infinity' | 0) || 'react/compiler-runtime')))`).
  Foldable bigint truthy/falsy conditions are supported as well
  (e.g. `module[(1n && 'require')]((0n || 'react/compiler-runtime'))`).
  Destructured `c` aliases with default patterns are recognized too
  (e.g. `const {c: cache = fallback} = runtime`, `const {['\\x63']: c = fallback} = runtime`, `const {c = fallback} = require(...)`, `const {['\\x63']: c = fallback} = module['\\x72equire'](...)`).
  Assignment-destructure aliases are recognized too
  (e.g. `({c: cache = fallback} = runtime)`, `({c = fallback} = runtime)`, `({['\\x63']: c = fallback} = (0, require)(...))`, `({['\\x63']: c = fallback} = (0, globalThis.module['\\x72equire'])(...))`).
  Escaped/unicode `require` keys are supported across these roots
  (e.g. `global.module['\\u0072equire'](...)`).

You can find usage documentation here: https://react.dev/learn/react-compiler
