/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import {EvaluatorResult, doEval} from './evaluator';

export type SproutResult =
  | {kind: 'success'; value: string}
  | {kind: 'invalid'; value: string};

function stringify(result: EvaluatorResult): string {
  return `(kind: ${result.kind}) ${result.value}${
    result.logs.length > 0 ? `\nlogs: [${result.logs.toString()}]` : ''
  }`;
}
function makeError(description: string, value: string): SproutResult {
  return {
    kind: 'invalid',
    value: description + '\n' + value,
  };
}
function logsEqual(a: Array<string>, b: Array<string>) {
  if (a.length !== b.length) {
    return false;
  }
  return a.every((val, idx) => val === b[idx]);
}
export function runSprout(
  originalCode: string,
  compiledCode: string,
): SproutResult {
  let compiledResult;
  try {
    (globalThis as any).__SNAP_EVALUATOR_MODE = 'compiled';
    compiledResult = doEval(compiledCode);
  } catch (e) {
    throw e;
  } finally {
    (globalThis as any).__SNAP_EVALUATOR_MODE = undefined;
  }
  if (compiledResult.kind === 'UnexpectedError') {
    return makeError('Unexpected error in compiler runner', compiledResult.value);
  }
  if (originalCode.indexOf('@disableNonForgetInSprout') === -1) {
    const originalResult = doEval(originalCode);

    if (originalResult.kind === 'UnexpectedError') {
      return makeError(
        'Unexpected error in uncompiled runner',
        originalResult.value,
      );
    } else if (
      compiledResult.kind !== originalResult.kind ||
      compiledResult.value !== originalResult.value ||
      !logsEqual(compiledResult.logs, originalResult.logs)
    ) {
      return makeError(
        'Found differences in evaluator results',
        `Uncompiled (expected):
${stringify(originalResult)}
Compiled:
${stringify(compiledResult)}
`,
      );
    }
  }
  return {
    kind: 'success',
    value: stringify(compiledResult),
  };
}
