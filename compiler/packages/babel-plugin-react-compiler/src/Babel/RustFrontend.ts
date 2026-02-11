/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type * as BabelCore from '@babel/core';
import * as BabelParser from '@babel/parser';
import traverse, {NodePath} from '@babel/traverse';
import * as t from '@babel/types';
import {
  runRustCompilerCli,
  type RustCompileRequest,
  type RustCompileResponse,
} from '../RustBridge/RustCli';

type RustFrontendDebugValue = {
  kind?: string;
  name?: string;
  value?: string;
};

export type RustFrontendLogger = {
  logEvent: (filename: string | null, event: unknown) => void;
  debugLogIRs?: (value: RustFrontendDebugValue) => void;
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
    emit_debug_ir: logger?.debugLogIRs != null,
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
  emitRustFrontendDebugTelemetry(logger, rustResult);
  maybeApplyStrictRustProgramReplacement(
    prog,
    pass,
    logger,
    filename,
    sourceCode,
    sourceType,
    dialect,
    rustResult,
  );
}

function emitRustFrontendDebugTelemetry(
  logger: RustFrontendLogger | null,
  rustResult: Extract<RustCompileResponse, {status: 'ok'}>,
): void {
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
    name: 'RustFrontendProtocolVersion',
    value:
      rustResult.protocol_version != null
        ? String(rustResult.protocol_version)
        : '',
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
    name: 'RustFrontendPlaceholderTransformCandidateCount',
    value: String(rustResult.placeholder_transform_candidate_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformSkippedCount',
    value: String(rustResult.placeholder_transform_skipped_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformCandidateComponentCount',
    value: String(rustResult.placeholder_transform_candidate_component_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformCandidateHookCount',
    value: String(rustResult.placeholder_transform_candidate_hook_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformTransformedComponentCount',
    value: String(rustResult.placeholder_transform_transformed_component_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformTransformedHookCount',
    value: String(rustResult.placeholder_transform_transformed_hook_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformSkippedComponentCount',
    value: String(rustResult.placeholder_transform_skipped_component_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformSkippedHookCount',
    value: String(rustResult.placeholder_transform_skipped_hook_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendDetectedComponentFunctionCount',
    value: String(rustResult.detected_component_function_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendDetectedHookFunctionCount',
    value: String(rustResult.detected_hook_function_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendDetectedComponentFunctions',
    value: rustResult.detected_component_functions.join(','),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendDetectedHookFunctions',
    value: rustResult.detected_hook_functions.join(','),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeCalleeReused',
    value: String(rustResult.placeholder_runtime_callee_reused),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeCalleeGenerated',
    value: String(rustResult.placeholder_runtime_callee_generated),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendPlaceholderTransformStatus',
    value: rustResult.placeholder_transform_status,
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
    name: 'RustFrontendRuntimeCalleeCandidateCount',
    value: String(rustResult.placeholder_runtime_callee_candidate_count),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeNamespaceCandidates',
    value: rustResult.placeholder_runtime_namespace_candidates.join(','),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeNamespaceCandidateCount',
    value: String(rustResult.placeholder_runtime_namespace_candidate_count),
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
    name: 'RustFrontendRuntimeCalleeCandidateCountBeforeTransform',
    value: String(
      rustResult.placeholder_runtime_callee_candidate_count_before_transform,
    ),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeNamespaceCandidatesBeforeTransform',
    value: rustResult.placeholder_runtime_namespace_candidates_before_transform.join(
      ',',
    ),
  });
  logger?.debugLogIRs?.({
    kind: 'debug',
    name: 'RustFrontendRuntimeNamespaceCandidateCountBeforeTransform',
    value: String(
      rustResult.placeholder_runtime_namespace_candidate_count_before_transform,
    ),
  });
}

function maybeApplyStrictRustProgramReplacement(
  prog: NodePath<t.Program>,
  pass: BabelCore.PluginPass,
  logger: RustFrontendLogger | null,
  filename: string | null,
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
    stripTypeOnlyUnsupportedExpressions(parsed);
  } catch {
    const reason = 'rust_frontend_parse_failure';
    logger?.logEvent(filename, {
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
