/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import {runRustCompilerCli} from '../RustBridge/RustCli';
import {runBabelPluginReactCompiler} from '../Babel/RunReactCompilerBabelPlugin';
import {spawnSync} from 'child_process';

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

    expect(rustResult.code).toBe(babelResult.code);
  });

  it('strict rust mode transforms default exported named components', () => {
    const result = withStrictRustEngine(() =>
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

    expect(result.code).toContain('export default function Component');
    expect(result.code).toContain('react/compiler-runtime');
    expect(result.code).toContain('const $ = _c(0);');
  });

  it('strict rust mode does not duplicate existing placeholder cache init', () => {
    const source = [
      "import { c as _c } from 'react/compiler-runtime';",
      'export function Component() {',
      '  const $ = _c(0);',
      '  return <div />;',
      '}',
    ].join('\n');
    const result = withStrictRustEngine(() =>
      runBabelPluginReactCompiler(source, '/fixture.tsx', 'typescript', {
        compilationMode: 'all',
        compilerEngine: 'rust',
      }),
    );

    const outputCode = result.code ?? '';
    const memoInitCount = (outputCode.match(/const \$ = _c\(0\);/g) ?? []).length;
    expect(memoInitCount).toBe(1);
  });

  it('strict rust mode detects fixture entrypoint component names', () => {
    const source = [
      'function component() {',
      '  return 1;',
      '}',
      'export const FIXTURE_ENTRYPOINT = { fn: component, params: [] };',
    ].join('\n');
    const result = withStrictRustEngine(() =>
      runBabelPluginReactCompiler(source, '/fixture.tsx', 'typescript', {
        compilationMode: 'all',
        compilerEngine: 'rust',
      }),
    );

    expect(result.code).toContain('function component');
    expect(result.code).not.toContain('react/compiler-runtime');
  });
});
