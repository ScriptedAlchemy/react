/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type * as BabelCore from '@babel/core';
import {Logger} from '../Entrypoint';
import {CompilerError} from '..';
import {
  maybeRunRustProgramCompiler,
} from './RustFrontend';

const ENABLE_REACT_COMPILER_TIMINGS =
  process.env['ENABLE_REACT_COMPILER_TIMINGS'] === '1';

function markCompilationEnd(filename: string): void {
  if (ENABLE_REACT_COMPILER_TIMINGS === true) {
    performance.mark(`${filename}:end`, {
      detail: 'BabelPlugin:Program:end',
    });
  }
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
            const logger: Logger | null =
              'logger' in pass.opts && pass.opts.logger != null
                ? (pass.opts.logger as Logger)
                : null;
            maybeRunRustProgramCompiler(
              prog,
              pass,
              logger,
              pass.filename ?? null,
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
