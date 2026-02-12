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

export {
  CompilerSuggestionOperation,
  ErrorCategory,
  ErrorSeverity,
  Effect,
  LintRulePreset,
  LintRules,
  ValueKind,
  ValueReason,
  parsePluginOptions,
  parseConfigPragmaForTests,
  printFunctionWithOutlined,
  printReactiveFunctionWithOutlined,
  printReactiveFunction,
  getRuleForCategory,
  validateEnvironmentConfig,
} from './Compat/LegacyApi';
export type {
  CompileErrorEvent,
  CompileSuccessEvent,
  CompilerDiagnosticOptions,
  CompilerErrorDetailOptions,
  CompilerPipelineValue,
  CompilerReactTarget,
  LintRule,
  Logger,
  LoggerEvent,
  PipelineErrorEvent,
  PluginOptions,
  SourceLocation,
  TypeConfig,
} from './Compat/LegacyApi';
