# React Compiler (Rust-first) Knowledge Base

This workspace now uses a Rust-first compiler implementation, with a thin Babel bridge.

## Current architecture

- `packages/babel-plugin-react-compiler/`
  - `src/Babel/BabelPlugin.ts` — plugin entrypoint (always Rust path)
  - `src/Babel/RustFrontend.ts` — Rust CLI orchestration + AST replacement + compatibility logger events
  - `src/RustBridge/RustCli.ts` — CLI request/response + protocol validation
  - `src/Compat/LegacyApi.ts` — compatibility types/exports consumed by downstream packages
- `rust/react_compiler_core/` — Rust compiler core
- `rust/react_compiler_cli/` — stdin/stdout JSON bridge executable

## Practical commands

Run from `compiler/`:

```bash
yarn rust:check
yarn rust:test
yarn workspace babel-plugin-react-compiler build
yarn workspace snap build
yarn workspace eslint-plugin-react-compiler build
yarn build
```

## Snap harness notes

- Snap commands still exist for fixture/evaluator workflows.
- The old JS/Babel-vs-Rust engine toggles are removed from active execution.
- Use plain `yarn snap ...` commands (no engine environment flags).

## Compatibility surface guidance

The plugin keeps a **thin** compatibility export surface in `LegacyApi.ts` for downstream
consumers (eslint plugin, snap, MCP server, healthcheck). Prefer importing from:

- `babel-plugin-react-compiler/src`

Avoid adding new deep imports to internal paths under `src/*`.

## Debugging guidance

When debugging Rust bridge behavior:

1. Verify CLI invocation and protocol handling in `RustCli.ts`.
2. Verify AST replacement flow in `RustFrontend.ts`.
3. Confirm consumer builds still pass:
   - `yarn workspace snap build`
   - `yarn workspace eslint-plugin-react-compiler build`
   - `yarn workspace react-compiler-healthcheck build`

## Version control

Use Git in this repository:

```bash
git status
git add -A
git commit -m "message"
git push -u origin <branch>
```
