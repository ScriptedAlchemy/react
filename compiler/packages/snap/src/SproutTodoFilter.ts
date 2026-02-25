/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// Legacy JS fixture allowlist removed with the fixture corpus.
// Keep this as a dedicated extension point for future sprout skips.
const skipFilter = new Set<string>();

export default skipFilter;
