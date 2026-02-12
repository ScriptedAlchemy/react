# babel-plugin-react-compiler

React Compiler is a compiler that optimizes React applications, ensuring that only the minimal parts of components and hooks will re-render when state changes. The compiler also validates that components and hooks follow the Rules of React.

This package contains the React Compiler Babel plugin used in projects that run Babel transforms.
The plugin is now a thin bridge to the Rust React Compiler backend.

## Notes for internal consumers

- Import compiler types/helpers from `babel-plugin-react-compiler/src` (root export surface).
- Avoid deep imports into internal source subpaths.
- Rust CLI execution can be configured with:
  - `REACT_COMPILER_RUST_CLI_BIN` (explicit binary path, or bare command name resolved via PATH), or
  - `REACT_COMPILER_RUST_USE_PREBUILT_BIN=1` (use `compiler/rust/target/debug/react_compiler_cli` when present).
  - `REACT_COMPILER_RUST_MANIFEST` (authoritative manifest override; must resolve to an existing `Cargo.toml` file path).
  - Path-form overrides support `~` home-directory expansion.
- Optional Rust CLI timeout override:
  - `REACT_COMPILER_RUST_CLI_TIMEOUT_MS` (positive integer milliseconds, default `60000`).
- Optional Rust CLI stdout/stderr buffer override:
  - `REACT_COMPILER_RUST_CLI_MAX_BUFFER_BYTES` (positive integer bytes, default `67108864`).
- Compatibility logger integration:
  - bridge emits `CompileSuccess`, `CompileError`, and `PipelineError` events
  - `CompileSuccess` is emitted per detected React function with source location metadata when available
  - Rust error categories are normalized for downstream lint consumers (`syntax -> Syntax`, `internal/request -> Invariant`).
  - Rust warning/hint/off severities are preserved in mapped compile-error detail when available.
  - compile-error detail payloads include the Rust error `code` for downstream handling.
  - explicit event type aliases are exported for consumers (`CompileSuccessEvent`, `CompileErrorEvent`, `CompileDiagnosticEvent`, `PipelineErrorEvent`).
- Rust bridge response validation accepts:
  - `error.category` in `{request, syntax, internal}`
  - `error.severity` in `{error, warning, hint, off}`
- Dialect selection:
  - TypeScript extensions (`.ts`, `.tsx`, `.mts`, `.cts`) map to Rust TypeScript mode
  - Flow file hints (`.flow` extension or Babel `flow` parser plugin) map to Rust Flow mode
  - all other inputs default to JavaScript mode
- Runtime helper alias reuse supports CommonJS/global patterns including
  `require(...)`, `module.require(...)`, and global module roots
  (`globalThis`, `global`, `window`, `self`) across member/computed chains.

You can find usage documentation here: https://react.dev/learn/react-compiler
