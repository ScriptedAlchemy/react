/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

export const RUST_FRONTEND_INVOCATION_FAILURE_REASON =
  'rust_frontend_invocation_failure' as const;

export const RUST_FRONTEND_PARSE_OR_CANONICALIZATION_FAILURE_REASON =
  'rust_frontend_parse_or_canonicalization_failure' as const;

export const RUST_FRONTEND_PLACEHOLDER_TRANSFORMS_ENV_VAR =
  'REACT_COMPILER_RUST_PLACEHOLDER_TRANSFORMS' as const;

export function rustFrontendErrorReason(code: string, reason: string): string {
  return `rust_frontend_error:${code}:${reason}`;
}
