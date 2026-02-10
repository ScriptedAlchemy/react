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

function findObviousFlowTypeSyntaxMarker(
  sourceCode: string,
  sourceType: 'script' | 'module',
): null | {index: number; length: number; kind: string} {
  try {
    const ast = BabelParser.parse(sourceCode, {
      sourceType,
      plugins: ['flow', 'jsx'],
    });
    let firstMatch: null | {index: number; length: number; kind: string} = null;
    const recordMarker = (
      kind: string,
      node: {start?: number | null; end?: number | null},
    ): void => {
      if (node.start == null || node.end == null) {
        return;
      }
      const candidate = {
        index: node.start,
        length: Math.max(node.end - node.start, 1),
        kind,
      };
      if (firstMatch == null || candidate.index < firstMatch.index) {
        firstMatch = candidate;
      }
    };
    traverse(ast, {
      TypeAlias(path) {
        recordMarker('type_alias', path.node);
      },
      OpaqueType(path) {
        recordMarker('opaque_type', path.node);
      },
      InterfaceDeclaration(path) {
        recordMarker('interface', path.node);
      },
      DeclareClass(path) {
        recordMarker('declare', path.node);
      },
      DeclareFunction(path) {
        recordMarker('declare', path.node);
      },
      DeclareModule(path) {
        recordMarker('declare', path.node);
      },
      DeclareVariable(path) {
        recordMarker('declare', path.node);
      },
      DeclareTypeAlias(path) {
        recordMarker('declare', path.node);
      },
      DeclareInterface(path) {
        recordMarker('declare', path.node);
      },
      TypeCastExpression(path) {
        recordMarker('flow_type_cast', path.node);
      },
      Function(path) {
        const hasTypedParam = path.node.params.some(
          param => (param as any).typeAnnotation != null,
        );
        if (hasTypedParam) {
          recordMarker('typed_function_params', path.node);
        }
        if (path.node.returnType != null) {
          recordMarker('typed_function_return', path.node);
        }
      },
      ArrowFunctionExpression(path) {
        const hasTypedParam = path.node.params.some(
          param => (param as any).typeAnnotation != null,
        );
        if (hasTypedParam) {
          recordMarker('typed_arrow_params', path.node);
        }
      },
      VariableDeclarator(path) {
        if ((path.node.id as any).typeAnnotation != null) {
          recordMarker('typed_variable', path.node.id as any);
        }
      },
      ImportDeclaration(path) {
        if (
          path.node.importKind === 'type' ||
          path.node.importKind === 'typeof' ||
          path.node.specifiers.some(
            specifier =>
              t.isImportSpecifier(specifier) && specifier.importKind === 'type',
          )
        ) {
          recordMarker('import_type', path.node);
        }
      },
      ExportNamedDeclaration(path) {
        if (path.node.exportKind === 'type') {
          recordMarker('export_type', path.node);
        }
      },
    });
    if (firstMatch != null) {
      return firstMatch;
    }
  } catch {
    // Fall back to regex-based marker detection when parser preflight fails.
  }
  const flowTypeMarkers = [
    {pattern: /\bimport\s+type\b/, kind: 'import_type'},
    {pattern: /\bexport\s+type\b/, kind: 'export_type'},
    {pattern: /\bopaque\s+type\b/, kind: 'opaque_type'},
    {pattern: /\binterface\s+[A-Za-z_$]/, kind: 'interface'},
    {
      pattern: /\bdeclare\s+(class|function|module|var|type|interface)\b/,
      kind: 'declare',
    },
    {pattern: /\btype\s+[A-Za-z_$][\w$]*\s*=/, kind: 'type_alias'},
    {
      pattern: /\bfunction\s+[A-Za-z_$][\w$]*\s*\([^)]*:\s*[^)]*\)/,
      kind: 'typed_function_params',
    },
    {
      pattern:
        /\bfunction\s+[A-Za-z_$][\w$]*\s*\([^)]*\)\s*:\s*[A-Za-z_$][\w$<>{}\[\]|?,\s]*/,
      kind: 'typed_function_return',
    },
    {pattern: /\([^)]*:\s*[^)]*\)\s*=>/, kind: 'typed_arrow_params'},
    {
      pattern: /\b(?:const|let|var)\s+[A-Za-z_$][\w$]*\s*:\s*[^=;]+[=;]/,
      kind: 'typed_variable',
    },
    {
      pattern: /\(\s*[A-Za-z_$][\w$.]*\s*:\s*[^)]+\)/,
      kind: 'flow_type_cast',
    },
    {pattern: /\/\*::/, kind: 'flow_comment_block'},
    {pattern: /\/\/::/, kind: 'flow_comment_line'},
  ];
  let firstMatch: null | {index: number; length: number; kind: string} = null;
  for (const marker of flowTypeMarkers) {
    const match = marker.pattern.exec(sourceCode);
    if (match == null || match.index == null) {
      continue;
    }
    const candidate = {
      index: match.index,
      length: match[0].length,
      kind: marker.kind,
    };
    if (firstMatch == null || candidate.index < firstMatch.index) {
      firstMatch = candidate;
    }
  }
  return firstMatch;
}

function findObviousTypeScriptUnsupportedMarker(
  sourceCode: string,
  sourceType: 'script' | 'module',
): null | {index: number; length: number; kind: string} {
  try {
    const ast = BabelParser.parse(sourceCode, {
      sourceType,
      plugins: ['typescript', 'jsx'],
    });
    let firstMatch: null | {index: number; length: number; kind: string} = null;
    const recordMarker = (
      kind: 'instantiation_expression' | 'satisfies_expression',
      node: {start?: number | null; end?: number | null},
    ): void => {
      if (node.start == null || node.end == null) {
        return;
      }
      const candidate = {
        index: node.start,
        length: Math.max(node.end - node.start, 1),
        kind,
      };
      if (firstMatch == null || candidate.index < firstMatch.index) {
        firstMatch = candidate;
      }
    };
    traverse(ast, {
      TSInstantiationExpression(path) {
        recordMarker('instantiation_expression', path.node);
      },
      CallExpression(path) {
        if (path.node.typeParameters != null) {
          recordMarker('instantiation_expression', path.node);
        }
      },
      NewExpression(path) {
        if (path.node.typeParameters != null) {
          recordMarker('instantiation_expression', path.node);
        }
      },
      OptionalCallExpression(path) {
        if (path.node.typeParameters != null) {
          recordMarker('instantiation_expression', path.node);
        }
      },
      TSSatisfiesExpression(path) {
        recordMarker('satisfies_expression', path.node);
      },
    });
    if (firstMatch != null) {
      return firstMatch;
    }
  } catch {
    // Fall back to regex-based marker detection when parser preflight fails.
  }
  const fallbackMarkers = [
    {
      pattern:
        /=\s*[A-Za-z_$][\w$]*\s*<[^>\n]+>\s*(?=[;),.])/,
      kind: 'instantiation_expression',
    },
    {
      pattern:
        /=\s*[A-Za-z_$][\w$]*\s*<[^>\n]+>\s*\(/,
      kind: 'instantiation_expression',
    },
    {
      pattern: /\bsatisfies\b/,
      kind: 'satisfies_expression',
    },
  ];
  let firstFallbackMatch: null | {index: number; length: number; kind: string} = null;
  for (const marker of fallbackMarkers) {
    const match = marker.pattern.exec(sourceCode);
    if (match == null || match.index == null) {
      continue;
    }
    const candidate = {
      index: match.index,
      length: match[0].length,
      kind: marker.kind,
    };
    if (firstFallbackMatch == null || candidate.index < firstFallbackMatch.index) {
      firstFallbackMatch = candidate;
    }
  }
  return firstFallbackMatch;
}

function sourceOffsetToLocation(
  sourceCode: string,
  offset: number,
  length: number,
  filename: string | null,
): t.SourceLocation | null {
  if (offset < 0 || length <= 0 || offset > sourceCode.length) {
    return null;
  }
  const boundedLength = Math.min(length, sourceCode.length - offset);
  const prefix = sourceCode.slice(0, offset);
  const startLine = prefix.split('\n').length;
  const startColumn = offset - (prefix.lastIndexOf('\n') + 1);
  const marker = sourceCode.slice(offset, offset + boundedLength);
  const markerLines = marker.split('\n');
  const endLine = startLine + markerLines.length - 1;
  const endColumn =
    markerLines.length === 1
      ? startColumn + marker.length
      : markerLines[markerLines.length - 1]?.length ?? startColumn;
  return {
    filename: filename ?? '',
    identifierName: '',
    start: {
      line: startLine,
      column: startColumn,
      index: offset,
    },
    end: {
      line: endLine,
      column: endColumn,
      index: offset + boundedLength,
    },
  };
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
  const tsUnsupportedMarker =
    strictRustEngine && dialect === 'typescript'
      ? findObviousTypeScriptUnsupportedMarker(sourceCode, sourceType)
      : null;
  if (tsUnsupportedMarker != null) {
    logStrictRustFrontendFallback(
      logger,
      filename,
      `rust_frontend_preflight:typescript_${tsUnsupportedMarker.kind}`,
      sourceOffsetToLocation(
        sourceCode,
        tsUnsupportedMarker.index,
        tsUnsupportedMarker.length,
        pass.filename ?? null,
      ),
    );
    return;
  }
  const flowTypeMarker =
    dialect === 'flow'
      ? findObviousFlowTypeSyntaxMarker(sourceCode, sourceType)
      : null;
  if (dialect === 'flow' && flowTypeMarker != null) {
    if (strictRustEngine) {
      logStrictRustFrontendFallback(
        logger,
        filename,
        `rust_frontend_error:unsupported_flow_syntax:flow_syntax_not_supported:${flowTypeMarker.kind}`,
        sourceOffsetToLocation(
          sourceCode,
          flowTypeMarker.index,
          flowTypeMarker.length,
          pass.filename ?? null,
        ),
      );
    }
    return;
  }
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
