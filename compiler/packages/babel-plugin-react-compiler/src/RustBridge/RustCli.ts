/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import fs from 'fs';
import path from 'path';
import {spawnSync} from 'child_process';
import {RUST_CLI_PROTOCOL_VERSION} from './RustCliProtocol';

export type RustCompileRequest = {
  source: string;
  filename?: string;
  dialect?: 'javascript' | 'typescript';
  is_module?: boolean;
  apply_placeholder_transforms?: boolean;
  emit_debug_ir?: boolean;
  protocol_version?: number;
};

export type RustCompileResponse =
  | {
      status: 'ok';
      protocol_version?: number;
      code: string;
      debug_ir?: string;
    }
  | {
      status: 'error';
      protocol_version?: number;
      code: string;
      category: string;
      reason: string;
      severity: string;
      message: string;
      location?: {
        start_line: number;
        start_column: number;
        end_line: number;
        end_column: number;
      } | null;
    };

function resolveRustManifestPath(): string {
  const explicitPath = process.env['REACT_COMPILER_RUST_MANIFEST'];
  const candidates = new Set<string>();
  if (explicitPath != null && explicitPath.length > 0) {
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
    return {command: explicitBinary, args: []};
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
      return {command: debugBinaryPath, args: []};
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
  const requestPayload: RustCompileRequest = {
    ...request,
    protocol_version: request.protocol_version ?? RUST_CLI_PROTOCOL_VERSION,
  };

  const result = spawnSync(invocation.command, invocation.args, {
    input: JSON.stringify(requestPayload),
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

  const response = JSON.parse(result.stdout) as unknown;
  assertRustCompileResponseShape(response);
  assertCompatibleRustCliProtocolVersion(response);
  return response;
}

function assertRustCompileResponseShape(
  response: unknown,
): asserts response is RustCompileResponse {
  if (response == null || typeof response !== 'object') {
    throw new Error(
      'Rust compiler CLI returned an invalid response payload (expected JSON object)',
    );
  }
  const payload = response as {[key: string]: unknown};
  const status = payload['status'];
  if (status !== 'ok' && status !== 'error') {
    throw new Error(
      `Rust compiler CLI returned invalid status: ${String(status)}`,
    );
  }

  if (status === 'ok') {
    if (typeof payload['code'] !== 'string') {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (missing code string)',
      );
    }
    return;
  }

  if (
    typeof payload['code'] !== 'string' ||
    typeof payload['category'] !== 'string' ||
    typeof payload['reason'] !== 'string' ||
    typeof payload['severity'] !== 'string' ||
    typeof payload['message'] !== 'string'
  ) {
    throw new Error(
      'Rust compiler CLI returned invalid error payload (missing required string fields)',
    );
  }

  const location = payload['location'];
  if (location == null) {
    return;
  }
  if (typeof location !== 'object') {
    throw new Error(
      'Rust compiler CLI returned invalid error payload (location must be an object when present)',
    );
  }
  const locationRecord = location as {[key: string]: unknown};
  if (
    !Number.isInteger(locationRecord['start_line']) ||
    !Number.isInteger(locationRecord['start_column']) ||
    !Number.isInteger(locationRecord['end_line']) ||
    !Number.isInteger(locationRecord['end_column']) ||
    (locationRecord['start_line'] as number) < 0 ||
    (locationRecord['start_column'] as number) < 0 ||
    (locationRecord['end_line'] as number) < 0 ||
    (locationRecord['end_column'] as number) < 0
  ) {
    throw new Error(
      'Rust compiler CLI returned invalid error payload (location must include non-negative integer start/end fields)',
    );
  }
}

function assertCompatibleRustCliProtocolVersion(
  response: RustCompileResponse,
): void {
  const protocolVersion = (response as {protocol_version?: unknown})
    .protocol_version;
  if (protocolVersion == null) {
    return;
  }
  if (!Number.isInteger(protocolVersion)) {
    throw new Error(
      `Rust compiler CLI returned non-integer protocol_version: ${String(
        protocolVersion,
      )}`,
    );
  }
  if (protocolVersion !== RUST_CLI_PROTOCOL_VERSION) {
    throw new Error(
      `Rust compiler CLI protocol_version mismatch. Expected ${RUST_CLI_PROTOCOL_VERSION}, received ${protocolVersion}`,
    );
  }
}
