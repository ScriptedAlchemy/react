/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type * as BabelCore from '@babel/core';
import generate from '@babel/generator';
import * as BabelParser from '@babel/parser';
import traverse, {NodePath} from '@babel/traverse';
import * as t from '@babel/types';
import {compileProgram, Logger, parsePluginOptions} from '../Entrypoint';
import {
  injectReanimatedFlag,
  pipelineUsesReanimatedPlugin,
} from '../Entrypoint/Reanimated';
import validateNoUntransformedReferences from '../Entrypoint/ValidateNoUntransformedReferences';
import {CompilerError} from '..';
import {
  runRustCompilerCli,
  type RustCompileRequest,
} from '../RustBridge/RustCli';

const ENABLE_REACT_COMPILER_TIMINGS =
  process.env['ENABLE_REACT_COMPILER_TIMINGS'] === '1';

function markCompilationEnd(filename: string): void {
  if (ENABLE_REACT_COMPILER_TIMINGS === true) {
    performance.mark(`${filename}:end`, {
      detail: 'BabelPlugin:Program:end',
    });
  }
}

function isStrictRustEngineEnabled(): boolean {
  return (
    process.env['REACT_COMPILER_RUST_STRICT'] === '1' ||
    process.env['REACT_COMPILER_RUST_STRICT'] === 'true'
  );
}

function isRecoverableRustFrontendErrorCode(code: string): boolean {
  return (
    code === 'unsupported_flow_syntax' ||
    code === 'parse_failure' ||
    code === 'codegen_failure' ||
    code === 'unsupported_dialect'
  );
}

function toBabelSourceLocation(
  location:
    | {
        start_line: number;
        start_column: number;
        end_line: number;
        end_column: number;
      }
    | null
    | undefined,
  sourceCode: string,
  filename: string | null,
): t.SourceLocation | null {
  if (location == null) {
    return null;
  }
  const startIndex = lineColumnToIndex(
    sourceCode,
    location.start_line,
    location.start_column,
  );
  const endIndex = lineColumnToIndex(
    sourceCode,
    location.end_line,
    location.end_column,
  );
  return {
    filename: filename ?? '',
    identifierName: '',
    start: {
      line: location.start_line,
      column: location.start_column,
      index: startIndex ?? 0,
    },
    end: {
      line: location.end_line,
      column: location.end_column,
      index: endIndex ?? startIndex ?? 0,
    },
  };
}

function lineColumnToIndex(
  sourceCode: string,
  line: number,
  column: number,
): number | null {
  if (line < 1 || column < 0) {
    return null;
  }
  let currentLine = 1;
  let offset = 0;
  while (currentLine < line) {
    const nextLineBreak = sourceCode.indexOf('\n', offset);
    if (nextLineBreak === -1) {
      return null;
    }
    offset = nextLineBreak + 1;
    currentLine += 1;
  }
  const lineEnd = sourceCode.indexOf('\n', offset);
  const effectiveLineEnd = lineEnd === -1 ? sourceCode.length : lineEnd;
  const maxColumn = effectiveLineEnd - offset;
  if (column > maxColumn) {
    return null;
  }
  return offset + column;
}

function logStrictRustFrontendFallback(
  logger: Logger | null,
  filename: string | null,
  reason: string,
  loc: t.SourceLocation | null = null,
): void {
  logger?.logEvent(filename, {
    kind: 'CompileSkip',
    fnLoc: null,
    reason,
    loc,
  });
}

function detectRustDialect(
  filename: string | null,
  sourceCode: string | null,
): 'javascript' | 'typescript' | 'flow' {
  if (filename != null && /\.(cts|mts|tsx|ts)$/i.test(filename)) {
    return 'typescript';
  }
  if (sourceCode != null && sourceCode.indexOf('@flow') !== -1) {
    return 'flow';
  }
  return 'javascript';
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

function stripTypeOnlyUnsupportedExpressions(
  ast: BabelParser.ParseResult<t.File>,
): void {
  traverse(ast, {
    TSInstantiationExpression(path) {
      path.replaceWith(path.node.expression);
    },
    TSSatisfiesExpression(path) {
      path.replaceWith(path.node.expression);
    },
  });
}

function canonicalizeProgramForComparison(
  code: string,
  filename: string | null,
  dialect: 'javascript' | 'typescript' | 'flow',
  sourceType: 'script' | 'module',
): string {
  const parsed = parseProgramFromRustOutput(code, filename, dialect, sourceType);
  stripTypeOnlyUnsupportedExpressions(parsed);
  return (
    generate(parsed, {
      comments: false,
      compact: true,
      minified: true,
      retainLines: false,
    }).code ?? ''
  );
}

function maybeRunRustProgramCompiler(
  prog: NodePath<t.Program>,
  pass: BabelCore.PluginPass,
  logger: Logger | null,
  filename: string | null,
  strictRustEngine: boolean,
): void {
  const sourceCode = pass.file.code ?? '';
  const sourceType = prog.node.sourceType === 'module' ? 'module' : 'script';
  const dialect = detectRustDialect(pass.filename ?? null, sourceCode);
  const rustRequest: RustCompileRequest = {
    source: sourceCode,
    dialect,
    is_module: prog.node.sourceType === 'module',
    apply_placeholder_transforms: false,
    emit_debug_ir: logger?.debugLogIRs != null,
  };
  if (pass.filename != null) {
    rustRequest.filename = pass.filename;
  }
  const rustResult = runRustCompilerCli(rustRequest);

  if (rustResult.status === 'error') {
    if (
      !strictRustEngine ||
      isRecoverableRustFrontendErrorCode(rustResult.code)
    ) {
      if (strictRustEngine) {
        logStrictRustFrontendFallback(
          logger,
          filename,
          `rust_frontend_error:${rustResult.code}:${rustResult.reason}`,
          toBabelSourceLocation(
            rustResult.location,
            sourceCode,
            pass.filename ?? null,
          ),
        );
      }
      return;
    }
    logger?.logEvent(filename, {
      kind: 'PipelineError',
      fnLoc: null,
      data: `[RustCompiler:${rustResult.code}] ${rustResult.message}`,
    });
    throw new Error(`[RustCompiler:${rustResult.code}] ${rustResult.message}`);
  }
  if (rustResult.debug_ir != null) {
    logger?.debugLogIRs?.({
      kind: 'debug',
      name: 'RustFrontendDebug',
      value: rustResult.debug_ir,
    });
  }
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransforms',
    value: rustResult.placeholder_transformed_functions.join(','),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendStatementCount',
    value: String(rustResult.statement_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendStatementCountAfterTransform',
    value: String(rustResult.statement_count_after_transform),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeHelperImportCountBeforeTransform',
    value: String(rustResult.placeholder_runtime_helper_import_count_before_transform),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeHelperImportCountAfterTransform',
    value: String(rustResult.placeholder_runtime_helper_import_count_after_transform),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeHelperImportAdded',
    value: String(rustResult.placeholder_runtime_helper_import_added),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformCandidates',
    value: rustResult.placeholder_transform_candidates.join(','),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformSkipped',
    value: rustResult.placeholder_transform_skipped_functions.join(','),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeCallee',
    value: rustResult.placeholder_runtime_callee_name ?? '',
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeCalleeBeforeTransform',
    value: rustResult.placeholder_runtime_callee_name_before_transform ?? '',
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeCalleeCandidates',
    value: rustResult.placeholder_runtime_callee_candidates.join(','),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeNamespaceCandidates',
    value: rustResult.placeholder_runtime_namespace_candidates.join(','),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeCalleeCandidatesBeforeTransform',
    value: rustResult.placeholder_runtime_callee_candidates_before_transform.join(
      ',',
    ),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeNamespaceCandidatesBeforeTransform',
    value: rustResult.placeholder_runtime_namespace_candidates_before_transform.join(
      ',',
    ),
  });
  if (!strictRustEngine || rustResult.code === sourceCode) {
    return;
  }
  let parsed: BabelParser.ParseResult<t.File>;
  let canonicalSource: string;
  let canonicalRustOutput: string;
  try {
    parsed = parseProgramFromRustOutput(
      rustResult.code,
      pass.filename ?? null,
      dialect,
      sourceType,
    );
    stripTypeOnlyUnsupportedExpressions(parsed);
    canonicalSource = canonicalizeProgramForComparison(
      sourceCode,
      pass.filename ?? null,
      dialect,
      sourceType,
    );
    canonicalRustOutput = canonicalizeProgramForComparison(
      rustResult.code,
      pass.filename ?? null,
      dialect,
      sourceType,
    );
  } catch {
    if (strictRustEngine) {
      logStrictRustFrontendFallback(
        logger,
        filename,
        'rust_frontend_parse_or_canonicalization_failure',
      );
    }
    return;
  }
  if (canonicalSource === canonicalRustOutput) {
    return;
  }

  prog.node.body = parsed.program.body;
  prog.node.directives = parsed.program.directives;
  prog.node.sourceType = parsed.program.sourceType;
  prog.node.interpreter = parsed.program.interpreter ?? null;
  prog.scope.crawl();
}

/*
 * The React Forget Babel Plugin
 * @param {*} _babel
 * @returns
 */
export default function BabelPluginReactCompiler(
  _babel: typeof BabelCore,
): BabelCore.PluginObj {
  return {
    name: 'react-forget',
    visitor: {
      /*
       * Note: Babel does some "smart" merging of visitors across plugins, so even if A is inserted
       * prior to B, if A does not have a Program visitor and B does, B will run first. We always
       * want Forget to run true to source as possible.
       */
      Program: {
        enter(prog, pass): void {
          try {
            const filename = pass.filename ?? 'unknown';
            if (ENABLE_REACT_COMPILER_TIMINGS === true) {
              performance.mark(`${filename}:start`, {
                detail: 'BabelPlugin:Program:start',
              });
            }
            let opts = parsePluginOptions(pass.opts);
            const isDev =
              (typeof __DEV__ !== 'undefined' && __DEV__ === true) ||
              process.env['NODE_ENV'] === 'development';
            if (
              opts.enableReanimatedCheck === true &&
              pipelineUsesReanimatedPlugin(pass.file.opts.plugins)
            ) {
              opts = injectReanimatedFlag(opts);
            }
            if (
              opts.environment.enableResetCacheOnSourceFileChanges !== false &&
              isDev
            ) {
              opts = {
                ...opts,
                environment: {
                  ...opts.environment,
                  enableResetCacheOnSourceFileChanges: true,
                },
              };
            }
            if (opts.compilerEngine === 'rust') {
              const strictRustEngine = isStrictRustEngineEnabled();
              maybeRunRustProgramCompiler(
                prog,
                pass,
                opts.logger,
                pass.filename ?? null,
                strictRustEngine,
              );
            }
            const result = compileProgram(prog, {
              opts,
              filename: pass.filename ?? null,
              comments: pass.file.ast.comments ?? [],
              code: pass.file.code,
            });
            validateNoUntransformedReferences(
              prog,
              pass.filename ?? null,
              opts.logger,
              opts.environment,
              result,
            );
            markCompilationEnd(filename);
          } catch (e) {
            if (e instanceof CompilerError) {
              throw e.withPrintedMessage(pass.file.code, {eslint: false});
            }
            throw e;
          }
        },
        exit(_, pass): void {
          if (ENABLE_REACT_COMPILER_TIMINGS === true) {
            const filename = pass.filename ?? 'unknown';
            const measurement = performance.measure(filename, {
              start: `${filename}:start`,
              end: `${filename}:end`,
              detail: 'BabelPlugin:Program',
            });
            if ('logger' in pass.opts && pass.opts.logger != null) {
              const logger: Logger = pass.opts.logger as Logger;
              logger.logEvent(filename, {
                kind: 'Timing',
                measurement,
              });
            }
          }
        },
      },
    },
  };
}
