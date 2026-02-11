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
import {
  runRustCompilerCli,
  type RustCompileRequest,
  type RustCompileResponse,
} from '../RustBridge/RustCli';

export type RustFrontendLogger = {
  logEvent: (filename: string | null, event: unknown) => void;
};

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

export function maybeRunRustProgramCompiler(
  prog: NodePath<t.Program>,
  pass: BabelCore.PluginPass,
  logger: RustFrontendLogger | null,
  filename: string | null,
): void {
  const sourceCode = pass.file.code ?? '';
  const sourceType = prog.node.sourceType === 'module' ? 'module' : 'script';
  const dialect = detectRustDialect(pass.filename ?? null);
  const rustRequest: RustCompileRequest = {
    source: sourceCode,
    dialect,
    is_module: prog.node.sourceType === 'module',
    apply_placeholder_transforms: true,
    emit_debug_ir: false,
  };
  if (pass.filename != null) {
    rustRequest.filename = pass.filename;
  }
  let rustResult: ReturnType<typeof runRustCompilerCli>;
  try {
    rustResult = runRustCompilerCli(rustRequest);
  } catch {
    const reason = 'rust_frontend_invocation_failure';
    logger?.logEvent(filename, {
      kind: 'PipelineError',
      fnLoc: null,
      data: `[RustCompiler:${reason}] failed to invoke rust compiler cli`,
    });
    throw new Error(`[RustCompiler:${reason}] failed to invoke rust compiler cli`);
  }

  if (rustResult.status === 'error') {
    logger?.logEvent(filename, {
      kind: 'PipelineError',
      fnLoc: null,
      data: `[RustCompiler:${rustResult.code}] ${rustResult.message}`,
    });
    throw new Error(`[RustCompiler:${rustResult.code}] ${rustResult.message}`);
  }
  maybeApplyStrictRustProgramReplacement(
    prog,
    pass,
    sourceCode,
    sourceType,
    dialect,
    rustResult,
  );
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
