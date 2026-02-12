/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type * as BabelCore from '@babel/core';
import * as BabelParser from '@babel/parser';
import {NodePath} from '@babel/traverse';
import type * as t from '@babel/types';
import {
  type CompilerErrorDetailOptions,
  ErrorCategory,
  ErrorSeverity,
  type Logger,
  type SourceLocation,
} from '../Compat/LegacyApi';
import {
  runRustCompilerCli,
  type RustCompileRequest,
  type RustCompileResponse,
  type RustSourceLocation,
} from '../RustBridge/RustCli';

function getParserPluginNames(pass: BabelCore.PluginPass): Array<string> {
  const parserOpts = (
    pass.file as {
      opts?: {
        parserOpts?: {plugins?: Array<unknown>} | null;
      };
    }
  )?.opts?.parserOpts;
  const parserPlugins = parserOpts?.plugins;
  if (!Array.isArray(parserPlugins)) {
    return [];
  }
  const names: Array<string> = [];
  parserPlugins.forEach(pluginEntry => {
    if (typeof pluginEntry === 'string') {
      names.push(pluginEntry);
      return;
    }
    if (Array.isArray(pluginEntry) && pluginEntry.length > 0) {
      if (typeof pluginEntry[0] === 'string') {
        names.push(pluginEntry[0]);
      }
      return;
    }
  });
  return names;
}

function detectRustDialectFromParserPlugins(
  pass: BabelCore.PluginPass,
): 'javascript' | 'typescript' | 'flow' {
  const parserPluginNames = getParserPluginNames(pass);
  for (const pluginName of parserPluginNames) {
    if (pluginName === 'flow') {
      return 'flow';
    }
    if (pluginName === 'typescript') {
      return 'typescript';
    }
  }
  return 'javascript';
}

function detectRustDialect(
  filename: string | null,
  pass: BabelCore.PluginPass,
): 'javascript' | 'typescript' | 'flow' {
  if (filename != null && /\.(cts|mts|tsx|ts)$/i.test(filename)) {
    return 'typescript';
  }
  if (filename != null && /\.flow$/i.test(filename)) {
    return 'flow';
  }
  return detectRustDialectFromParserPlugins(pass);
}

function parseProgramFromRustOutput(
  transformedCode: string,
  filename: string | null,
  dialect: 'javascript' | 'typescript' | 'flow',
  sourceType: 'script' | 'module',
): BabelParser.ParseResult<t.File> {
  const plugins: Array<BabelParser.ParserPlugin> = ['jsx'];
  if (dialect === 'typescript') {
    plugins.unshift('typescript');
  } else if (dialect === 'flow') {
    plugins.unshift('flow');
  }

  const parserOptions: BabelParser.ParserOptions = {
    sourceType,
    plugins,
  };
  if (filename != null) {
    parserOptions.sourceFilename = filename;
  }

  return BabelParser.parse(transformedCode, parserOptions);
}

function getRustFrontendLogger(pass: BabelCore.PluginPass): Logger | null {
  const options = pass.opts as {logger?: unknown} | null | undefined;
  if (options == null || options.logger == null) {
    return null;
  }
  const candidate = options.logger as Partial<Logger>;
  if (typeof candidate.logEvent !== 'function') {
    return null;
  }
  if (typeof candidate.debugLogIRs === 'function') {
    return {
      logEvent: candidate.logEvent,
      debugLogIRs: candidate.debugLogIRs,
    };
  }
  return {
    logEvent: candidate.logEvent,
  };
}

function emitLoggerEvent(
  logger: Logger | null,
  filename: string | null,
  event: unknown,
): void {
  logger?.logEvent(filename, event as Parameters<Logger['logEvent']>[1]);
}

function toLegacySourceLocation(
  location: RustSourceLocation | null | undefined,
  filename: string | null,
): SourceLocation | null {
  if (location == null) {
    return null;
  }
  return {
    start: {
      line: location.start_line,
      column: location.start_column,
    },
    end: {
      line: location.end_line,
      column: location.end_column,
    },
    filename,
  };
}

function emitCompileSuccessEvents(
  logger: Logger | null,
  filename: string | null,
  result: Extract<RustCompileResponse, {status: 'ok'}>,
): void {
  const reactFunctions = result.react_functions;
  if (!Array.isArray(reactFunctions) || reactFunctions.length === 0) {
    emitLoggerEvent(logger, filename, {
      kind: 'CompileSuccess',
      fnLoc: null,
    });
    return;
  }
  for (const reactFunction of reactFunctions) {
    emitLoggerEvent(logger, filename, {
      kind: 'CompileSuccess',
      fnLoc: toLegacySourceLocation(reactFunction.loc, filename),
      fnName: reactFunction.name,
      fnKind: reactFunction.kind,
    });
  }
}

function mapRustCategoryToLegacyCategory(category: string): ErrorCategory {
  switch (category) {
    case 'syntax':
      return ErrorCategory.Syntax;
    case 'internal':
    case 'request':
    default:
      return ErrorCategory.Invariant;
  }
}

function mapRustCategoryToLegacySeverity(category: string): ErrorSeverity {
  switch (category) {
    case 'request':
      return ErrorSeverity.InvalidConfig;
    case 'internal':
      return ErrorSeverity.Invariant;
    default:
      return ErrorSeverity.InvalidJS;
  }
}

function mapRustErrorResultToLegacySeverity(
  result: Extract<RustCompileResponse, {status: 'error'}>,
): ErrorSeverity {
  const categoryMappedSeverity = mapRustCategoryToLegacySeverity(result.category);
  if (
    categoryMappedSeverity === ErrorSeverity.InvalidConfig ||
    categoryMappedSeverity === ErrorSeverity.Invariant
  ) {
    return categoryMappedSeverity;
  }
  switch (result.severity.toLowerCase()) {
    case 'warning':
      return ErrorSeverity.Warning;
    case 'hint':
      return ErrorSeverity.Hint;
    case 'off':
      return ErrorSeverity.Off;
    default:
      return categoryMappedSeverity;
  }
}

function mapRustErrorCodeToLegacyCategory(
  code: string,
  fallbackCategory: ErrorCategory,
): ErrorCategory {
  switch (code) {
    case 'unsupported_flow_syntax':
      return ErrorCategory.UnsupportedSyntax;
    default:
      return fallbackCategory;
  }
}

function createRustCompileErrorDetail(
  result: Extract<RustCompileResponse, {status: 'error'}>,
  filename: string | null,
): CompilerErrorDetailOptions {
  const legacyCategory = mapRustErrorCodeToLegacyCategory(
    result.code,
    mapRustCategoryToLegacyCategory(result.category),
  );
  const severity = mapRustErrorResultToLegacySeverity(result);
  const loc = toLegacySourceLocation(result.location, filename);
  return {
    category: legacyCategory,
    code: result.code,
    reason: result.reason,
    description: result.message,
    severity,
    loc,
    rustCategory: result.category,
    rustSeverity: result.severity,
    suggestions: null,
    options: {
      suggestions: null,
    },
    primaryLocation: () => loc,
    printErrorMessage: () => `[${result.category}] ${result.message}`,
  };
}

export function maybeRunRustProgramCompiler(
  prog: NodePath<t.Program>,
  pass: BabelCore.PluginPass,
): void {
  const sourceCode = pass.file.code ?? '';
  const filename = pass.filename ?? null;
  const logger = getRustFrontendLogger(pass);
  const emitDebugIr = logger?.debugLogIRs != null;
  const sourceType = prog.node.sourceType === 'module' ? 'module' : 'script';
  const dialect = detectRustDialect(filename, pass);
  const rustRequest: RustCompileRequest = {
    source: sourceCode,
    dialect,
    is_module: prog.node.sourceType === 'module',
    apply_placeholder_transforms: true,
    emit_debug_ir: emitDebugIr,
  };
  if (filename != null) {
    rustRequest.filename = filename;
  }
  let rustResult: ReturnType<typeof runRustCompilerCli>;
  try {
    rustResult = runRustCompilerCli(rustRequest);
  } catch (error) {
    const causeMessage =
      error instanceof Error ? error.message : String(error);
    const message =
      '[RustCompiler:rust_frontend_invocation_failure] failed to invoke rust compiler cli: ' +
      causeMessage;
    emitLoggerEvent(logger, filename, {
      kind: 'PipelineError',
      fnLoc: null,
      data: message,
    });
    throw new Error(message);
  }

  if (rustResult.status === 'error') {
    const detail = createRustCompileErrorDetail(rustResult, filename);
    emitLoggerEvent(logger, filename, {
      kind: 'CompileError',
      fnLoc:
        detail.primaryLocation?.() ??
        (detail.loc != null ? detail.loc : null),
      detail,
    });
    throw new Error(`[RustCompiler:${rustResult.code}] ${rustResult.message}`);
  }
  if (emitDebugIr && typeof rustResult.debug_ir === 'string') {
    logger?.debugLogIRs?.({
      kind: 'debug',
      name: 'RustFrontendDebugIR',
      value: rustResult.debug_ir,
    });
  }
  maybeApplyStrictRustProgramReplacement(
    prog,
    pass,
    sourceCode,
    sourceType,
    dialect,
    rustResult,
  );
  emitCompileSuccessEvents(logger, filename, rustResult);
}

function maybeApplyStrictRustProgramReplacement(
  prog: NodePath<t.Program>,
  pass: BabelCore.PluginPass,
  sourceCode: string,
  sourceType: 'script' | 'module',
  dialect: 'javascript' | 'typescript' | 'flow',
  rustResult: Extract<RustCompileResponse, {status: 'ok'}>,
): void {
  if (rustResult.code === sourceCode) {
    return;
  }
  let parsed: BabelParser.ParseResult<t.File>;
  try {
    parsed = parseProgramFromRustOutput(
      rustResult.code,
      pass.filename ?? null,
      dialect,
      sourceType,
    );
  } catch (error) {
    const reason = 'rust_frontend_parse_failure';
    const causeMessage =
      error instanceof Error ? error.message : String(error);
    const message = `[RustCompiler:${reason}] failed to parse rust output for replacement: ${causeMessage}`;
    const logger = getRustFrontendLogger(pass);
    emitLoggerEvent(logger, pass.filename ?? null, {
      kind: 'PipelineError',
      fnLoc: null,
      data: message,
    });
    throw new Error(message);
  }

  prog.node.body = parsed.program.body;
  prog.node.directives = parsed.program.directives;
  prog.node.sourceType = parsed.program.sourceType;
  prog.node.interpreter = parsed.program.interpreter ?? null;
  prog.scope.crawl();
}
