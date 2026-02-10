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

function logStrictRustFrontendFallback(
  logger: Logger | null,
  filename: string | null,
  reason: string,
): void {
  logger?.logEvent(filename, {
    kind: 'CompileSkip',
    fnLoc: null,
    reason,
    loc: null,
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

function hasObviousFlowTypeSyntax(sourceCode: string): boolean {
  const flowTypeMarkers = [
    /\bimport\s+type\b/,
    /\bexport\s+type\b/,
    /\bopaque\s+type\b/,
    /\binterface\s+[A-Za-z_$]/,
    /\bdeclare\s+(class|function|module|var|type|interface)\b/,
    /\btype\s+[A-Za-z_$][\w$]*\s*=/,
  ];
  return flowTypeMarkers.some(pattern => pattern.test(sourceCode));
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
  if (dialect === 'flow' && hasObviousFlowTypeSyntax(sourceCode)) {
    if (strictRustEngine) {
      logStrictRustFrontendFallback(
        logger,
        filename,
        'rust_frontend_error:unsupported_flow_syntax:flow_syntax_not_supported',
      );
    }
    return;
  }
  const rustRequest: RustCompileRequest = {
    source: sourceCode,
    dialect,
    is_module: prog.node.sourceType === 'module',
    apply_placeholder_transforms: false,
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
