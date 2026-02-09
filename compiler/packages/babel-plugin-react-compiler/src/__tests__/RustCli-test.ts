/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import {runRustCompilerCli} from '../RustBridge/RustCli';

describe('Rust compiler CLI bridge', () => {
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
      expect(result.code).toContain('export const value = 1;');
    }
  });
});
