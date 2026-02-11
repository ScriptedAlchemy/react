/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type * as BabelCore from '@babel/core';
import {transformFromAstSync} from '@babel/core';
import * as BabelParser from '@babel/parser';
import invariant from 'invariant';
import BabelPluginReactCompiler from './BabelPlugin';

export function runBabelPluginReactCompiler(
  text: string,
  file: string,
  language: 'flow' | 'typescript',
  options: Record<string, unknown> | null,
  includeAst: boolean = false,
): BabelCore.BabelFileResult {
  const ast = BabelParser.parse(text, {
    sourceFilename: file,
    plugins: [language, 'jsx'],
    sourceType: 'module',
  });
  const result = transformFromAstSync(ast, text, {
    ast: includeAst,
    filename: file,
    highlightCode: false,
    retainLines: true,
    plugins: [
      [BabelPluginReactCompiler, options],
    ],
    sourceType: 'module',
    configFile: false,
    babelrc: false,
  });
  invariant(
    result?.code != null,
    `Expected BabelPluginReactForget to codegen successfully, got: ${result}`,
  );
  return result;
}
