/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type * as BabelCore from '@babel/core';
import {maybeRunRustProgramCompiler} from './RustFrontend';

/*
 * Rust-backed React Compiler Babel plugin.
 */
export default function BabelPluginReactCompiler(
  _babel: typeof BabelCore,
): BabelCore.PluginObj {
  return {
    name: 'react-compiler',
    visitor: {
      /*
       * Note: Babel does some "smart" merging of visitors across plugins, so even if A is inserted
       * prior to B, if A does not have a Program visitor and B does, B will run first. Keep the
       * Rust compiler pass as close to source as possible.
       */
      Program: {
        enter(prog, pass): void {
          maybeRunRustProgramCompiler(prog, pass);
        },
      },
    },
  };
}
