# React Compiler Rust Workspace

This directory hosts the Rust implementation of the React Compiler.

## Current crates

- `react_compiler_core`: Rust compiler core for parsing, transforming, and emitting compiled source.
- `react_compiler_cli`: JSON-over-stdin/stdout bridge executable used by the Babel integration layer.

## Commands

Run from `compiler/`:

```sh
yarn rust:check
yarn rust:test
yarn rust:fmt
```

To execute compiler fixture runs through the Rust engine path:

```sh
yarn snap -p simple
```

## Notes

- The workspace pins the Rust toolchain to `stable` via `rust-toolchain.toml`.
- Parsing and code generation are implemented using SWC parser/codegen primitives.
