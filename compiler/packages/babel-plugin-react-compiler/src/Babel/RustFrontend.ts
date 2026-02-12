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
} from '../RustBridge/RustCli';

function detectRustDialect(
  filename: string | null,
): 'javascript' | 'typescript' {
  if (filename != null && /\.(cts|mts|tsx|ts)$/i.test(filename)) {
    return 'typescript';
  }
  return 'javascript';
}

function parseProgramFromRustOutput(
  transformedCode: string,
  filename: string | null,
  dialect: 'javascript' | 'typescript',
  sourceType: 'script' | 'module',
): BabelParser.ParseResult<t.File> {
  const plugins: Array<BabelParser.ParserPlugin> = ['jsx'];
  if (dialect === 'typescript') {
    plugins.unshift('typescript');
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
  location: Extract<RustCompileResponse, {status: 'error'}>['location'],
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

function createRustCompileErrorDetail(
  result: Extract<RustCompileResponse, {status: 'error'}>,
  filename: string | null,
): CompilerErrorDetailOptions {
  const legacyCategory =
    result.category === 'syntax'
      ? ErrorCategory.Syntax
      : result.category === 'internal'
        ? ErrorCategory.Invariant
        : ErrorCategory.Invariant;
  const severity =
    result.category === 'request'
      ? ErrorSeverity.InvalidConfig
      : result.category === 'internal'
        ? ErrorSeverity.Invariant
        : ErrorSeverity.InvalidJS;
  const loc = toLegacySourceLocation(result.location, filename);
  return {
    category: legacyCategory,
    reason: result.reason,
    description: result.message,
    severity,
    loc,
    rustCategory: result.category,
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
  const dialect = detectRustDialect(filename);
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
  emitLoggerEvent(logger, filename, {
    kind: 'CompileSuccess',
    fnLoc: null,
  });
}

function maybeApplyStrictRustProgramReplacement(
  prog: NodePath<t.Program>,
  pass: BabelCore.PluginPass,
  sourceCode: string,
  sourceType: 'script' | 'module',
  dialect: 'javascript' | 'typescript',
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
  } catch {
    const reason = 'rust_frontend_parse_failure';
    const logger = getRustFrontendLogger(pass);
    emitLoggerEvent(logger, pass.filename ?? null, {
      kind: 'PipelineError',
      fnLoc: null,
      data: `[RustCompiler:${reason}] failed to parse rust output for replacement`,
    });
    throw new Error(
      `[RustCompiler:${reason}] failed to parse rust output for replacement`,
    );
  }

  prog.node.body = parsed.program.body;
  prog.node.directives = parsed.program.directives;
  prog.node.sourceType = parsed.program.sourceType;
  prog.node.interpreter = parsed.program.interpreter ?? null;
  prog.scope.crawl();
}
