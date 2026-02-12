/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import fs from 'fs';
import os from 'os';
import path from 'path';
import {spawnSync} from 'child_process';
import {RUST_CLI_PROTOCOL_VERSION} from './RustCliProtocol';

const DEFAULT_RUST_CLI_TIMEOUT_MS = 60_000;
const DEFAULT_RUST_CLI_MAX_BUFFER_BYTES = 64 * 1024 * 1024;

function resolvePossiblyTildePath(inputPath: string): string {
  if (inputPath === '~') {
    return os.homedir();
  }
  if (inputPath.startsWith('~/') || inputPath.startsWith('~\\')) {
    return path.join(os.homedir(), inputPath.slice(2));
  }
  return path.resolve(inputPath);
}

export type RustCompileRequest = {
  source: string;
  filename?: string;
  dialect?: 'javascript' | 'typescript' | 'flow';
  is_module?: boolean;
  apply_placeholder_transforms?: boolean;
  emit_debug_ir?: boolean;
  protocol_version?: number;
};

export type RustSourceLocation = {
  start_line: number;
  start_column: number;
  end_line: number;
  end_column: number;
};

export type RustReactFunction = {
  name: string;
  kind: string;
  loc?: RustSourceLocation | null;
};

export type RustCompileResponse =
  | {
      status: 'ok';
      protocol_version?: number;
      code: string;
      debug_ir?: string;
      react_functions?: Array<RustReactFunction>;
    }
  | {
      status: 'error';
      protocol_version?: number;
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
  const checkedCandidates = new Set<string>();
  if (explicitPath != null && explicitPath.length > 0) {
    const resolvedExplicitPath = resolvePossiblyTildePath(explicitPath);
    if (
      fs.existsSync(resolvedExplicitPath) &&
      fs.statSync(resolvedExplicitPath).isFile()
    ) {
      if (path.basename(resolvedExplicitPath) !== 'Cargo.toml') {
        throw new Error(
          `REACT_COMPILER_RUST_MANIFEST must point to a Cargo.toml file, got: ${resolvedExplicitPath}`,
        );
      }
      return resolvedExplicitPath;
    }
    throw new Error(
      `REACT_COMPILER_RUST_MANIFEST points to a missing manifest: ${resolvedExplicitPath}`,
    );
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
    checkedCandidates.add(candidate);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  const checked = Array.from(checkedCandidates).map(item => `- ${item}`).join('\n');
  throw new Error(
    `Could not resolve Rust workspace manifest. Set REACT_COMPILER_RUST_MANIFEST to compiler/rust/Cargo.toml.\nChecked:\n${checked}`,
  );
}

function resolveRustCliInvocation(manifestPath: string): {
  command: string;
  args: Array<string>;
} {
  const explicitBinary = process.env['REACT_COMPILER_RUST_CLI_BIN'];
  if (explicitBinary != null && explicitBinary.length > 0) {
    if (/[\\/]/.test(explicitBinary)) {
      const resolvedBinary = resolvePossiblyTildePath(explicitBinary);
      if (
        !fs.existsSync(resolvedBinary) ||
        !fs.statSync(resolvedBinary).isFile()
      ) {
        throw new Error(
          `REACT_COMPILER_RUST_CLI_BIN points to a missing binary: ${resolvedBinary}`,
        );
      }
      return {command: resolvedBinary, args: []};
    }
    return {command: explicitBinary, args: []};
  }

  const usePrebuiltBinary =
    isEnabledEnvironmentVariable('REACT_COMPILER_RUST_USE_PREBUILT_BIN');
  if (usePrebuiltBinary) {
    const rustWorkspaceRoot = path.dirname(manifestPath);
    const binaryName =
      process.platform === 'win32' ? 'react_compiler_cli.exe' : 'react_compiler_cli';
    const profiles = resolveRustPrebuiltProfiles();
    for (const profile of profiles) {
      const prebuiltBinaryPath = path.resolve(
        rustWorkspaceRoot,
        'target',
        profile,
        binaryName,
      );
      if (
        fs.existsSync(prebuiltBinaryPath) &&
        fs.statSync(prebuiltBinaryPath).isFile()
      ) {
        return {command: prebuiltBinaryPath, args: []};
      }
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

function isEnabledEnvironmentVariable(name: string): boolean {
  const raw = process.env[name];
  if (raw == null) {
    return false;
  }
  const normalized = raw.trim().toLowerCase();
  return (
    normalized === '1' ||
    normalized === 'true' ||
    normalized === 'yes' ||
    normalized === 'on'
  );
}

function resolveRustPrebuiltProfiles(): Array<'debug' | 'release'> {
  const raw = process.env['REACT_COMPILER_RUST_PREBUILT_PROFILE'];
  if (raw == null || raw.length === 0) {
    return ['debug', 'release'];
  }
  const normalized = raw.trim().toLowerCase();
  if (normalized === 'debug' || normalized === 'release') {
    return [normalized];
  }
  throw new Error(
    `REACT_COMPILER_RUST_PREBUILT_PROFILE must be "debug" or "release", got: ${raw}`,
  );
}

function resolveRustCliTimeoutMs(): number {
  const raw = process.env['REACT_COMPILER_RUST_CLI_TIMEOUT_MS'];
  if (raw == null || raw.length === 0) {
    return DEFAULT_RUST_CLI_TIMEOUT_MS;
  }
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || !Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(
      `REACT_COMPILER_RUST_CLI_TIMEOUT_MS must be a positive integer, got: ${raw}`,
    );
  }
  return parsed;
}

function resolveRustCliMaxBufferBytes(): number {
  const raw = process.env['REACT_COMPILER_RUST_CLI_MAX_BUFFER_BYTES'];
  if (raw == null || raw.length === 0) {
    return DEFAULT_RUST_CLI_MAX_BUFFER_BYTES;
  }
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || !Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(
      `REACT_COMPILER_RUST_CLI_MAX_BUFFER_BYTES must be a positive integer, got: ${raw}`,
    );
  }
  return parsed;
}

export function runRustCompilerCli(
  request: RustCompileRequest,
): RustCompileResponse {
  const manifestPath = resolveRustManifestPath();
  const invocation = resolveRustCliInvocation(manifestPath);
  const timeoutMs = resolveRustCliTimeoutMs();
  const maxBufferBytes = resolveRustCliMaxBufferBytes();
  const requestPayload: RustCompileRequest = {
    ...request,
    protocol_version: request.protocol_version ?? RUST_CLI_PROTOCOL_VERSION,
  };

  const result = spawnSync(invocation.command, invocation.args, {
    input: JSON.stringify(requestPayload),
    encoding: 'utf-8',
    timeout: timeoutMs,
    maxBuffer: maxBufferBytes,
  });

  if (result.error != null) {
    const reason = result.error.message;
    const invocationSummary = [invocation.command, ...invocation.args].join(' ');
    throw new Error(
      `Rust compiler CLI (${invocationSummary}) failed before completion: ${reason}`,
    );
  }
  if (result.signal != null) {
    const invocationSummary = [invocation.command, ...invocation.args].join(' ');
    throw new Error(
      `Rust compiler CLI (${invocationSummary}) was terminated by signal ${result.signal}`,
    );
  }
  if (result.status !== 0) {
    const invocationSummary = [invocation.command, ...invocation.args].join(' ');
    const stderr = typeof result.stderr === 'string' ? result.stderr : '';
    const stdout = typeof result.stdout === 'string' ? result.stdout : '';
    const output = stderr.length > 0 ? stderr : stdout;
    throw new Error(
      `Rust compiler CLI (${invocationSummary}) exited with status ${result.status}\n${
        output.length > 0 ? output : '<no CLI output>'
      }`,
    );
  }
  if (typeof result.stdout !== 'string' || result.stdout.length === 0) {
    throw new Error('Rust compiler CLI returned an empty response');
  }

  let response: unknown;
  try {
    response = JSON.parse(result.stdout) as unknown;
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(
      `Rust compiler CLI returned invalid JSON response: ${reason}\n${result.stdout}`,
    );
  }
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
    if (typeof payload['code'] !== 'string' || payload['code'].length === 0) {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (code must be a non-empty string)',
      );
    }
    if (
      payload['debug_ir'] != null &&
      typeof payload['debug_ir'] !== 'string'
    ) {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (debug_ir must be a string when present)',
      );
    }
    const reactFunctions = payload['react_functions'];
    if (reactFunctions == null) {
      return;
    }
    if (!Array.isArray(reactFunctions)) {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (react_functions must be an array when present)',
      );
    }
    for (const fnRecord of reactFunctions) {
      if (fnRecord == null || typeof fnRecord !== 'object') {
        throw new Error(
          'Rust compiler CLI returned invalid ok payload (react_functions entries must be objects)',
        );
      }
      const fnData = fnRecord as {[key: string]: unknown};
      if (typeof fnData['name'] !== 'string' || typeof fnData['kind'] !== 'string') {
        throw new Error(
          'Rust compiler CLI returned invalid ok payload (react_functions entries require name/kind strings)',
        );
      }
      const fnKind = fnData['kind'];
      if (fnKind !== 'Component' && fnKind !== 'Hook') {
        throw new Error(
          `Rust compiler CLI returned invalid ok payload (react_functions kind must be Component or Hook, got ${String(
            fnKind,
          )})`,
        );
      }
      const fnLoc = fnData['loc'];
      if (fnLoc == null) {
        continue;
      }
      assertRustLocationPayload(fnLoc, 'react_functions[].loc');
    }
    return;
  }

  if (
    typeof payload['code'] !== 'string' ||
    payload['code'].length === 0 ||
    typeof payload['category'] !== 'string' ||
    payload['category'].length === 0 ||
    typeof payload['reason'] !== 'string' ||
    payload['reason'].length === 0 ||
    typeof payload['severity'] !== 'string' ||
    payload['severity'].length === 0 ||
    typeof payload['message'] !== 'string'
  ) {
    throw new Error(
      'Rust compiler CLI returned invalid error payload (required string fields must be present and non-empty)',
    );
  }
  const category = payload['category'];
  if (
    category !== 'request' &&
    category !== 'syntax' &&
    category !== 'internal'
  ) {
    throw new Error(
      `Rust compiler CLI returned invalid error payload (unsupported category: ${String(
        category,
      )})`,
    );
  }
  const severity = (payload['severity'] as string).toLowerCase();
  if (
    severity !== 'error' &&
    severity !== 'warning' &&
    severity !== 'hint' &&
    severity !== 'off'
  ) {
    throw new Error(
      `Rust compiler CLI returned invalid error payload (unsupported severity: ${String(
        payload['severity'],
      )})`,
    );
  }

  const location = payload['location'];
  if (location == null) {
    return;
  }
  assertRustLocationPayload(location, 'location');
}

function assertRustLocationPayload(
  location: unknown,
  fieldName: string,
): asserts location is RustSourceLocation {
  if (location == null || typeof location !== 'object') {
    throw new Error(
      `Rust compiler CLI returned invalid payload (${fieldName} must be an object when present)`,
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
      `Rust compiler CLI returned invalid payload (${fieldName} must include non-negative integer start/end fields)`,
    );
  }
  const startLine = locationRecord['start_line'] as number;
  const startColumn = locationRecord['start_column'] as number;
  const endLine = locationRecord['end_line'] as number;
  const endColumn = locationRecord['end_column'] as number;
  if (endLine < startLine || (endLine === startLine && endColumn < startColumn)) {
    throw new Error(
      `Rust compiler CLI returned invalid payload (${fieldName} end location must not precede start location)`,
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
