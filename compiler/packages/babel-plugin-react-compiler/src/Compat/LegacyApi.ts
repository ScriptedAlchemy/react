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
  debugLogIRs?: (value: CompilerPipelineValue) => void;
};

export type CompilerReactTarget = '17' | '18' | '19';

export type PluginOptions = {
  environment?: Record<string, unknown>;
  logger?: Logger;
  enableReanimatedCheck?: boolean;
  target?: CompilerReactTarget;
  [key: string]: unknown;
};

export enum CompilerSuggestionOperation {
  InsertBefore,
  InsertAfter,
  Remove,
  Replace,
}

export type CompilerSuggestion =
  | {
      op:
        | CompilerSuggestionOperation.InsertBefore
        | CompilerSuggestionOperation.InsertAfter
        | CompilerSuggestionOperation.Replace;
      range: [number, number];
      description: string;
      text: string;
    }
  | {
      op: CompilerSuggestionOperation.Remove;
      range: [number, number];
      description: string;
    };

export type CompilerDiagnosticOptions = {
  category: string;
  reason: string;
  description: string | null;
  suggestions?: Array<CompilerSuggestion> | null | undefined;
  [key: string]: unknown;
};

export type CompilerErrorDetailOptions = CompilerDiagnosticOptions;

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

export function parsePluginOptions(options: PluginOptions): PluginOptions {
  return {...options};
}

export function validateEnvironmentConfig(
  environment: Record<string, unknown>,
): Record<string, unknown> {
  return {...environment};
}

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

export enum ErrorSeverity {
  Error = 'Error',
  Warning = 'Warning',
  Hint = 'Hint',
  Off = 'Off',
}

export enum LintRulePreset {
  Recommended = 'Recommended',
  RecommendedLatest = 'RecommendedLatest',
  Off = 'Off',
}

export type LintRule = {
  name: string;
  category: string;
  description: string;
  severity: ErrorSeverity;
  preset: LintRulePreset;
};

export const LintRules: Array<LintRule> = [
  {
    name: 'gating',
    category: 'Gating',
    description: 'Validates React Compiler gating configuration',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'globals',
    category: 'Globals',
    description: 'Validates against mutating globals during render',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'hooks',
    category: 'Hooks',
    description: 'Validates Rules of Hooks constraints',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Off,
  },
  {
    name: 'immutability',
    category: 'Immutability',
    description: 'Validates immutability of props/state values',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'incompatible-library',
    category: 'IncompatibleLibrary',
    description: 'Validates incompatible library usage for memoization',
    severity: ErrorSeverity.Warning,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'preserve-manual-memoization',
    category: 'PreserveManualMemo',
    description: 'Validates preserving manual memoization guarantees',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'purity',
    category: 'Purity',
    description: 'Validates render-time purity',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'refs',
    category: 'Refs',
    description: 'Validates correct ref access patterns',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'set-state-in-effect',
    category: 'EffectSetState',
    description: 'Validates against synchronous setState in effects',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'set-state-in-render',
    category: 'RenderSetState',
    description: 'Validates against setState during render',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'static-components',
    category: 'StaticComponents',
    description: 'Validates components are static across renders',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'unsupported-syntax',
    category: 'UnsupportedSyntax',
    description: 'Validates syntax unsupported by React Compiler',
    severity: ErrorSeverity.Warning,
    preset: LintRulePreset.Recommended,
  },
  {
    name: 'void-use-memo',
    category: 'VoidUseMemo',
    description: 'Validates useMemo return value usage',
    severity: ErrorSeverity.Error,
    preset: LintRulePreset.RecommendedLatest,
  },
];
