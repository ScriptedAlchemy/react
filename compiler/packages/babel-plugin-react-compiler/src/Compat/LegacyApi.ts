/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

export type LoggerEvent =
  | {
      kind: 'CompileSuccess';
      fnLoc?: SourceLocation | null;
      [key: string]: unknown;
    }
  | {
      kind: 'CompileError';
      detail: CompilerErrorDetailOptions;
      fnLoc?: SourceLocation | null;
      [key: string]: unknown;
    }
  | {
      kind: 'CompileDiagnostic' | 'PipelineError';
      detail?: CompilerErrorDetailOptions;
      fnLoc?: SourceLocation | null;
      [key: string]: unknown;
    }
  | {
      kind: string;
      [key: string]: unknown;
    };

export type CompileSuccessEvent = Extract<LoggerEvent, {kind: 'CompileSuccess'}>;
export type CompileErrorEvent = Extract<LoggerEvent, {kind: 'CompileError'}>;
export type PipelineErrorEvent = Extract<LoggerEvent, {kind: 'PipelineError'}>;

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

export type SourceLocation =
  | {
      start: {line: number; column: number};
      end: {line: number; column: number};
      filename?: string | null;
    }
  | symbol;

export type PrintErrorMessageOptions = {
  eslint: boolean;
};

export type CompilerDiagnosticOptions = {
  category: string;
  reason: string;
  description: string | null;
  loc?: SourceLocation | null;
  severity?: string;
  primaryLocation?: () => SourceLocation | null;
  printErrorMessage?: (
    source: string,
    options: PrintErrorMessageOptions,
  ) => string;
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
  InvalidReact = 'InvalidReact',
  InvalidJS = 'InvalidJS',
  InvalidConfig = 'InvalidConfig',
  Invariant = 'Invariant',
  CannotPreserveMemoization = 'CannotPreserveMemoization',
  Todo = 'Todo',
}

export enum LintRulePreset {
  Recommended = 'Recommended',
  RecommendedLatest = 'RecommendedLatest',
  Off = 'Off',
}

export enum ErrorCategory {
  CapitalizedCalls = 'CapitalizedCalls',
  EffectSetState = 'EffectSetState',
  ErrorBoundaries = 'ErrorBoundaries',
  Gating = 'Gating',
  Globals = 'Globals',
  Hooks = 'Hooks',
  Immutability = 'Immutability',
  IncompatibleLibrary = 'IncompatibleLibrary',
  Invariant = 'Invariant',
  PreserveManualMemo = 'PreserveManualMemo',
  Purity = 'Purity',
  Refs = 'Refs',
  RenderSetState = 'RenderSetState',
  StaticComponents = 'StaticComponents',
  Syntax = 'Syntax',
  Todo = 'Todo',
  UnsupportedSyntax = 'UnsupportedSyntax',
  VoidUseMemo = 'VoidUseMemo',
}

export type LintRule = {
  name: string;
  category: ErrorCategory;
  description: string;
  severity: ErrorSeverity;
  preset: LintRulePreset;
};

export function getRuleForCategory(category: ErrorCategory): LintRule {
  switch (category) {
    case ErrorCategory.CapitalizedCalls:
      return {
        name: 'capitalized-calls',
        category,
        description: 'Validates against invalid capitalized function calls',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Off,
      };
    case ErrorCategory.EffectSetState:
      return {
        name: 'set-state-in-effect',
        category,
        description: 'Validates against synchronous setState in effects',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.ErrorBoundaries:
      return {
        name: 'error-boundaries',
        category,
        description:
          'Validates usage of error boundaries instead of try/catch in child components',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.Gating:
      return {
        name: 'gating',
        category,
        description: 'Validates React Compiler gating configuration',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.Globals:
      return {
        name: 'globals',
        category,
        description: 'Validates against mutating globals during render',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.Hooks:
      return {
        name: 'hooks',
        category,
        description: 'Validates Rules of Hooks constraints',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Off,
      };
    case ErrorCategory.Immutability:
      return {
        name: 'immutability',
        category,
        description: 'Validates immutability of props/state values',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.IncompatibleLibrary:
      return {
        name: 'incompatible-library',
        category,
        description: 'Validates incompatible library usage for memoization',
        severity: ErrorSeverity.Warning,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.Invariant:
      return {
        name: 'invariant',
        category,
        description: 'Internal invariants',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Off,
      };
    case ErrorCategory.PreserveManualMemo:
      return {
        name: 'preserve-manual-memoization',
        category,
        description: 'Validates preserving manual memoization guarantees',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.Purity:
      return {
        name: 'purity',
        category,
        description: 'Validates render-time purity',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.Refs:
      return {
        name: 'refs',
        category,
        description: 'Validates correct ref access patterns',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.RenderSetState:
      return {
        name: 'set-state-in-render',
        category,
        description: 'Validates against setState during render',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.StaticComponents:
      return {
        name: 'static-components',
        category,
        description: 'Validates components are static across renders',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.Syntax:
      return {
        name: 'syntax',
        category,
        description: 'Validates syntax errors',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.Off,
      };
    case ErrorCategory.Todo:
      return {
        name: 'todo',
        category,
        description: 'Tracks unimplemented compiler work items',
        severity: ErrorSeverity.Hint,
        preset: LintRulePreset.Off,
      };
    case ErrorCategory.UnsupportedSyntax:
      return {
        name: 'unsupported-syntax',
        category,
        description: 'Validates syntax unsupported by React Compiler',
        severity: ErrorSeverity.Warning,
        preset: LintRulePreset.Recommended,
      };
    case ErrorCategory.VoidUseMemo:
      return {
        name: 'void-use-memo',
        category,
        description: 'Validates useMemo return value usage',
        severity: ErrorSeverity.Error,
        preset: LintRulePreset.RecommendedLatest,
      };
    default: {
      return {
        name: String(category).toLowerCase(),
        category,
        description: 'Unknown lint rule category',
        severity: ErrorSeverity.Warning,
        preset: LintRulePreset.Off,
      };
    }
  }
}

export const LintRules: Array<LintRule> = Object.values(ErrorCategory).map(
  category => getRuleForCategory(category),
);
