# React Compiler Rust Workspace

This directory hosts the Rust implementation of the React Compiler.

## Current crates

- `react_compiler_core`: Rust-native parser/frontend scaffold for compiler input processing.
- `react_compiler_cli`: JSON-over-stdin/stdout executable bridge around `react_compiler_core`.

## Commands

Run from `compiler/`:

```sh
yarn rust:check
yarn rust:test
yarn rust:fmt
```

## Notes

- The workspace pins the Rust toolchain to `stable` via `rust-toolchain.toml`.
- Initial frontend parsing is implemented with SWC parser primitives as the foundation for replacing Babel-dependent frontend logic.
