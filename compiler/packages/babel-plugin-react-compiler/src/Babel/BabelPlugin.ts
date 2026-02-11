/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type * as BabelCore from '@babel/core';
import {
  maybeRunRustProgramCompiler,
  type RustFrontendLogger,
} from './RustFrontend';

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
            const logger: RustFrontendLogger | null =
              'logger' in pass.opts && pass.opts.logger != null
                ? (pass.opts.logger as RustFrontendLogger)
                : null;
            maybeRunRustProgramCompiler(
              prog,
              pass,
              logger,
              pass.filename ?? null,
            );
          } catch (e) {
            throw e;
          }
        },
      },
    },
  };
}
