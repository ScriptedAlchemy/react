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
  apply_placeholder_transforms?: boolean;
  emit_debug_ir?: boolean;
};

type RustSourceLocation = {
  start_line: number;
  start_column: number;
  end_line: number;
  end_column: number;
};

export type RustCompileResponse =
  | {
      status: 'ok';
      code: string;
      statement_count: number;
      detected_react_functions: number;
      react_functions: Array<{
        name: string;
        kind: 'Component' | 'Hook';
        loc: null | RustSourceLocation;
      }>;
      debug_ir?: string;
    }
  | {
      status: 'error';
      code: string;
      category: string;
      reason: string;
      severity: string;
      message: string;
      location?: RustSourceLocation | null;
    };

function resolveRustManifestPath(): string {
  const explicitPath = process.env['REACT_COMPILER_RUST_MANIFEST'];
  const candidates = new Set<string>();
  if (explicitPath != null) {
    candidates.add(explicitPath);
  }

  let currentDir = process.cwd();
  for (let depth = 0; depth < 8; depth++) {
    candidates.add(path.resolve(currentDir, 'rust', 'Cargo.toml'));
    candidates.add(path.resolve(currentDir, 'compiler', 'rust', 'Cargo.toml'));
    const nextDir = path.dirname(currentDir);
    if (nextDir === currentDir) {
      break;
    }
    currentDir = nextDir;
  }

  candidates.add(path.resolve(__dirname, '../../../../rust/Cargo.toml'));
  candidates.add(path.resolve(__dirname, '../../../rust/Cargo.toml'));

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    'Could not resolve Rust workspace manifest. Set REACT_COMPILER_RUST_MANIFEST to compiler/rust/Cargo.toml',
  );
}

function resolveRustCliInvocation(manifestPath: string): {
  command: string;
  args: Array<string>;
} {
  const explicitBinary = process.env['REACT_COMPILER_RUST_CLI_BIN'];
  if (explicitBinary != null && explicitBinary.length > 0) {
    return {
      command: explicitBinary,
      args: [],
    };
  }

  const usePrebuiltBinary =
    process.env['REACT_COMPILER_RUST_USE_PREBUILT_BIN'] === '1' ||
    process.env['REACT_COMPILER_RUST_USE_PREBUILT_BIN'] === 'true';
  if (usePrebuiltBinary) {
    const rustWorkspaceRoot = path.dirname(manifestPath);
    const binaryName =
      process.platform === 'win32' ? 'react_compiler_cli.exe' : 'react_compiler_cli';
    const debugBinaryPath = path.resolve(
      rustWorkspaceRoot,
      'target',
      'debug',
      binaryName,
    );
    if (fs.existsSync(debugBinaryPath)) {
      return {
        command: debugBinaryPath,
        args: [],
      };
    }
  }

  return {
    command: 'cargo',
    args: [
      '+stable',
      'run',
      '--quiet',
      '--manifest-path',
      manifestPath,
      '-p',
      'react_compiler_cli',
    ],
  };
}

export function runRustCompilerCli(
  request: RustCompileRequest,
): RustCompileResponse {
  const manifestPath = resolveRustManifestPath();
  const invocation = resolveRustCliInvocation(manifestPath);
  const result = spawnSync(invocation.command, invocation.args, {
    input: JSON.stringify(request),
    encoding: 'utf-8',
  });

  if (result.status !== 0) {
    throw new Error(
      `Rust compiler CLI (${invocation.command}) exited with status ${result.status}\n${result.stderr}`,
    );
  }

  if (typeof result.stdout !== 'string' || result.stdout.length === 0) {
    throw new Error('Rust compiler CLI returned an empty response');
  }

  return JSON.parse(result.stdout) as RustCompileResponse;
}
