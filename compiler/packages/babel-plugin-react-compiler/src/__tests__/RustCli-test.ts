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
      expect(result.react_functions[0]?.name).toBe('useValue');
      expect(result.react_functions[0]?.kind).toBe('Hook');
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
      expect(result.code).toContain('react/compiler-runtime');
      expect(result.code).toContain('const $ = _c(0);');
    }
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
