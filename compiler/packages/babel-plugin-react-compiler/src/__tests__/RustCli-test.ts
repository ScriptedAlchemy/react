/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import {runRustCompilerCli} from '../RustBridge/RustCli';
import {runBabelPluginReactCompiler} from '../Babel/RunReactCompilerBabelPlugin';
import {spawnSync} from 'child_process';
import * as BabelParser from '@babel/parser';
import generate from '@babel/generator';

const hasCargo = spawnSync('cargo', ['--version'], {
  encoding: 'utf-8',
}).status === 0;

const describeWithCargo = hasCargo ? describe : describe.skip;

function withStrictRustEngine<T>(fn: () => T): T {
  const previous = process.env['REACT_COMPILER_RUST_STRICT'];
  process.env['REACT_COMPILER_RUST_STRICT'] = '1';
  try {
    return fn();
  } finally {
    if (previous == null) {
      delete process.env['REACT_COMPILER_RUST_STRICT'];
    } else {
      process.env['REACT_COMPILER_RUST_STRICT'] = previous;
    }
  }
}

function withEnvVar<T>(name: string, value: string, fn: () => T): T {
  const previous = process.env[name];
  process.env[name] = value;
  try {
    return fn();
  } finally {
    if (previous == null) {
      delete process.env[name];
    } else {
      process.env[name] = previous;
    }
  }
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function canonicalizeCode(code: string | null | undefined): string {
  const source = code ?? '';
  try {
    const ast = BabelParser.parse(source, {
      sourceType: 'module',
      plugins: ['typescript', 'jsx'],
    });
    return (
      generate(ast, {
        comments: false,
        compact: true,
        minified: true,
        retainLines: false,
      }).code ?? ''
    );
  } catch {
    return source.replace(/\s+/g, ' ').trim();
  }
}

describeWithCargo('Rust compiler CLI bridge', () => {
  it('parses JavaScript input through the Rust frontend', () => {
    const result = runRustCompilerCli({
      source: 'export const value = 1;',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.statement_count).toBe(1);
      expect(result.detected_react_functions).toBe(0);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions).toEqual([]);
      expect(result.code).toContain('export const value = 1;');
    }
  });

  it('returns detected React function metadata from Rust frontend', () => {
    const result = runRustCompilerCli({
      source: 'function useValue() { return 1; }',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.react_functions).toHaveLength(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('useValue');
      expect(result.react_functions[0]?.kind).toBe('Hook');
      expect(result.react_functions[0]?.loc).not.toBeNull();
    }
  });

  it('detects react function metadata from assignment expressions', () => {
    const result = runRustCompilerCli({
      source: 'let Component; Component = () => <div />;',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('Component');
      expect(result.react_functions[0]?.kind).toBe('Component');
      expect(result.react_functions[0]?.loc).not.toBeNull();
    }
  });

  it('returns structured rust error code for unsupported flow syntax', () => {
    const result = runRustCompilerCli({
      source: '// @flow\nfunction Component(props: {x: number}) { return props.x; }',
      dialect: 'flow',
      filename: 'fixture.js',
      is_module: false,
    });

    expect(result.status).toBe('error');
    if (result.status === 'error') {
      expect(result.code).toBe('unsupported_flow_syntax');
      expect(result.category).toBe('syntax');
      expect(result.reason).toBe('flow_syntax_not_supported');
      expect(result.severity).toBe('error');
    }
  });

  it('returns parse diagnostics metadata for JavaScript parse failures', () => {
    const result = runRustCompilerCli({
      source: 'const = 1;',
      dialect: 'javascript',
      filename: 'broken.js',
      is_module: false,
    });

    expect(result.status).toBe('error');
    if (result.status === 'error') {
      expect(result.code).toBe('parse_failure');
      expect(result.category).toBe('syntax');
      expect(result.reason).toBe('unexpected_token');
      expect(result.severity).toBe('error');
      expect(result.location).not.toBeNull();
    }
  });

  it('accepts flow dialect when source uses plain JavaScript syntax', () => {
    const result = runRustCompilerCli({
      source: '/* @flow */\nfunction Component() { return <div />; }',
      dialect: 'flow',
      filename: 'fixture.js',
      is_module: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('Component');
    }
  });

  it('resolves fixture entrypoint aliases to underlying function bindings', () => {
    const result = runRustCompilerCli({
      source:
        'function component() { return 1; } const alias = component; export const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('component');
    }
  });

  it('resolves fixture entrypoint computed object keys', () => {
    const result = runRustCompilerCli({
      source:
        "function component() { return 1; } export const FIXTURE_ENTRYPOINT = { ['fn']: component, params: [] };",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('component');
    }
  });

  it('resolves fixture entrypoint shorthand object keys', () => {
    const result = runRustCompilerCli({
      source:
        'function component() { return 1; } const fn = component; export const FIXTURE_ENTRYPOINT = { fn, params: [] };',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('component');
    }
  });

  it('resolves fixture entrypoint assigned aliases to underlying function bindings', () => {
    const result = runRustCompilerCli({
      source:
        'function component() { return 1; } let alias; alias = component; export const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('component');
    }
  });

  it('resolves fixture entrypoint computed member assignments', () => {
    const result = runRustCompilerCli({
      source:
        "function component() { return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT['fn'] = component;",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('component');
    }
  });

  it('resolves fixture entrypoint assignments to object literals', () => {
    const result = runRustCompilerCli({
      source:
        'function component() { return 1; } let FIXTURE_ENTRYPOINT; FIXTURE_ENTRYPOINT = { fn: component, params: [] };',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('component');
    }
  });

  it('resolves fixture entrypoint member assignments', () => {
    const result = runRustCompilerCli({
      source:
        'function component() { return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT.fn = component;',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('component');
    }
  });

  it('resolves fixture entrypoint assigned function-expression bindings', () => {
    const result = runRustCompilerCli({
      source:
        'let alias; alias = () => 1; export const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('alias');
    }
  });

  it('resolves fixture entrypoint assigned function-expression bindings with component names', () => {
    const result = runRustCompilerCli({
      source:
        'let component; component = () => 1; export const FIXTURE_ENTRYPOINT = { fn: component, params: [] };',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: false,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.react_functions[0]?.name).toBe('component');
    }
  });

  it('can request placeholder transforms explicitly from Rust CLI', () => {
    const result = runRustCompilerCli({
      source: 'export function Component() { return <div />; }',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.placeholder_transforms_applied).toBeGreaterThan(0);
      expect(result.placeholder_transformed_functions).toEqual(['Component']);
      expect(result.placeholder_runtime_callee_name_before_transform).toBeUndefined();
      expect(
        result.placeholder_runtime_callee_candidates_before_transform,
      ).toEqual([]);
      expect(result.placeholder_runtime_callee_name).toBe('_c');
      expect(result.placeholder_runtime_callee_candidates).toEqual(['_c']);
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('reuses module runtime require member aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "const cache = require('react/compiler-runtime').c; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.placeholder_runtime_callee_name_before_transform).toBe(
        'cache',
      );
      expect(result.placeholder_runtime_callee_candidates_before_transform).toEqual(
        ['cache'],
      );
      expect(result.placeholder_runtime_callee_name).toBe('cache');
      expect(result.placeholder_runtime_callee_candidates).toEqual(['cache']);
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime require namespace aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "const runtime = require('react/compiler-runtime'); const cache = runtime.c; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime namespace import aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "import * as runtime from 'react/compiler-runtime'; const cache = runtime.c; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime default import aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "import runtime from 'react/compiler-runtime'; const cache = runtime.c; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime namespace destructure assignment aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "import * as runtime from 'react/compiler-runtime'; let cache; ({ c: cache } = runtime); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime require destructure assignment aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "let runtime; runtime = require('react/compiler-runtime'); let cache; ({ c: cache } = runtime); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime shorthand destructure aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "const runtime = require('react/compiler-runtime'); const { c } = runtime; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = c(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module exported runtime require member aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "export const cache = require('react/compiler-runtime').c; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module exported runtime require namespace aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "export const runtime = require('react/compiler-runtime'); export const cache = runtime.c; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime import aliases through identifier aliases for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "import { c as cache0 } from 'react/compiler-runtime'; const cache = cache0; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses shadowed runtime callee aliases without treating them as namespaces in modules', () => {
    const result = runRustCompilerCli({
      source:
        "const runtime = require('react/compiler-runtime'); const { c: runtime } = require('react/compiler-runtime'); const cache = runtime.c; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = runtime(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime require aliases through assignments for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "const { c: cache0 } = require('react/compiler-runtime'); let cache; cache = cache0; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime aliases from sequence assignments for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "let cache; (cache = require('react/compiler-runtime').c, sideEffect()); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime aliases from sequence assignment rhs values for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "let cache; cache = (sideEffect(), require('react/compiler-runtime').c); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime aliases from call argument assignments for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "let cache; sideEffect(cache = require('react/compiler-runtime').c); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('reuses module runtime aliases from sequence declarators for placeholder transforms', () => {
    const result = runRustCompilerCli({
      source:
        "const cache = (sideEffect(), require('react/compiler-runtime').c); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
      expect(result.code).not.toContain('import { c as _c }');
    }
  });

  it('falls back to generated module runtime import when runtime alias is reassigned', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cache = unknown; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('import { c as _c }');
      expect(result.code).toContain('const $ = _c(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
    }
  });

  it('falls back to generated module runtime import when runtime alias uses compound assignment', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cache += 1; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('import { c as _c }');
      expect(result.code).toContain('const $ = _c(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
    }
  });

  it('falls back to generated module runtime import when runtime alias uses logical assignment', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cache &&= unknown; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('import { c as _c }');
      expect(result.code).toContain('const $ = _c(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
    }
  });

  it('falls back to generated module runtime import when runtime alias uses update expression', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cache++; export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('import { c as _c }');
      expect(result.code).toContain('const $ = _c(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
    }
  });

  it('falls back to generated module runtime import when runtime alias is overwritten by object-pattern assignment', () => {
    const result = runRustCompilerCli({
      source:
        "let cache; ({ c: cache } = require('react/compiler-runtime')); ({ x: cache } = source); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('import { c as _c }');
      expect(result.code).toContain('const $ = _c(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
    }
  });

  it('falls back to generated module runtime import when runtime alias is reassigned in sequence', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; (cache = unknown, sideEffect()); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('import { c as _c }');
      expect(result.code).toContain('const $ = _c(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
    }
  });

  it('falls back to generated module runtime import when runtime alias is conditionally reassigned', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cond && (cache = unknown); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('import { c as _c }');
      expect(result.code).toContain('const $ = _c(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
    }
  });

  it('falls back to generated module runtime import when runtime alias is reassigned in call arguments', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; sideEffect(cache = unknown); export function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('import { c as _c }');
      expect(result.code).toContain('const $ = _c(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with runtime require destructure aliases', () => {
    const result = runRustCompilerCli({
      source:
        "const { c: cache } = require('react/compiler-runtime'); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('does not transform script components when runtime alias is reassigned', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cache = unknown; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('does not transform script components when runtime alias is reassigned in sequence', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; (cache = unknown, sideEffect()); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('does not transform script components when runtime alias is conditionally reassigned', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cond && (cache = unknown); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('does not transform script components when runtime alias is reassigned in call arguments', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; sideEffect(cache = unknown); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('does not transform script components when runtime alias uses compound assignment', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cache += 1; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('does not transform script components when runtime alias uses logical assignment', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cache &&= unknown; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('does not transform script components when runtime alias is deleted', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; delete cache; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('does not transform script components when runtime alias uses update expression', () => {
    const result = runRustCompilerCli({
      source:
        "let cache = require('react/compiler-runtime').c; cache++; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('does not transform script components when runtime alias is overwritten by object-pattern assignment', () => {
    const result = runRustCompilerCli({
      source:
        "let cache; ({ c: cache } = require('react/compiler-runtime')); ({ x: cache } = source); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.code).not.toContain('const $ = cache(0);');
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('transforms script components with runtime require shorthand destructure aliases', () => {
    const result = runRustCompilerCli({
      source:
        "const { c } = require('react/compiler-runtime'); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = c(0);');
    }
  });

  it('transforms script components with runtime alias chains', () => {
    const result = runRustCompilerCli({
      source:
        "const { c: cache0 } = require('react/compiler-runtime'); const cache = cache0; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with shadowed runtime callee aliases', () => {
    const result = runRustCompilerCli({
      source:
        "const runtime = require('react/compiler-runtime'); const { c: runtime } = require('react/compiler-runtime'); const cache = runtime.c; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = runtime(0);');
      expect(result.code).not.toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with runtime assignment alias chains', () => {
    const result = runRustCompilerCli({
      source:
        "const { c: cache0 } = require('react/compiler-runtime'); let cache; cache = cache0; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with runtime aliases from sequence assignments', () => {
    const result = runRustCompilerCli({
      source:
        "let cache; (cache = require('react/compiler-runtime').c, sideEffect()); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with runtime aliases from sequence assignment rhs values', () => {
    const result = runRustCompilerCli({
      source:
        "let cache; cache = (sideEffect(), require('react/compiler-runtime').c); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with runtime aliases from call argument assignments', () => {
    const result = runRustCompilerCli({
      source:
        "let cache; sideEffect(cache = require('react/compiler-runtime').c); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with runtime aliases from sequence declarators', () => {
    const result = runRustCompilerCli({
      source:
        "const cache = (sideEffect(), require('react/compiler-runtime').c); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with runtime namespace member aliases', () => {
    const result = runRustCompilerCli({
      source:
        "const runtime = require('react/compiler-runtime'); const cache = runtime.c; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with assigned runtime namespace member aliases', () => {
    const result = runRustCompilerCli({
      source:
        "let runtime; runtime = require('react/compiler-runtime'); let cache; cache = runtime.c; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with runtime namespace destructure aliases', () => {
    const result = runRustCompilerCli({
      source:
        "const runtime = require('react/compiler-runtime'); const { c: cache } = runtime; function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('transforms script components with assigned runtime destructure aliases', () => {
    const result = runRustCompilerCli({
      source:
        "let runtime; runtime = require('react/compiler-runtime'); let cache; ({ c: cache } = runtime); function Component() { return <div />; }",
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.code).toContain('const $ = cache(0);');
    }
  });

  it('does not transform script components without runtime bindings', () => {
    const result = runRustCompilerCli({
      source: 'function Component() { return <div />; }',
      dialect: 'javascript',
      filename: 'fixture.js',
      is_module: false,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(0);
      expect(result.placeholder_transformed_functions).toEqual([]);
      expect(result.placeholder_runtime_callee_name_before_transform).toBeUndefined();
      expect(
        result.placeholder_runtime_callee_candidates_before_transform,
      ).toEqual([]);
      expect(result.placeholder_runtime_callee_name).toBeUndefined();
      expect(result.placeholder_runtime_callee_candidates).toEqual([]);
      expect(result.code).not.toContain('const $ = _c(0);');
    }
  });

  it('transforms react-like assignment expressions in module Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source: 'let Component; Component = () => <div />; export {Component};',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('Component');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms anonymous default export components in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source: 'export default function () { return <div />; }',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.placeholder_transformed_functions).toEqual([
        '__default_export_component__',
      ]);
      expect(result.placeholder_runtime_callee_name_before_transform).toBeUndefined();
      expect(
        result.placeholder_runtime_callee_candidates_before_transform,
      ).toEqual([]);
      expect(result.placeholder_runtime_callee_name).toBe('_c');
      expect(result.placeholder_runtime_callee_candidates).toEqual(['_c']);
      expect(result.react_functions[0]?.name).toBe('__default_export_component__');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms parenthesized default export arrow components in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source: 'export default (() => <div />);',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('__default_export_component__');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms TypeScript-asserted default export arrow components in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source: 'export default ((() => <div />) as any);',
      dialect: 'typescript',
      filename: 'fixture.tsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('__default_export_component__');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms functions referenced by default export identifiers in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source: 'function component() { return <div />; } export default component;',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('component');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms functions referenced by aliased default export identifiers in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source:
        'function component() { return <div />; } const alias = component; export default alias;',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('component');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms functions referenced by assigned default export identifiers in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source:
        'function component() { return <div />; } let alias; alias = component; export default alias;',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('component');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms assigned function-expression default export identifiers in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source: 'let alias; alias = () => <div />; export default alias;',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('alias');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms assigned function-expression named default export specifiers in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source: 'let component; component = () => <div />; export {component as default};',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('component');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms functions referenced by multi-aliased default export identifiers in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source:
        'function component() { return <div />; } const aliasA = component; const aliasB = aliasA; export default aliasB;',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('component');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('transforms functions referenced by named default export specifiers in Rust CLI mode', () => {
    const result = runRustCompilerCli({
      source: 'function component() { return <div />; } export {component as default};',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: true,
      apply_placeholder_transforms: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.detected_react_functions).toBe(1);
      expect(result.placeholder_transforms_applied).toBe(1);
      expect(result.react_functions[0]?.name).toBe('component');
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
  });

  it('can request debug IR output from Rust CLI', () => {
    const result = runRustCompilerCli({
      source: 'function Component() { return <div />; }',
      dialect: 'javascript',
      filename: 'fixture.jsx',
      is_module: false,
      emit_debug_ir: true,
    });

    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.debug_ir).toContain('ReactiveFunctionsDebug v0');
      expect(result.debug_ir).toContain('name=Component kind=Component');
    }
  });

  it('uses explicit rust cli binary override when configured', () => {
    withEnvVar('REACT_COMPILER_RUST_CLI_BIN', process.execPath, () => {
      expect(() =>
        runRustCompilerCli({
          source: 'const value = 1;',
          dialect: 'javascript',
          filename: 'fixture.js',
          is_module: false,
        }),
      ).toThrow(new RegExp(escapeRegExp(`Rust compiler CLI (${process.execPath})`)));
    });
  });

  it('can be selected as compiler engine in Babel plugin options', () => {
    const rustResult = runBabelPluginReactCompiler(
      'export function Component() { return <div />; }',
      '/fixture.tsx',
      'typescript',
      {
        compilationMode: 'all',
        compilerEngine: 'rust',
      },
    );
    const babelResult = runBabelPluginReactCompiler(
      'export function Component() { return <div />; }',
      '/fixture.tsx',
      'typescript',
      {
        compilationMode: 'all',
        compilerEngine: 'babel',
      },
    );

    expect(canonicalizeCode(rustResult.code)).toBe(
      canonicalizeCode(babelResult.code),
    );
  });

  it('emits Rust frontend debug snapshots via logger debugLogIRs', () => {
    const debugValues: Array<{kind: string; name: string; value: string}> = [];
    runBabelPluginReactCompiler(
      'export function Component() { return <div />; }',
      '/fixture.tsx',
      'typescript',
      {
        compilationMode: 'all',
        compilerEngine: 'rust',
        logger: {
          logEvent() {},
          debugLogIRs(value) {
            if (value.kind === 'debug') {
              debugValues.push({
                kind: value.kind,
                name: value.name,
                value: value.value,
              });
            }
          },
        },
      },
    );

    const rustDebug = debugValues.find(value => value.name === 'RustFrontendDebug');
    expect(rustDebug).toBeDefined();
    expect(rustDebug?.value).toContain('ReactiveFunctionsDebug v0');
    const rustPlaceholderDebug = debugValues.find(
      value => value.name === 'RustFrontendPlaceholderTransforms',
    );
    expect(rustPlaceholderDebug).toBeDefined();
    expect(rustPlaceholderDebug?.value).toBe('');
    const rustRuntimeCalleeDebug = debugValues.find(
      value => value.name === 'RustFrontendRuntimeCallee',
    );
    expect(rustRuntimeCalleeDebug).toBeDefined();
    expect(rustRuntimeCalleeDebug?.value).toBe('');
    const rustRuntimeCalleeBeforeTransformDebug = debugValues.find(
      value => value.name === 'RustFrontendRuntimeCalleeBeforeTransform',
    );
    expect(rustRuntimeCalleeBeforeTransformDebug).toBeDefined();
    expect(rustRuntimeCalleeBeforeTransformDebug?.value).toBe('');
    const rustRuntimeCalleeCandidatesDebug = debugValues.find(
      value => value.name === 'RustFrontendRuntimeCalleeCandidates',
    );
    expect(rustRuntimeCalleeCandidatesDebug).toBeDefined();
    expect(rustRuntimeCalleeCandidatesDebug?.value).toBe('');
    const rustRuntimeCalleeCandidatesBeforeTransformDebug = debugValues.find(
      value =>
        value.name === 'RustFrontendRuntimeCalleeCandidatesBeforeTransform',
    );
    expect(rustRuntimeCalleeCandidatesBeforeTransformDebug).toBeDefined();
    expect(rustRuntimeCalleeCandidatesBeforeTransformDebug?.value).toBe('');
  });

  it('emits runtime callee debug telemetry in strict rust mode', () => {
    const debugValues: Array<{kind: string; name: string; value: string}> = [];
    withStrictRustEngine(() =>
      runBabelPluginReactCompiler(
        'export function Component() { return <div />; }',
        '/fixture.tsx',
        'typescript',
        {
          logger: {
            logEvent() {},
            debugLogIRs(value) {
              if (value.kind === 'debug') {
                debugValues.push({
                  kind: value.kind,
                  name: value.name,
                  value: value.value,
                });
              }
            },
          },
          compilationMode: 'all',
          compilerEngine: 'rust',
        },
      ),
    );

    const placeholderDebug = debugValues.find(
      value => value.name === 'RustFrontendPlaceholderTransforms',
    );
    expect(placeholderDebug).toBeDefined();
    expect(placeholderDebug?.value).toBe('');
    const runtimeCalleeDebug = debugValues.find(
      value => value.name === 'RustFrontendRuntimeCallee',
    );
    expect(runtimeCalleeDebug).toBeDefined();
    expect(runtimeCalleeDebug?.value).toBe('');
    const runtimeCalleeBeforeTransformDebug = debugValues.find(
      value => value.name === 'RustFrontendRuntimeCalleeBeforeTransform',
    );
    expect(runtimeCalleeBeforeTransformDebug).toBeDefined();
    expect(runtimeCalleeBeforeTransformDebug?.value).toBe('');
    const runtimeCalleeCandidatesDebug = debugValues.find(
      value => value.name === 'RustFrontendRuntimeCalleeCandidates',
    );
    expect(runtimeCalleeCandidatesDebug).toBeDefined();
    expect(runtimeCalleeCandidatesDebug?.value).toBe('');
    const runtimeCalleeCandidatesBeforeTransformDebug = debugValues.find(
      value =>
        value.name === 'RustFrontendRuntimeCalleeCandidatesBeforeTransform',
    );
    expect(runtimeCalleeCandidatesBeforeTransformDebug).toBeDefined();
    expect(runtimeCalleeCandidatesBeforeTransformDebug?.value).toBe('');
  });

  it('strict rust mode matches babel output for default export components', () => {
    const rustResult = withStrictRustEngine(() =>
      runBabelPluginReactCompiler(
        'export default function Component() { return <div />; }',
        '/fixture.tsx',
        'typescript',
        {
          compilationMode: 'all',
          compilerEngine: 'rust',
        },
      ),
    );
    const babelResult = runBabelPluginReactCompiler(
      'export default function Component() { return <div />; }',
      '/fixture.tsx',
      'typescript',
      {
        compilationMode: 'all',
        compilerEngine: 'babel',
      },
    );

    expect(canonicalizeCode(rustResult.code)).toBe(
      canonicalizeCode(babelResult.code),
    );
  });

  it('strict rust mode matches babel output with existing cache init', () => {
    const source = [
      "import { c as _c } from 'react/compiler-runtime';",
      'export function Component() {',
      '  const $ = _c(0);',
      '  return <div />;',
      '}',
    ].join('\n');
    const rustResult = withStrictRustEngine(() =>
      runBabelPluginReactCompiler(source, '/fixture.tsx', 'typescript', {
        compilationMode: 'all',
        compilerEngine: 'rust',
      }),
    );
    const babelResult = runBabelPluginReactCompiler(
      source,
      '/fixture.tsx',
      'typescript',
      {
        compilationMode: 'all',
        compilerEngine: 'babel',
      },
    );

    expect(canonicalizeCode(rustResult.code)).toBe(
      canonicalizeCode(babelResult.code),
    );
  });

  it('strict rust mode matches babel output for fixture entrypoint names', () => {
    const source = [
      'function component() {',
      '  return 1;',
      '}',
      'export const FIXTURE_ENTRYPOINT = { fn: component, params: [] };',
    ].join('\n');
    const rustResult = withStrictRustEngine(() =>
      runBabelPluginReactCompiler(source, '/fixture.tsx', 'typescript', {
        compilationMode: 'all',
        compilerEngine: 'rust',
      }),
    );
    const babelResult = runBabelPluginReactCompiler(
      source,
      '/fixture.tsx',
      'typescript',
      {
        compilationMode: 'all',
        compilerEngine: 'babel',
      },
    );

    expect(canonicalizeCode(rustResult.code)).toBe(
      canonicalizeCode(babelResult.code),
    );
  });

  it('strict rust mode falls back to babel path for flow syntax', () => {
    const source = [
      '// @flow',
      'function Component(props: {name: string}) {',
      '  return props.name;',
      '}',
      'export const FIXTURE_ENTRYPOINT = { fn: Component, params: [{name: "A"}] };',
    ].join('\n');
    const rustResult = withStrictRustEngine(() =>
      runBabelPluginReactCompiler(source, '/fixture.js', 'flow', {
        compilationMode: 'all',
        compilerEngine: 'rust',
      }),
    );
    const babelResult = runBabelPluginReactCompiler(
      source,
      '/fixture.js',
      'flow',
      {
        compilationMode: 'all',
        compilerEngine: 'babel',
      },
    );

    expect(canonicalizeCode(rustResult.code)).toBe(
      canonicalizeCode(babelResult.code),
    );
  });

  it('logs strict rust fallback location for recoverable flow frontend errors', () => {
    const loggedEvents: Array<unknown> = [];
    const source = [
      '// @flow',
      'function Component(props: {name: string}) {',
      '  return props.name;',
      '}',
      'export const FIXTURE_ENTRYPOINT = { fn: Component, params: [{name: "A"}] };',
    ].join('\n');

    withStrictRustEngine(() =>
      runBabelPluginReactCompiler(source, '/fixture.js', 'flow', {
        compilationMode: 'all',
        compilerEngine: 'rust',
        logger: {
          logEvent(_filename, event) {
            loggedEvents.push(event);
          },
        },
      }),
    );

    const fallbackEvent = loggedEvents.find(
      (event: any) =>
        event.kind === 'CompileSkip' &&
        typeof event.reason === 'string' &&
        event.reason.startsWith(
          'rust_frontend_error:unsupported_flow_syntax:flow_syntax_not_supported',
        ),
    ) as any;

    expect(fallbackEvent).toBeDefined();
    expect(fallbackEvent.loc).not.toBeNull();
    if (fallbackEvent.loc != null) {
      expect(fallbackEvent.loc.start.line).toBeGreaterThan(0);
      expect(fallbackEvent.loc.start.index).toBeGreaterThan(0);
    }
  });

  it('strict rust mode falls back on TS instantiation expressions', () => {
    const source = [
      'function id<T>(x: T): T {',
      '  return x;',
      '}',
      'function Component() {',
      '  const instantiate = id<string>;',
      "  return instantiate('hello');",
      '}',
      'export const FIXTURE_ENTRYPOINT = { fn: Component, params: [] };',
    ].join('\n');
    const rustResult = withStrictRustEngine(() =>
      runBabelPluginReactCompiler(source, '/fixture.ts', 'typescript', {
        compilationMode: 'all',
        compilerEngine: 'rust',
      }),
    );
    const babelResult = runBabelPluginReactCompiler(
      source,
      '/fixture.ts',
      'typescript',
      {
        compilationMode: 'all',
        compilerEngine: 'babel',
      },
    );

    expect(canonicalizeCode(rustResult.code)).toBe(
      canonicalizeCode(babelResult.code),
    );
  });

  it('strict rust mode falls back on TS satisfies expressions', () => {
    const source = [
      'function Component() {',
      '  const value = [1, 2, 3] satisfies Array<number>;',
      '  return value.length;',
      '}',
      'export const FIXTURE_ENTRYPOINT = { fn: Component, params: [] };',
    ].join('\n');
    const rustResult = withStrictRustEngine(() =>
      runBabelPluginReactCompiler(source, '/fixture.ts', 'typescript', {
        compilationMode: 'all',
        compilerEngine: 'rust',
      }),
    );
    const babelResult = runBabelPluginReactCompiler(
      source,
      '/fixture.ts',
      'typescript',
      {
        compilationMode: 'all',
        compilerEngine: 'babel',
      },
    );

    expect(canonicalizeCode(rustResult.code)).toBe(
      canonicalizeCode(babelResult.code),
    );
  });
});
