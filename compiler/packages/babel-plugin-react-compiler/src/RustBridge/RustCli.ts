/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import fs from 'fs';
import path from 'path';
import {spawnSync} from 'child_process';

export type RustCompileRequest = {
  source: string;
  filename?: string;
  dialect?: 'javascript' | 'typescript' | 'flow';
  is_module?: boolean;
};

export type RustCompileResponse =
  | {status: 'ok'; code: string; statement_count: number}
  | {status: 'error'; message: string};

function resolveRustManifestPath(): string {
  const explicitPath = process.env['REACT_COMPILER_RUST_MANIFEST'];
  const candidates = [
    explicitPath,
    path.resolve(process.cwd(), 'rust', 'Cargo.toml'),
    path.resolve(process.cwd(), 'compiler', 'rust', 'Cargo.toml'),
    path.resolve(__dirname, '../../../../rust/Cargo.toml'),
  ].filter((candidate): candidate is string => candidate != null);

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    'Could not resolve Rust workspace manifest. Set REACT_COMPILER_RUST_MANIFEST to compiler/rust/Cargo.toml',
  );
}

export function runRustCompilerCli(
  request: RustCompileRequest,
): RustCompileResponse {
  const manifestPath = resolveRustManifestPath();
  const result = spawnSync(
    'cargo',
    [
      '+stable',
      'run',
      '--quiet',
      '--manifest-path',
      manifestPath,
      '-p',
      'react_compiler_cli',
    ],
    {
      input: JSON.stringify(request),
      encoding: 'utf-8',
    },
  );

  if (result.status !== 0) {
    throw new Error(
      `Rust compiler CLI exited with status ${result.status}\n${result.stderr}`,
    );
  }

  if (typeof result.stdout !== 'string' || result.stdout.length === 0) {
    throw new Error('Rust compiler CLI returned an empty response');
  }

  return JSON.parse(result.stdout) as RustCompileResponse;
}
