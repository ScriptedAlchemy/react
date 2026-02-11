/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

export type LoggerEvent = {
  kind: string;
  [key: string]: unknown;
};

export type CompilerPipelineValue =
  | {kind: 'hir'; name: string; value: unknown}
  | {kind: 'reactive'; name: string; value: unknown}
  | {kind: 'debug'; name: string; value: string}
  | {kind: 'ast'; name: string; value: unknown};

export type Logger = {
  logEvent: (filename: string | null, event: LoggerEvent) => void;
  debugLogIRs: (value: CompilerPipelineValue) => void;
};

export type CompilerReactTarget = '17' | '18' | '19';

export type PluginOptions = {
  environment?: Record<string, unknown>;
  logger?: Logger;
  enableReanimatedCheck?: boolean;
  target?: CompilerReactTarget;
  [key: string]: unknown;
};

export enum Effect {
  Read = 'Read',
  Store = 'Store',
  Capture = 'Capture',
}

export enum ValueKind {
  Primitive = 'Primitive',
  Mutable = 'Mutable',
}

export enum ValueReason {
  KnownReturnSignature = 'KnownReturnSignature',
}

export type TypeConfig = {
  kind: string;
  [key: string]: unknown;
};

type ParseConfigPragmaDefaults = {
  compilationMode?: string;
};

type ParseConfigPragmaResult = {
  environment: Record<string, unknown>;
  compilationMode?: string;
  [key: string]: unknown;
};

export function parseConfigPragmaForTests(
  firstLine: string,
  defaults: ParseConfigPragmaDefaults = {},
): ParseConfigPragmaResult {
  const result: ParseConfigPragmaResult = {
    environment: {},
  };
  if (defaults.compilationMode != null) {
    result.compilationMode = defaults.compilationMode;
  }
  if (firstLine.includes('@compilationMode(infer)')) {
    result.compilationMode = 'infer';
  } else if (firstLine.includes('@compilationMode(all)')) {
    result.compilationMode = 'all';
  }
  return result;
}

function formatDebugValue(value: unknown): string {
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2) ?? String(value);
  } catch {
    return String(value);
  }
}

export function printFunctionWithOutlined(value: unknown): string {
  return formatDebugValue(value);
}

export function printReactiveFunctionWithOutlined(value: unknown): string {
  return formatDebugValue(value);
}

export const printReactiveFunction = printReactiveFunctionWithOutlined;
