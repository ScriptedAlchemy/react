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
  kind: 'Component' | 'Hook';
  loc?: RustSourceLocation | null;
};

export type RustCompileSuccessResponse = {
  status: 'ok';
  protocol_version?: number;
  code: string;
  statement_count: number;
  statement_count_after_transform: number;
  placeholder_runtime_helper_import_count_before_transform: number;
  placeholder_runtime_helper_import_count_after_transform: number;
  placeholder_runtime_helper_import_added: boolean;
  placeholder_runtime_callee_reused: boolean;
  placeholder_runtime_callee_generated: boolean;
  placeholder_transform_status: string;
  placeholder_transform_candidates: Array<string>;
  placeholder_transform_skipped_functions: Array<string>;
  placeholder_transform_candidate_count: number;
  placeholder_transform_skipped_count: number;
  placeholder_transform_candidate_component_count: number;
  placeholder_transform_candidate_hook_count: number;
  placeholder_transform_transformed_component_count: number;
  placeholder_transform_transformed_hook_count: number;
  placeholder_transform_skipped_component_count: number;
  placeholder_transform_skipped_hook_count: number;
  detected_component_function_count: number;
  detected_hook_function_count: number;
  detected_component_functions: Array<string>;
  detected_hook_functions: Array<string>;
  detected_react_functions: number;
  react_functions: Array<RustReactFunction>;
  placeholder_transforms_applied: number;
  placeholder_transformed_functions: Array<string>;
  placeholder_runtime_callee_name_before_transform?: string;
  placeholder_runtime_callee_candidates_before_transform: Array<string>;
  placeholder_runtime_callee_candidate_count_before_transform: number;
  placeholder_runtime_namespace_candidates_before_transform: Array<string>;
  placeholder_runtime_namespace_candidate_count_before_transform: number;
  placeholder_runtime_callee_name?: string;
  placeholder_runtime_callee_candidates: Array<string>;
  placeholder_runtime_callee_candidate_count: number;
  placeholder_runtime_namespace_candidates: Array<string>;
  placeholder_runtime_namespace_candidate_count: number;
  debug_ir?: string;
};

export type RustCompileErrorResponse = {
  status: 'error';
  protocol_version?: number;
  code: string;
  category: string;
  reason: string;
  severity: string;
  message: string;
  location?: RustSourceLocation | null;
};

export type RustCompileResponse =
  | RustCompileSuccessResponse
  | RustCompileErrorResponse;

function resolveRustManifestPath(): string {
  const explicitPath = process.env['REACT_COMPILER_RUST_MANIFEST'];
  const candidates = new Set<string>();
  const checkedCandidates = new Set<string>();
  if (explicitPath != null && explicitPath.length > 0) {
    const resolvedExplicitPath = resolvePossiblyTildePath(explicitPath);
    if (!fs.existsSync(resolvedExplicitPath)) {
      throw new Error(
        `REACT_COMPILER_RUST_MANIFEST points to a missing manifest: ${resolvedExplicitPath}`,
      );
    }
    if (!fs.statSync(resolvedExplicitPath).isFile()) {
      throw new Error(
        `REACT_COMPILER_RUST_MANIFEST must point to a file, got: ${resolvedExplicitPath}`,
      );
    }
    if (path.basename(resolvedExplicitPath) !== 'Cargo.toml') {
      throw new Error(
        `REACT_COMPILER_RUST_MANIFEST must point to a Cargo.toml file, got: ${resolvedExplicitPath}`,
      );
    }
    return resolvedExplicitPath;
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
    if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
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
    const resolvedCommand = resolveCommandFromPath(explicitBinary);
    if (resolvedCommand == null) {
      throw new Error(
        `REACT_COMPILER_RUST_CLI_BIN command is not available on PATH: ${explicitBinary}`,
      );
    }
    return {command: resolvedCommand, args: []};
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
    if (profiles.length === 1) {
      throw new Error(
        `REACT_COMPILER_RUST_USE_PREBUILT_BIN is enabled but no prebuilt react_compiler_cli binary was found in target/${profiles[0]}`,
      );
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

function resolveCommandFromPath(commandName: string): string | null {
  const envPath = process.env['PATH'];
  if (envPath == null || envPath.length === 0) {
    return null;
  }
  const pathEntries = envPath.split(path.delimiter).filter(entry => entry.length > 0);
  const commandCandidates = getPathCommandCandidates(commandName);

  for (const pathEntry of pathEntries) {
    for (const commandCandidate of commandCandidates) {
      const candidate = path.join(pathEntry, commandCandidate);
      if (!fs.existsSync(candidate) || !fs.statSync(candidate).isFile()) {
        continue;
      }
      if (process.platform !== 'win32') {
        try {
          fs.accessSync(candidate, fs.constants.X_OK);
        } catch {
          continue;
        }
      }
      return candidate;
    }
  }

  return null;
}

function getPathCommandCandidates(commandName: string): Array<string> {
  if (process.platform !== 'win32') {
    return [commandName];
  }
  if (path.extname(commandName).length > 0) {
    return [commandName];
  }
  const windowsPathExtensions = (process.env['PATHEXT'] ?? '.COM;.EXE;.BAT;.CMD')
    .split(';')
    .filter(extension => extension.length > 0)
    .map(extension =>
      extension.length > 0 && !extension.startsWith('.') ? `.${extension}` : extension,
    );
  return [commandName, ...windowsPathExtensions.map(extension => `${commandName}${extension}`)];
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
  const invocationSummary = [invocation.command, ...invocation.args].join(' ');
  const timeoutMs = resolveRustCliTimeoutMs();
  const maxBufferBytes = resolveRustCliMaxBufferBytes();
  const requestPayload = normalizeRustCompileRequest(request);

  const result = spawnSync(invocation.command, invocation.args, {
    input: JSON.stringify(requestPayload),
    encoding: 'utf-8',
    timeout: timeoutMs,
    maxBuffer: maxBufferBytes,
  });

  if (result.error != null) {
    const reason = result.error.message;
    throw new Error(
      `Rust compiler CLI (${invocationSummary}) failed before completion: ${reason}`,
    );
  }
  if (result.signal != null) {
    throw new Error(
      `Rust compiler CLI (${invocationSummary}) was terminated by signal ${result.signal}`,
    );
  }
  if (result.status !== 0) {
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

function normalizeRustCompileRequest(request: RustCompileRequest): RustCompileRequest {
  if (request == null || typeof request !== 'object') {
    throw new Error('Rust compiler CLI request must be a JSON object');
  }
  if (typeof request.source !== 'string') {
    throw new Error('Rust compiler CLI request source must be a string');
  }
  const normalizedRequest: RustCompileRequest = {
    source: request.source,
  };
  if (request.filename != null) {
    if (typeof request.filename !== 'string' || request.filename.length === 0) {
      throw new Error(
        'Rust compiler CLI request filename must be a non-empty string when present',
      );
    }
    normalizedRequest.filename = request.filename;
  }
  if (request.dialect != null) {
    normalizedRequest.dialect = normalizeRequestDialect(request.dialect);
  }
  if (request.is_module != null) {
    if (typeof request.is_module !== 'boolean') {
      throw new Error(
        'Rust compiler CLI request is_module must be a boolean when present',
      );
    }
    normalizedRequest.is_module = request.is_module;
  }
  if (request.apply_placeholder_transforms != null) {
    if (typeof request.apply_placeholder_transforms !== 'boolean') {
      throw new Error(
        'Rust compiler CLI request apply_placeholder_transforms must be a boolean when present',
      );
    }
    normalizedRequest.apply_placeholder_transforms =
      request.apply_placeholder_transforms;
  }
  if (request.emit_debug_ir != null) {
    if (typeof request.emit_debug_ir !== 'boolean') {
      throw new Error(
        'Rust compiler CLI request emit_debug_ir must be a boolean when present',
      );
    }
    normalizedRequest.emit_debug_ir = request.emit_debug_ir;
  }
  normalizedRequest.protocol_version = resolveRequestProtocolVersion(
    request.protocol_version,
  );
  return normalizedRequest;
}

function normalizeRequestDialect(
  dialect: RustCompileRequest['dialect'],
): 'javascript' | 'typescript' | 'flow' {
  if (dialect === 'javascript' || dialect === 'typescript' || dialect === 'flow') {
    return dialect;
  }
  throw new Error(
    `Rust compiler CLI request dialect must be one of javascript/typescript/flow, got: ${String(
      dialect,
    )}`,
  );
}

function resolveRequestProtocolVersion(raw: unknown): number {
  if (raw == null) {
    return RUST_CLI_PROTOCOL_VERSION;
  }
  const numericVersion =
    typeof raw === 'string' && raw.trim().length > 0 ? Number(raw) : raw;
  if (!Number.isInteger(numericVersion) || (numericVersion as number) < 0) {
    throw new Error(
      `Rust compiler CLI request protocol_version must be a non-negative integer, got: ${String(
        raw,
      )}`,
    );
  }
  return numericVersion as number;
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
    requireNonEmptyStringField(payload, 'code', 'ok payload');
    if (
      payload['debug_ir'] != null &&
      typeof payload['debug_ir'] !== 'string'
    ) {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (debug_ir must be a string when present)',
      );
    }
    const reactFunctions = payload['react_functions'];
    if (!Array.isArray(reactFunctions)) {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (react_functions must be an array)',
      );
    }
    const validatedReactFunctions: Array<RustReactFunction> = [];
    for (const fnRecord of reactFunctions) {
      if (fnRecord == null || typeof fnRecord !== 'object') {
        throw new Error(
          'Rust compiler CLI returned invalid ok payload (react_functions entries must be objects)',
        );
      }
      const fnData = fnRecord as {[key: string]: unknown};
      requireNonEmptyStringField(
        fnData,
        'name',
        'ok payload (react_functions entries)',
      );
      const fnKind = requireNonEmptyStringField(
        fnData,
        'kind',
        'ok payload (react_functions entries)',
      );
      if (fnKind !== 'Component' && fnKind !== 'Hook') {
        throw new Error(
          `Rust compiler CLI returned invalid ok payload (react_functions kind must be Component or Hook, got ${String(
            fnKind,
          )})`,
        );
      }
      const fnLoc = fnData['loc'];
      if (fnLoc == null) {
        validatedReactFunctions.push({
          name: fnData['name'] as string,
          kind: fnKind as RustReactFunction['kind'],
          loc: null,
        });
        continue;
      }
      assertRustLocationPayload(fnLoc, 'react_functions[].loc');
      validatedReactFunctions.push({
        name: fnData['name'] as string,
        kind: fnKind as RustReactFunction['kind'],
        loc: fnLoc,
      });
    }
    assertRustOkMetadataPayload(payload, validatedReactFunctions.length);
    return;
  }

  const code = requireNonEmptyStringField(payload, 'code', 'error payload');
  const category = requireNonEmptyStringField(
    payload,
    'category',
    'error payload',
  );
  const reason = requireNonEmptyStringField(payload, 'reason', 'error payload');
  const severityValue = requireNonEmptyStringField(
    payload,
    'severity',
    'error payload',
  );
  const message = requireNonEmptyStringField(
    payload,
    'message',
    'error payload',
  );
  void code;
  void reason;
  void message;
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
  const severity = severityValue.toLowerCase();
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

function assertRustOkMetadataPayload(
  payload: {[key: string]: unknown},
  reactFunctionCount: number,
): void {
  requireNonNegativeIntegerField(payload, 'statement_count', 'ok payload');
  requireNonNegativeIntegerField(
    payload,
    'statement_count_after_transform',
    'ok payload',
  );
  requireNonNegativeIntegerField(
    payload,
    'placeholder_runtime_helper_import_count_before_transform',
    'ok payload',
  );
  requireNonNegativeIntegerField(
    payload,
    'placeholder_runtime_helper_import_count_after_transform',
    'ok payload',
  );
  requireBooleanField(
    payload,
    'placeholder_runtime_helper_import_added',
    'ok payload',
  );
  requireBooleanField(payload, 'placeholder_runtime_callee_reused', 'ok payload');
  requireBooleanField(
    payload,
    'placeholder_runtime_callee_generated',
    'ok payload',
  );
  requireNonEmptyStringField(payload, 'placeholder_transform_status', 'ok payload');

  const placeholderTransformCandidates = requireStringArrayField(
    payload,
    'placeholder_transform_candidates',
    'ok payload',
  );
  const placeholderTransformSkippedFunctions = requireStringArrayField(
    payload,
    'placeholder_transform_skipped_functions',
    'ok payload',
  );
  const detectedComponentFunctions = requireStringArrayField(
    payload,
    'detected_component_functions',
    'ok payload',
  );
  const detectedHookFunctions = requireStringArrayField(
    payload,
    'detected_hook_functions',
    'ok payload',
  );
  const placeholderTransformedFunctions = requireStringArrayField(
    payload,
    'placeholder_transformed_functions',
    'ok payload',
  );
  const placeholderRuntimeCalleeCandidatesBeforeTransform = requireStringArrayField(
    payload,
    'placeholder_runtime_callee_candidates_before_transform',
    'ok payload',
  );
  const placeholderRuntimeNamespaceCandidatesBeforeTransform = requireStringArrayField(
    payload,
    'placeholder_runtime_namespace_candidates_before_transform',
    'ok payload',
  );
  const placeholderRuntimeCalleeCandidates = requireStringArrayField(
    payload,
    'placeholder_runtime_callee_candidates',
    'ok payload',
  );
  const placeholderRuntimeNamespaceCandidates = requireStringArrayField(
    payload,
    'placeholder_runtime_namespace_candidates',
    'ok payload',
  );

  assertSortedUniqueStringArray(
    placeholderTransformCandidates,
    'placeholder_transform_candidates',
    'ok payload',
  );
  assertSortedUniqueStringArray(
    placeholderTransformSkippedFunctions,
    'placeholder_transform_skipped_functions',
    'ok payload',
  );
  assertSortedUniqueStringArray(
    placeholderTransformedFunctions,
    'placeholder_transformed_functions',
    'ok payload',
  );
  assertSortedUniqueStringArray(
    placeholderRuntimeCalleeCandidatesBeforeTransform,
    'placeholder_runtime_callee_candidates_before_transform',
    'ok payload',
  );
  assertSortedUniqueStringArray(
    placeholderRuntimeNamespaceCandidatesBeforeTransform,
    'placeholder_runtime_namespace_candidates_before_transform',
    'ok payload',
  );
  assertSortedUniqueStringArray(
    placeholderRuntimeCalleeCandidates,
    'placeholder_runtime_callee_candidates',
    'ok payload',
  );
  assertSortedUniqueStringArray(
    placeholderRuntimeNamespaceCandidates,
    'placeholder_runtime_namespace_candidates',
    'ok payload',
  );

  if (
    payload['placeholder_runtime_callee_name_before_transform'] != null &&
    typeof payload['placeholder_runtime_callee_name_before_transform'] !== 'string'
  ) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (placeholder_runtime_callee_name_before_transform must be a string when present)',
    );
  }
  if (
    payload['placeholder_runtime_callee_name'] != null &&
    typeof payload['placeholder_runtime_callee_name'] !== 'string'
  ) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (placeholder_runtime_callee_name must be a string when present)',
    );
  }

  assertArrayCountMatchesField(
    payload,
    'placeholder_transform_candidates',
    placeholderTransformCandidates.length,
    'placeholder_transform_candidate_count',
    'ok payload',
  );
  assertArrayCountMatchesField(
    payload,
    'placeholder_transform_skipped_functions',
    placeholderTransformSkippedFunctions.length,
    'placeholder_transform_skipped_count',
    'ok payload',
  );
  assertArrayCountMatchesField(
    payload,
    'detected_component_functions',
    detectedComponentFunctions.length,
    'detected_component_function_count',
    'ok payload',
  );
  assertArrayCountMatchesField(
    payload,
    'detected_hook_functions',
    detectedHookFunctions.length,
    'detected_hook_function_count',
    'ok payload',
  );
  assertArrayCountMatchesField(
    payload,
    'placeholder_runtime_callee_candidates_before_transform',
    placeholderRuntimeCalleeCandidatesBeforeTransform.length,
    'placeholder_runtime_callee_candidate_count_before_transform',
    'ok payload',
  );
  assertArrayCountMatchesField(
    payload,
    'placeholder_runtime_namespace_candidates_before_transform',
    placeholderRuntimeNamespaceCandidatesBeforeTransform.length,
    'placeholder_runtime_namespace_candidate_count_before_transform',
    'ok payload',
  );
  assertArrayCountMatchesField(
    payload,
    'placeholder_runtime_callee_candidates',
    placeholderRuntimeCalleeCandidates.length,
    'placeholder_runtime_callee_candidate_count',
    'ok payload',
  );
  assertArrayCountMatchesField(
    payload,
    'placeholder_runtime_namespace_candidates',
    placeholderRuntimeNamespaceCandidates.length,
    'placeholder_runtime_namespace_candidate_count',
    'ok payload',
  );
  assertArrayCountMatchesField(
    payload,
    'placeholder_transformed_functions',
    placeholderTransformedFunctions.length,
    'placeholder_transforms_applied',
    'ok payload',
  );
  assertArrayCountMatchesField(
    payload,
    'react_functions',
    reactFunctionCount,
    'detected_react_functions',
    'ok payload',
  );

  requireNonNegativeIntegerField(
    payload,
    'placeholder_transform_candidate_component_count',
    'ok payload',
  );
  requireNonNegativeIntegerField(
    payload,
    'placeholder_transform_candidate_hook_count',
    'ok payload',
  );
  requireNonNegativeIntegerField(
    payload,
    'placeholder_transform_transformed_component_count',
    'ok payload',
  );
  requireNonNegativeIntegerField(
    payload,
    'placeholder_transform_transformed_hook_count',
    'ok payload',
  );
  requireNonNegativeIntegerField(
    payload,
    'placeholder_transform_skipped_component_count',
    'ok payload',
  );
  requireNonNegativeIntegerField(
    payload,
    'placeholder_transform_skipped_hook_count',
    'ok payload',
  );

  assertConsistentOkCountRelationships(payload);
}

function assertConsistentOkCountRelationships(payload: {
  [key: string]: unknown;
}): void {
  const candidateCount = payload['placeholder_transform_candidate_count'] as number;
  const skippedCount = payload['placeholder_transform_skipped_count'] as number;
  const transformedCount = payload['placeholder_transforms_applied'] as number;
  if (candidateCount !== skippedCount + transformedCount) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (candidate count must equal skipped + transformed counts)',
    );
  }

  const candidateComponentCount = payload[
    'placeholder_transform_candidate_component_count'
  ] as number;
  const candidateHookCount = payload[
    'placeholder_transform_candidate_hook_count'
  ] as number;
  if (candidateCount !== candidateComponentCount + candidateHookCount) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (candidate component/hook counts must sum to candidate count)',
    );
  }

  const transformedComponentCount = payload[
    'placeholder_transform_transformed_component_count'
  ] as number;
  const transformedHookCount = payload[
    'placeholder_transform_transformed_hook_count'
  ] as number;
  if (transformedCount !== transformedComponentCount + transformedHookCount) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (transformed component/hook counts must sum to transformed count)',
    );
  }

  const skippedComponentCount = payload[
    'placeholder_transform_skipped_component_count'
  ] as number;
  const skippedHookCount = payload['placeholder_transform_skipped_hook_count'] as number;
  if (skippedCount !== skippedComponentCount + skippedHookCount) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (skipped component/hook counts must sum to skipped count)',
    );
  }

  if (candidateComponentCount !== transformedComponentCount + skippedComponentCount) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (component candidate count must equal transformed + skipped component counts)',
    );
  }
  if (candidateHookCount !== transformedHookCount + skippedHookCount) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (hook candidate count must equal transformed + skipped hook counts)',
    );
  }

  const detectedReactFunctionCount = payload['detected_react_functions'] as number;
  const detectedComponentCount = payload['detected_component_function_count'] as number;
  const detectedHookCount = payload['detected_hook_function_count'] as number;
  if (detectedReactFunctionCount !== detectedComponentCount + detectedHookCount) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (detected component/hook counts must sum to detected_react_functions)',
    );
  }

  assertRuntimeImportAndCalleeFlagConsistency(payload);
  assertPlaceholderTransformStatusConsistency(payload);
}

function assertRuntimeImportAndCalleeFlagConsistency(payload: {
  [key: string]: unknown;
}): void {
  const helperImportCountBefore = payload[
    'placeholder_runtime_helper_import_count_before_transform'
  ] as number;
  const helperImportCountAfter = payload[
    'placeholder_runtime_helper_import_count_after_transform'
  ] as number;
  const helperImportAdded = payload['placeholder_runtime_helper_import_added'] as boolean;
  const transformedCount = payload['placeholder_transforms_applied'] as number;
  const calleeReused = payload['placeholder_runtime_callee_reused'] as boolean;
  const calleeGenerated = payload['placeholder_runtime_callee_generated'] as boolean;
  const calleeNameBefore = payload['placeholder_runtime_callee_name_before_transform'];
  const calleeNameAfter = payload['placeholder_runtime_callee_name'];
  const calleeCandidatesBefore = payload[
    'placeholder_runtime_callee_candidates_before_transform'
  ] as Array<string>;
  const calleeCandidatesAfter = payload[
    'placeholder_runtime_callee_candidates'
  ] as Array<string>;

  if (helperImportCountAfter < helperImportCountBefore) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (runtime helper import count cannot decrease across transform)',
    );
  }
  if (helperImportAdded && helperImportCountAfter <= helperImportCountBefore) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (placeholder_runtime_helper_import_added requires after count > before count)',
    );
  }
  if (!helperImportAdded && helperImportCountAfter > helperImportCountBefore) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (placeholder_runtime_helper_import_added=false cannot accompany helper import count increase)',
    );
  }
  if (helperImportAdded && transformedCount === 0) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (helper import addition requires transformed functions)',
    );
  }

  if (calleeGenerated && calleeReused) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (placeholder_runtime_callee_generated and placeholder_runtime_callee_reused cannot both be true)',
    );
  }
  if (transformedCount === 0 && (calleeGenerated || calleeReused)) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (runtime callee generated/reused flags require transformed functions)',
    );
  }

  if (calleeGenerated) {
    if (calleeNameBefore != null || typeof calleeNameAfter !== 'string') {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (generated runtime callee requires no pre-transform callee and a post-transform callee name)',
      );
    }
    if (!helperImportAdded) {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (generated runtime callee requires helper import addition)',
      );
    }
  }

  if (calleeReused) {
    if (
      typeof calleeNameBefore !== 'string' ||
      typeof calleeNameAfter !== 'string' ||
      calleeNameBefore !== calleeNameAfter
    ) {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (reused runtime callee requires matching pre/post callee names)',
      );
    }
  }

  if (transformedCount > 0 && calleeNameBefore == null && !calleeGenerated) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (transformed output without pre-transform callee must report generated runtime callee)',
    );
  }

  if (calleeNameBefore == null && calleeCandidatesBefore.length > 0) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (pre-transform callee candidates require placeholder_runtime_callee_name_before_transform)',
    );
  }
  if (calleeNameAfter == null && calleeCandidatesAfter.length > 0) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (post-transform callee candidates require placeholder_runtime_callee_name)',
    );
  }
  if (
    typeof calleeNameBefore === 'string' &&
    !calleeCandidatesBefore.includes(calleeNameBefore)
  ) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (pre-transform callee name must appear in pre-transform callee candidates)',
    );
  }
  if (
    typeof calleeNameAfter === 'string' &&
    !calleeCandidatesAfter.includes(calleeNameAfter)
  ) {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (post-transform callee name must appear in post-transform callee candidates)',
    );
  }
  if (transformedCount > 0 && typeof calleeNameAfter !== 'string') {
    throw new Error(
      'Rust compiler CLI returned invalid ok payload (transformed output requires placeholder_runtime_callee_name)',
    );
  }
}

function assertPlaceholderTransformStatusConsistency(payload: {
  [key: string]: unknown;
}): void {
  const status = payload['placeholder_transform_status'] as string;
  const candidateCount = payload['placeholder_transform_candidate_count'] as number;
  const transformedCount = payload['placeholder_transforms_applied'] as number;
  switch (status) {
    case 'disabled':
      if (transformedCount !== 0) {
        throw new Error(
          'Rust compiler CLI returned invalid ok payload (disabled status cannot report transformed functions)',
        );
      }
      return;
    case 'no_candidates':
      if (candidateCount !== 0 || transformedCount !== 0) {
        throw new Error(
          'Rust compiler CLI returned invalid ok payload (no_candidates status requires zero candidates and transforms)',
        );
      }
      return;
    case 'transformed':
      if (transformedCount === 0) {
        throw new Error(
          'Rust compiler CLI returned invalid ok payload (transformed status requires placeholder_transforms_applied > 0)',
        );
      }
      return;
    case 'blocked_missing_runtime_callee':
    case 'no_op':
      if (candidateCount === 0 || transformedCount !== 0) {
        throw new Error(
          `Rust compiler CLI returned invalid ok payload (${status} status requires candidates with zero transforms)`,
        );
      }
      return;
    default:
      throw new Error(
        `Rust compiler CLI returned invalid ok payload (unsupported placeholder_transform_status: ${String(
          status,
        )})`,
      );
  }
}

function assertArrayCountMatchesField(
  payload: {[key: string]: unknown},
  arrayFieldName: string,
  arrayLength: number,
  countFieldName: string,
  payloadLabel: string,
): void {
  const count = requireNonNegativeIntegerField(payload, countFieldName, payloadLabel);
  if (count !== arrayLength) {
    throw new Error(
      `Rust compiler CLI returned invalid ${payloadLabel} (${countFieldName} must match ${arrayFieldName} length)`,
    );
  }
}

function requireNonEmptyStringField(
  payload: {[key: string]: unknown},
  key: string,
  payloadLabel: string,
): string {
  const value = payload[key];
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(
      `Rust compiler CLI returned invalid ${payloadLabel} (${key} must be a non-empty string)`,
    );
  }
  return value;
}

function requireBooleanField(
  payload: {[key: string]: unknown},
  key: string,
  payloadLabel: string,
): boolean {
  const value = payload[key];
  if (typeof value !== 'boolean') {
    throw new Error(
      `Rust compiler CLI returned invalid ${payloadLabel} (${key} must be a boolean)`,
    );
  }
  return value;
}

function requireNonNegativeIntegerField(
  payload: {[key: string]: unknown},
  key: string,
  payloadLabel: string,
): number {
  const value = payload[key];
  if (!Number.isInteger(value) || (value as number) < 0) {
    throw new Error(
      `Rust compiler CLI returned invalid ${payloadLabel} (${key} must be a non-negative integer)`,
    );
  }
  return value as number;
}

function requireStringArrayField(
  payload: {[key: string]: unknown},
  key: string,
  payloadLabel: string,
): Array<string> {
  const value = payload[key];
  if (!Array.isArray(value)) {
    throw new Error(
      `Rust compiler CLI returned invalid ${payloadLabel} (${key} must be an array of strings)`,
    );
  }
  for (const entry of value) {
    if (typeof entry !== 'string') {
      throw new Error(
        `Rust compiler CLI returned invalid ${payloadLabel} (${key} must contain only strings)`,
      );
    }
  }
  return value;
}

function assertSortedUniqueStringArray(
  values: Array<string>,
  key: string,
  payloadLabel: string,
): void {
  for (let index = 0; index < values.length; index++) {
    const currentValue = values[index];
    if (index === 0) {
      continue;
    }
    const previousValue = values[index - 1];
    if (currentValue === previousValue) {
      throw new Error(
        `Rust compiler CLI returned invalid ${payloadLabel} (${key} must not contain duplicate entries)`,
      );
    }
    if (currentValue < previousValue) {
      throw new Error(
        `Rust compiler CLI returned invalid ${payloadLabel} (${key} must be sorted in ascending order)`,
      );
    }
  }
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
  if ((protocolVersion as number) < 0) {
    throw new Error(
      `Rust compiler CLI returned negative protocol_version: ${String(
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
