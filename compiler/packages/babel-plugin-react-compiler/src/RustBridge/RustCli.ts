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
  dialect?: 'javascript' | 'typescript' | 'flow';
  is_module?: boolean;
  apply_placeholder_transforms?: boolean;
  emit_debug_ir?: boolean;
  protocol_version?: number;
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
      react_functions: Array<{
        name: string;
        kind: 'Component' | 'Hook';
        loc: null | RustSourceLocation;
      }>;
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
  const requestPayload: RustCompileRequest = {
    ...request,
    protocol_version:
      request.protocol_version ?? RUST_CLI_PROTOCOL_VERSION,
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
  const status = (response as {status?: unknown}).status;
  if (status !== 'ok' && status !== 'error') {
    throw new Error(
      `Rust compiler CLI returned invalid status: ${String(status)}`,
    );
  }
  if (status === 'ok') {
    const {
      code,
      statement_count,
      statement_count_after_transform,
      detected_react_functions,
      placeholder_transforms_applied,
      placeholder_transform_candidates,
      placeholder_transformed_functions,
      react_functions,
    } = response as {
      code?: unknown;
      statement_count?: unknown;
      statement_count_after_transform?: unknown;
      detected_react_functions?: unknown;
      placeholder_transforms_applied?: unknown;
      placeholder_transform_candidates?: unknown;
      placeholder_transformed_functions?: unknown;
      react_functions?: unknown;
    };
    const hasStringArray = (value: unknown): boolean =>
      Array.isArray(value) && value.every(entry => typeof entry === 'string');
    const hasValidSourceLocation = (value: unknown): boolean => {
      if (value == null) {
        return true;
      }
      if (typeof value !== 'object') {
        return false;
      }
      const {
        start_line,
        start_column,
        end_line,
        end_column,
      } = value as {
        start_line?: unknown;
        start_column?: unknown;
        end_line?: unknown;
        end_column?: unknown;
      };
      return (
        typeof start_line === 'number' &&
        typeof start_column === 'number' &&
        typeof end_line === 'number' &&
        typeof end_column === 'number'
      );
    };
    const hasValidReactFunctions = (value: unknown): boolean =>
      Array.isArray(value) &&
      value.every(
        item =>
          item != null &&
          typeof item === 'object' &&
          typeof (item as {name?: unknown}).name === 'string' &&
          ((item as {kind?: unknown}).kind === 'Component' ||
            (item as {kind?: unknown}).kind === 'Hook') &&
          hasValidSourceLocation((item as {loc?: unknown}).loc),
      );
    if (
      typeof code !== 'string' ||
      typeof statement_count !== 'number' ||
      typeof statement_count_after_transform !== 'number' ||
      typeof detected_react_functions !== 'number' ||
      typeof placeholder_transforms_applied !== 'number' ||
      !hasStringArray(placeholder_transform_candidates) ||
      !hasStringArray(placeholder_transformed_functions) ||
      !hasValidReactFunctions(react_functions)
    ) {
      throw new Error(
        'Rust compiler CLI returned invalid ok payload (missing required typed fields)',
      );
    }
    return;
  }
  const {
    code,
    category,
    reason,
    severity,
    message,
    location,
  } = response as {
    code?: unknown;
    category?: unknown;
    reason?: unknown;
    severity?: unknown;
    message?: unknown;
    location?: unknown;
  };
  const hasValidSourceLocation = (value: unknown): boolean => {
    if (value == null) {
      return true;
    }
    if (typeof value !== 'object') {
      return false;
    }
    const {
      start_line,
      start_column,
      end_line,
      end_column,
    } = value as {
      start_line?: unknown;
      start_column?: unknown;
      end_line?: unknown;
      end_column?: unknown;
    };
    return (
      typeof start_line === 'number' &&
      typeof start_column === 'number' &&
      typeof end_line === 'number' &&
      typeof end_column === 'number'
    );
  };
  if (
    typeof code !== 'string' ||
    typeof category !== 'string' ||
    typeof reason !== 'string' ||
    typeof severity !== 'string' ||
    typeof message !== 'string' ||
    !hasValidSourceLocation(location)
  ) {
    throw new Error(
      'Rust compiler CLI returned invalid error payload (missing required string fields)',
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
