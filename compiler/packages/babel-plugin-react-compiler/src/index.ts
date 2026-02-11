/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

declare global {
  // @internal
  let __DEV__: boolean | null | undefined;
}

import BabelPluginReactCompiler from './Babel/BabelPlugin';
export default BabelPluginReactCompiler;
