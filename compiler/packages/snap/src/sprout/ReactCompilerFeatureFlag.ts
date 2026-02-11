/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

export function isCompilerEnabled_Fixtures(): boolean {
  return true;
}

// Legacy alias retained for existing compiled fixture references.
export function isForgetEnabled_Fixtures(): boolean {
  return isCompilerEnabled_Fixtures();
}
