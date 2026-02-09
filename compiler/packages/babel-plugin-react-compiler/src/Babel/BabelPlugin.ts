/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type * as BabelCore from '@babel/core';
import * as BabelParser from '@babel/parser';
import {NodePath} from '@babel/traverse';
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
): BabelParser.ParseResult<t.File> {
  const plugins: BabelParser.ParserPlugin[] = ['jsx'];
  if (dialect === 'typescript') {
    plugins.unshift('typescript');
  } else if (dialect === 'flow') {
    plugins.unshift('flow');
  }

  const parserOptions: BabelParser.ParserOptions = {
    sourceType: 'unambiguous',
    plugins,
  };
  if (filename != null) {
    parserOptions.sourceFilename = filename;
  }

  return BabelParser.parse(transformedCode, parserOptions);
}

function maybeRunRustProgramCompiler(
  prog: NodePath<t.Program>,
  pass: BabelCore.PluginPass,
): void {
  const sourceCode = pass.file.code ?? '';
  const dialect = detectRustDialect(pass.filename ?? null, sourceCode);
  const rustRequest: RustCompileRequest = {
    source: sourceCode,
    dialect,
    is_module: prog.node.sourceType === 'module',
  };
  if (pass.filename != null) {
    rustRequest.filename = pass.filename;
  }
  const rustResult = runRustCompilerCli(rustRequest);

  if (rustResult.status === 'error') {
    throw new Error(`[RustCompiler] ${rustResult.message}`);
  }

  if (rustResult.code === sourceCode) {
    return;
  }

  const parsed = parseProgramFromRustOutput(
    rustResult.code,
    pass.filename ?? null,
    dialect,
  );

  prog.node.body = parsed.program.body;
  prog.node.directives = parsed.program.directives;
  prog.node.sourceType = parsed.program.sourceType;
  prog.node.interpreter = parsed.program.interpreter ?? null;
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
              maybeRunRustProgramCompiler(prog, pass);
              markCompilationEnd(filename);
              return;
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
