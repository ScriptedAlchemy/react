/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import {Worker} from 'jest-worker';
import {cpus} from 'os';
import process from 'process';
import * as readline from 'readline';
import ts from 'typescript';
import * as BabelParser from '@babel/parser';
import yargs from 'yargs';
import {hideBin} from 'yargs/helpers';
import {BABEL_PLUGIN_ROOT, FIXTURES_PATH, PROJECT_ROOT} from './constants';
import {TestFilter, TestFixture, getFixtures} from './fixture-utils';
import {TestResult, TestResults, report, update} from './reporter';
import {
  RunnerAction,
  RunnerState,
  makeWatchRunner,
  watchSrc,
} from './runner-watch';
import * as runnerWorker from './runner-worker';
import {execSync} from 'child_process';
import fs from 'fs';
import path from 'path';
import {minimize} from './minimize';
import {parseInput, parseLanguage, parseSourceType} from './compiler';
import {
  PARSE_CONFIG_PRAGMA_IMPORT,
  PRINT_HIR_IMPORT,
  PRINT_REACTIVE_IR_IMPORT,
  BABEL_PLUGIN_SRC,
} from './constants';
import chalk from 'chalk';

const WORKER_PATH = require.resolve('./runner-worker.js');
const NUM_WORKERS = cpus().length - 1;

readline.emitKeypressEvents(process.stdin);

type TestOptions = {
  sync: boolean;
  workerThreads: boolean;
  watch: boolean;
  update: boolean;
  pattern?: string;
  debug: boolean;
  verbose: boolean;
};

type MinimizeOptions = {
  path: string;
  update: boolean;
};

type CompileOptions = {
  path: string;
  debug: boolean;
};

type ParityOptions = {
  pattern?: string;
  verbose: boolean;
  output?: string;
  evaluator: boolean;
  failOnMismatch: boolean;
  maxMismatches: number;
  includeOutput: boolean;
  ignoreFormatting: boolean;
  ignoreLogs: boolean;
  skipBuild: boolean;
};

async function runTestCommand(opts: TestOptions): Promise<void> {
  const worker: Worker & typeof runnerWorker = new Worker(WORKER_PATH, {
    enableWorkerThreads: opts.workerThreads,
    numWorkers: NUM_WORKERS,
  }) as any;
  worker.getStderr().pipe(process.stderr);
  worker.getStdout().pipe(process.stdout);

  // Check if watch mode should be enabled
  const shouldWatch = opts.watch;

  if (shouldWatch) {
    makeWatchRunner(
      state => onChange(worker, state, opts.sync, opts.verbose),
      opts.debug,
      opts.pattern,
    );
    if (opts.pattern) {
      /**
       * Warm up workers when in watch mode. Loading the compiler plugin and
       * all of its transitive dependencies takes 1-3s (per worker) on an M1.
       * As jest-worker dispatches tasks using a round-robin strategy, we can
       * avoid an additional 1-3s wait on the first num_workers runs by warming
       * up workers eagerly.
       */
      for (let i = 0; i < NUM_WORKERS - 1; i++) {
        worker.transformFixture(
          {
            fixturePath: 'tmp',
            snapshotPath: './tmp.expect.md',
            inputPath: './tmp.js',
            input: `
            function Foo(props) {
              return identity(props);
            }
            `,
            snapshot: null,
          },
          0,
          false,
          false,
        );
      }
    }
  } else {
    // Non-watch mode. For simplicity we re-use the same watchSrc() function.
    // After the first build completes run tests and exit
    const tsWatch: ts.WatchOfConfigFile<ts.SemanticDiagnosticsBuilderProgram> =
      watchSrc(
        () => {},
        async (isTypecheckSuccess: boolean) => {
          let isSuccess = false;
          if (!isTypecheckSuccess) {
            console.error(
              'Found typescript errors in compiler source code, skipping test fixtures.',
            );
          } else {
            try {
              execSync('yarn build', {cwd: BABEL_PLUGIN_ROOT});
              console.log('Built compiler successfully with tsup');

              // Determine which filter to use
              let testFilter: TestFilter | null = null;
              if (opts.pattern) {
                testFilter = {
                  paths: [opts.pattern],
                };
              }

              const results = await runFixtures(
                worker,
                testFilter,
                0,
                opts.debug,
                false, // no requireSingleFixture in non-watch mode
                opts.sync,
              );
              if (opts.update) {
                update(results);
                isSuccess = true;
              } else {
                isSuccess = report(results, opts.verbose);
              }
            } catch (e) {
              console.warn('Failed to build compiler with tsup:', e);
            }
          }
          tsWatch?.close();
          await worker.end();
          process.exit(isSuccess ? 0 : 1);
        },
      );
  }
}

async function runMinimizeCommand(opts: MinimizeOptions): Promise<void> {
  // Resolve the input path
  const inputPath = path.isAbsolute(opts.path)
    ? opts.path
    : path.resolve(PROJECT_ROOT, opts.path);

  // Check if file exists
  if (!fs.existsSync(inputPath)) {
    console.error(`Error: File not found: ${inputPath}`);
    process.exit(1);
  }

  // Read the input file
  const input = fs.readFileSync(inputPath, 'utf-8');
  const filename = path.basename(inputPath);
  const firstLine = input.substring(0, input.indexOf('\n'));
  const language = parseLanguage(firstLine);
  const sourceType = parseSourceType(firstLine);

  console.log(`Minimizing: ${inputPath}`);

  const originalLines = input.split('\n').length;

  // Run the minimization
  const result = minimize(input, filename, language, sourceType);

  if (result.kind === 'success') {
    console.log('Could not minimize: the input compiles successfully.');
    process.exit(0);
  }

  if (result.kind === 'minimal') {
    console.log(
      'Could not minimize: the input fails but is already minimal and cannot be reduced further.',
    );
    process.exit(0);
  }

  // Output the minimized code
  console.log('--- Minimized Code ---');
  console.log(result.source);

  const minimizedLines = result.source.split('\n').length;
  console.log(
    `\nReduced from ${originalLines} lines to ${minimizedLines} lines`,
  );

  if (opts.update) {
    fs.writeFileSync(inputPath, result.source, 'utf-8');
    console.log(`\nUpdated ${inputPath} with minimized code.`);
  }
}

async function runCompileCommand(opts: CompileOptions): Promise<void> {
  // Resolve the input path
  const inputPath = path.isAbsolute(opts.path)
    ? opts.path
    : path.resolve(PROJECT_ROOT, opts.path);

  // Check if file exists
  if (!fs.existsSync(inputPath)) {
    console.error(`Error: File not found: ${inputPath}`);
    process.exit(1);
  }

  // Read the input file
  const input = fs.readFileSync(inputPath, 'utf-8');
  const filename = path.basename(inputPath);
  const firstLine = input.substring(0, input.indexOf('\n'));
  const language = parseLanguage(firstLine);
  const sourceType = parseSourceType(firstLine);

  // Import the compiler
  const importedCompilerPlugin = require(BABEL_PLUGIN_SRC) as Record<
    string,
    any
  >;
  const BabelPluginReactCompiler = importedCompilerPlugin['default'];
  const parseConfigPragmaForTests =
    importedCompilerPlugin[PARSE_CONFIG_PRAGMA_IMPORT];
  const printFunctionWithOutlined = importedCompilerPlugin[PRINT_HIR_IMPORT];
  const printReactiveFunctionWithOutlined =
    importedCompilerPlugin[PRINT_REACTIVE_IR_IMPORT];
  const EffectEnum = importedCompilerPlugin['Effect'];
  const ValueKindEnum = importedCompilerPlugin['ValueKind'];
  const ValueReasonEnum = importedCompilerPlugin['ValueReason'];

  // Setup debug logger
  let lastLogged: string | null = null;
  const debugIRLogger = opts.debug
    ? (value: any) => {
        let printed: string;
        switch (value.kind) {
          case 'hir':
            printed = printFunctionWithOutlined(value.value);
            break;
          case 'reactive':
            printed = printReactiveFunctionWithOutlined(value.value);
            break;
          case 'debug':
            printed = value.value;
            break;
          case 'ast':
            printed = '(ast)';
            break;
          default:
            printed = String(value);
        }

        if (printed !== lastLogged) {
          lastLogged = printed;
          console.log(`${chalk.green(value.name)}:\n${printed}\n`);
        } else {
          console.log(`${chalk.blue(value.name)}: (no change)\n`);
        }
      }
    : () => {};

  // Parse the input
  let ast;
  try {
    ast = parseInput(input, filename, language, sourceType);
  } catch (e: any) {
    console.error(`Parse error: ${e.message}`);
    process.exit(1);
  }

  // Build plugin options
  const config = parseConfigPragmaForTests(firstLine, {compilationMode: 'all'});
  const options = {
    ...config,
    environment: {
      ...config.environment,
    },
    logger: {
      logEvent: () => {},
      debugLogIRs: debugIRLogger,
    },
    enableReanimatedCheck: false,
  };

  // Compile
  const {transformFromAstSync} = require('@babel/core');
  try {
    const result = transformFromAstSync(ast, input, {
      filename: '/' + filename,
      highlightCode: false,
      retainLines: true,
      compact: true,
      plugins: [[BabelPluginReactCompiler, options]],
      sourceType: 'module',
      ast: false,
      cloneInputAst: true,
      configFile: false,
      babelrc: false,
    });

    if (result?.code != null) {
      // Format the output
      const prettier = require('prettier');
      const formatted = await prettier.format(result.code, {
        semi: true,
        parser: language === 'typescript' ? 'babel-ts' : 'flow',
      });
      console.log(formatted);
    } else {
      console.error('Error: No code emitted from compiler');
      process.exit(1);
    }
  } catch (e: any) {
    console.error(e.message);
    process.exit(1);
  }
}

type ParityMismatchKind = 'output_mismatch' | 'unexpected_error_mismatch';

type ParitySectionDiff = {
  firstRun: string | null;
  secondRun: string | null;
  normalizedFirstRun: string;
  normalizedSecondRun: string;
  // Legacy aliases retained for report compatibility.
  babel: string | null;
  rust: string | null;
  normalizedBabel: string;
  normalizedRust: string;
};

type ParityMismatch = {
  fixture: string;
  kind: ParityMismatchKind;
  hasOutputMismatch: boolean;
  hasRawOutputMismatch: boolean;
  hasNormalizedOutputMismatch: boolean;
  hasUnexpectedErrorMismatch: boolean;
  hasCodeSectionMismatch: boolean;
  hasEvalSectionMismatch: boolean;
  hasLogsSectionMismatch: boolean;
  hasErrorSectionMismatch: boolean;
  firstRunUnexpectedError: string | null;
  secondRunUnexpectedError: string | null;
  // Legacy aliases retained for report compatibility.
  babelUnexpectedError: string | null;
  rustUnexpectedError: string | null;
  outputPath: string;
  firstRunActual?: string | null;
  secondRunActual?: string | null;
  // Legacy aliases retained for report compatibility.
  babelActual?: string | null;
  rustActual?: string | null;
  codeSectionDiff?: ParitySectionDiff | null;
  evalSectionDiff?: ParitySectionDiff | null;
  logsSectionDiff?: ParitySectionDiff | null;
  errorSectionDiff?: ParitySectionDiff | null;
};

type SnapshotSections = {
  code: string | null;
  evalOutput: string | null;
  logs: string | null;
  error: string | null;
};

function normalizeCodeSection(value: string | null): string {
  return canonicalizeCodeForParity(value ?? '');
}

function createSectionDiff(
  firstRunValue: string | null,
  secondRunValue: string | null,
  normalize: (value: string | null) => string,
): ParitySectionDiff {
  const normalizedFirstRun = normalize(firstRunValue);
  const normalizedSecondRun = normalize(secondRunValue);
  return {
    firstRun: firstRunValue,
    secondRun: secondRunValue,
    normalizedFirstRun,
    normalizedSecondRun,
    babel: firstRunValue,
    rust: secondRunValue,
    normalizedBabel: normalizedFirstRun,
    normalizedRust: normalizedSecondRun,
  };
}

function canonicalizeCodeForParity(code: string): string {
  const parser = require('@babel/parser') as typeof BabelParser;
  const generator = require('@babel/generator').default as typeof import('@babel/generator').default;
  const pluginCandidates: Array<Array<BabelParser.ParserPlugin>> = [
    ['typescript', 'jsx'],
    ['flow', 'jsx'],
    ['jsx'],
  ];
  for (const plugins of pluginCandidates) {
    try {
      const ast = parser.parse(code, {
        sourceType: 'module',
        plugins,
      });
      return (
        generator(ast, {
          comments: false,
          compact: true,
          minified: true,
          retainLines: false,
        }).code ?? ''
      );
    } catch {
      // try next parser plugin set
    }
  }
  return code.replace(/\s+/g, ' ').trim();
}

function canonicalizeSnapshotForParity(snapshot: string | null): string | null {
  if (snapshot == null) {
    return null;
  }
  const codeBlockRegex = /(## Code\s+```javascript\n)([\s\S]*?)(\n```)/m;
  const match = snapshot.match(codeBlockRegex);
  if (match == null) {
    return snapshot.replace(/[ \t]+\n/g, '\n').trim();
  }
  const [, prefix, code, suffix] = match;
  const canonicalCode = canonicalizeCodeForParity(code);
  return snapshot
    .replace(codeBlockRegex, `${prefix}${canonicalCode}${suffix}`)
    .replace(/[ \t]+\n/g, '\n')
    .trim();
}

function stripLogsFromSnapshot(snapshot: string | null): string | null {
  if (snapshot == null) {
    return null;
  }
  const logsSectionRegex = /\n## Logs\n\n```[\s\S]*?```\n?/m;
  return snapshot.replace(logsSectionRegex, '\n');
}

function extractSnapshotSections(snapshot: string | null): SnapshotSections {
  if (snapshot == null) {
    return {
      code: null,
      evalOutput: null,
      logs: null,
      error: null,
    };
  }

  const codeMatch = snapshot.match(/## Code\s+```javascript\n([\s\S]*?)\n```/m);
  const logsMatch = snapshot.match(/## Logs\s+```\n([\s\S]*?)\n```/m);
  const errorMatch = snapshot.match(/## Error\s+```\n([\s\S]*?)\n```/m);

  const evalSeparator = '\n### Eval output\n';
  const evalIndex = snapshot.indexOf(evalSeparator);
  const evalOutput =
    evalIndex === -1 ? null : snapshot.slice(evalIndex + evalSeparator.length).trim();

  return {
    code: codeMatch?.[1] ?? null,
    evalOutput,
    logs: logsMatch?.[1] ?? null,
    error: errorMatch?.[1] ?? null,
  };
}

function normalizeSectionText(value: string | null): string {
  if (value == null) {
    return '';
  }
  return value.replace(/\s+/g, ' ').trim();
}

async function transformFixtureWithEnv(
  fixture: TestFixture,
  compilerVersion: number,
  includeEvaluator: boolean,
): Promise<TestResult> {
  return await runnerWorker.transformFixture(
    fixture,
    compilerVersion,
    false,
    includeEvaluator,
  );
}

async function runParityCommand(opts: ParityOptions): Promise<void> {
  if (!opts.skipBuild) {
    execSync('yarn build', {cwd: BABEL_PLUGIN_ROOT, stdio: 'inherit'});
  }

  let testFilter: TestFilter | null = null;
  if (opts.pattern) {
    testFilter = {
      paths: [opts.pattern],
    };
  }

  const fixtures = await getFixtures(testFilter);
  if (fixtures.size === 0) {
    console.log(
      chalk.yellow(
        `No fixtures found under ${FIXTURES_PATH}${
          opts.pattern != null ? ` for pattern "${opts.pattern}"` : ''
        }.`,
      ),
    );
  }
  const mismatches: Array<ParityMismatch> = [];
  let comparedFixtures = 0;
  let reachedMismatchLimit = false;

  for (const [fixtureName, fixture] of fixtures) {
    comparedFixtures += 1;
    const firstRunResult = await transformFixtureWithEnv(
      fixture,
      1,
      opts.evaluator,
    );
    const secondRunResult = await transformFixtureWithEnv(
      fixture,
      2,
      opts.evaluator,
    );

    const hasUnexpectedErrorMismatch =
      firstRunResult.unexpectedError !== secondRunResult.unexpectedError;
    const firstRunComparableActual = opts.ignoreLogs
      ? stripLogsFromSnapshot(firstRunResult.actual)
      : firstRunResult.actual;
    const secondRunComparableActual = opts.ignoreLogs
      ? stripLogsFromSnapshot(secondRunResult.actual)
      : secondRunResult.actual;
    const hasRawOutputMismatch =
      firstRunResult.actual !== secondRunResult.actual;
    const hasNormalizedOutputMismatch =
      canonicalizeSnapshotForParity(firstRunComparableActual) !==
      canonicalizeSnapshotForParity(secondRunComparableActual);
    const firstRunSections = extractSnapshotSections(firstRunResult.actual);
    const secondRunSections = extractSnapshotSections(secondRunResult.actual);
    const codeSectionDiff = createSectionDiff(
      firstRunSections.code,
      secondRunSections.code,
      normalizeCodeSection,
    );
    const evalSectionDiff = createSectionDiff(
      firstRunSections.evalOutput,
      secondRunSections.evalOutput,
      normalizeSectionText,
    );
    const logsSectionDiff = createSectionDiff(
      firstRunSections.logs,
      secondRunSections.logs,
      normalizeSectionText,
    );
    const errorSectionDiff = createSectionDiff(
      firstRunSections.error,
      secondRunSections.error,
      normalizeSectionText,
    );
    const hasCodeSectionMismatch =
      codeSectionDiff.normalizedFirstRun !== codeSectionDiff.normalizedSecondRun;
    const hasEvalSectionMismatch =
      evalSectionDiff.normalizedFirstRun !== evalSectionDiff.normalizedSecondRun;
    const hasLogsSectionMismatch =
      logsSectionDiff.normalizedFirstRun !== logsSectionDiff.normalizedSecondRun;
    const hasErrorSectionMismatch =
      errorSectionDiff.normalizedFirstRun !== errorSectionDiff.normalizedSecondRun;
    const hasOutputMismatch = opts.ignoreFormatting
      ? hasNormalizedOutputMismatch
      : hasRawOutputMismatch;
    if (!hasUnexpectedErrorMismatch && !hasOutputMismatch) {
      continue;
    }

    const mismatch: ParityMismatch = {
      fixture: fixtureName,
      kind: hasUnexpectedErrorMismatch
        ? 'unexpected_error_mismatch'
        : 'output_mismatch',
      hasOutputMismatch,
      hasRawOutputMismatch,
      hasNormalizedOutputMismatch,
      hasUnexpectedErrorMismatch,
      hasCodeSectionMismatch,
      hasEvalSectionMismatch,
      hasLogsSectionMismatch,
      hasErrorSectionMismatch,
      firstRunUnexpectedError: firstRunResult.unexpectedError,
      secondRunUnexpectedError: secondRunResult.unexpectedError,
      babelUnexpectedError: firstRunResult.unexpectedError,
      rustUnexpectedError: secondRunResult.unexpectedError,
      outputPath: secondRunResult.outputPath,
    };
    if (opts.includeOutput) {
      mismatch.firstRunActual = firstRunResult.actual;
      mismatch.secondRunActual = secondRunResult.actual;
      mismatch.babelActual = firstRunResult.actual;
      mismatch.rustActual = secondRunResult.actual;
    }
    mismatch.codeSectionDiff = hasCodeSectionMismatch ? codeSectionDiff : null;
    mismatch.evalSectionDiff = hasEvalSectionMismatch ? evalSectionDiff : null;
    mismatch.logsSectionDiff = hasLogsSectionMismatch ? logsSectionDiff : null;
    mismatch.errorSectionDiff = hasErrorSectionMismatch ? errorSectionDiff : null;
    mismatches.push(mismatch);

    if (opts.verbose) {
      const message = `${mismatch.fixture}: ${mismatch.kind}`;
      console.log(chalk.yellow(message));
      if (hasUnexpectedErrorMismatch) {
        console.log(
          chalk.red(
            `  first run error: ${mismatch.firstRunUnexpectedError ?? '<none>'}`,
          ),
        );
        console.log(
          chalk.red(
            `  second run error: ${mismatch.secondRunUnexpectedError ?? '<none>'}`,
          ),
        );
      }
    }

    if (opts.maxMismatches > 0 && mismatches.length >= opts.maxMismatches) {
      reachedMismatchLimit = true;
      break;
    }
  }

  const mismatchSummary = {
    unexpectedErrorMismatchCount: mismatches.filter(
      mismatch => mismatch.hasUnexpectedErrorMismatch,
    ).length,
    codeSectionMismatchCount: mismatches.filter(mismatch => mismatch.hasCodeSectionMismatch)
      .length,
    evalSectionMismatchCount: mismatches.filter(mismatch => mismatch.hasEvalSectionMismatch)
      .length,
    logsSectionMismatchCount: mismatches.filter(mismatch => mismatch.hasLogsSectionMismatch)
      .length,
    errorSectionMismatchCount: mismatches.filter(mismatch => mismatch.hasErrorSectionMismatch)
      .length,
  };

  const output = {
    generatedAt: new Date().toISOString(),
    fixtureCount: fixtures.size,
    comparedFixtureCount: comparedFixtures,
    mismatchCount: mismatches.length,
    reachedMismatchLimit,
    maxMismatches: opts.maxMismatches,
    evaluatorEnabled: opts.evaluator,
    includeOutput: opts.includeOutput,
    ignoreFormatting: opts.ignoreFormatting,
    ignoreLogs: opts.ignoreLogs,
    pattern: opts.pattern ?? null,
    mismatchSummary,
    mismatches,
  };

  if (opts.output != null) {
    const outputPath = path.isAbsolute(opts.output)
      ? opts.output
      : path.resolve(PROJECT_ROOT, opts.output);
    fs.mkdirSync(path.dirname(outputPath), {recursive: true});
    fs.writeFileSync(outputPath, JSON.stringify(output, null, 2) + '\n', 'utf8');
    console.log(`Wrote parity report to ${outputPath}`);
  }

  const paritySummary =
    mismatches.length === 0
      ? `Parity success: ${fixtures.size} fixtures matched.`
      : `Parity mismatches: ${mismatches.length}/${comparedFixtures} compared fixtures differ.`;
  console.log(paritySummary);
  process.exit(mismatches.length === 0 || !opts.failOnMismatch ? 0 : 1);
}

const cli = yargs(hideBin(process.argv)) as any;

cli
  .command(
    ['test', '$0'],
    'Run compiler tests',
    (yargs: any) => {
      return yargs
        .boolean('sync')
        .describe(
          'sync',
          'Run compiler in main thread (instead of using worker threads or subprocesses). Defaults to false.',
        )
        .default('sync', false)
        .boolean('worker-threads')
        .describe(
          'worker-threads',
          'Run compiler in worker threads (instead of subprocesses). Defaults to true.',
        )
        .default('worker-threads', true)
        .boolean('watch')
        .describe(
          'watch',
          'Run compiler in watch mode, re-running after changes',
        )
        .alias('w', 'watch')
        .default('watch', false)
        .boolean('update')
        .alias('u', 'update')
        .describe('update', 'Update fixtures')
        .default('update', false)
        .string('pattern')
        .alias('p', 'pattern')
        .describe(
          'pattern',
          'Optional glob pattern to filter fixtures (e.g., "error.*", "use-memo")',
        )
        .boolean('debug')
        .alias('d', 'debug')
        .describe('debug', 'Enable debug logging to print HIR for each pass')
        .default('debug', false)
        .boolean('verbose')
        .alias('v', 'verbose')
        .describe('verbose', 'Print individual test results')
        .default('verbose', false);
    },
    async (argv: any) => {
      await runTestCommand(argv as TestOptions);
    },
  )
  .command(
    'minimize <path>',
    'Minimize a test case to reproduce a compiler error',
    (yargs: any) => {
      return yargs
        .positional('path', {
          describe: 'Path to the file to minimize',
          type: 'string',
          demandOption: true,
        })
        .boolean('update')
        .alias('u', 'update')
        .describe(
          'update',
          'Update the input file in-place with the minimized version',
        )
        .default('update', false);
    },
    async (argv: any) => {
      await runMinimizeCommand(argv as unknown as MinimizeOptions);
    },
  )
  .command(
    'compile <path>',
    'Compile a file with the React Compiler',
    (yargs: any) => {
      return yargs
        .positional('path', {
          describe: 'Path to the file to compile',
          type: 'string',
          demandOption: true,
        })
        .boolean('debug')
        .alias('d', 'debug')
        .describe('debug', 'Enable debug logging to print HIR for each pass')
        .default('debug', false);
    },
    async (argv: any) => {
      await runCompileCommand(argv as unknown as CompileOptions);
    },
  )
  .command(
    'parity',
    'Compare repeated Rust fixture outputs for consistency',
    (yargs: any) => {
      return yargs
        .string('pattern')
        .alias('p', 'pattern')
        .describe(
          'pattern',
          'Optional glob pattern to filter fixtures (e.g., "while-*")',
        )
        .boolean('verbose')
        .alias('v', 'verbose')
        .describe('verbose', 'Print each mismatching fixture')
        .default('verbose', false)
        .string('output')
        .alias('o', 'output')
        .describe(
          'output',
          'Optional path to write machine-readable JSON parity report',
        )
        .boolean('evaluator')
        .describe(
          'evaluator',
          'Include evaluator output when comparing parity (default true)',
        )
        .default('evaluator', true)
        .boolean('fail-on-mismatch')
        .describe(
          'fail-on-mismatch',
          'Exit with non-zero status when mismatches are found (default true)',
        )
        .default('fail-on-mismatch', true)
        .number('max-mismatches')
        .describe(
          'max-mismatches',
          'Stop after recording this many mismatches (0 = no limit)',
        )
        .default('max-mismatches', 0)
        .boolean('include-output')
        .describe(
          'include-output',
          'Include full comparison output strings in JSON report (default false)',
        )
        .default('include-output', false)
        .boolean('ignore-formatting')
        .describe(
          'ignore-formatting',
          'Compare normalized compiler output to ignore formatting-only differences (default true)',
        )
        .default('ignore-formatting', true)
        .boolean('ignore-logs')
        .describe(
          'ignore-logs',
          'Ignore logger output sections when comparing parity (default true)',
        )
        .default('ignore-logs', true)
        .boolean('skip-build')
        .describe(
          'skip-build',
          'Skip rebuilding babel-plugin-react-compiler before parity run (default false)',
        )
        .default('skip-build', false);
    },
    async (argv: any) => {
      await runParityCommand(argv as unknown as ParityOptions);
    },
  )
  .help('help')
  .strict()
  .demandCommand()
  .parse();

/**
 * Do a test run and return the test results
 */
async function runFixtures(
  worker: Worker & typeof runnerWorker,
  filter: TestFilter | null,
  compilerVersion: number,
  debug: boolean,
  requireSingleFixture: boolean,
  sync: boolean,
): Promise<TestResults> {
  // We could in theory be fancy about tracking the contents of the fixtures
  // directory via our file subscription, but it's simpler to just re-read
  // the directory each time.
  const fixtures = await getFixtures(filter);
  const isOnlyFixture = filter !== null && fixtures.size === 1;
  const shouldLog = debug && (!requireSingleFixture || isOnlyFixture);

  let entries: Array<[string, TestResult]>;
  if (!sync) {
    // Note: promise.all to ensure parallelism when enabled
    const work: Array<Promise<[string, TestResult]>> = [];
    for (const [fixtureName, fixture] of fixtures) {
      work.push(
        worker
          .transformFixture(fixture, compilerVersion, shouldLog, true)
          .then(result => [fixtureName, result]),
      );
    }

    entries = await Promise.all(work);
  } else {
    entries = [];
    for (const [fixtureName, fixture] of fixtures) {
      let output = await runnerWorker.transformFixture(
        fixture,
        compilerVersion,
        shouldLog,
        true,
      );
      entries.push([fixtureName, output]);
    }
  }

  return new Map(entries);
}

// Callback to re-run tests after some change
async function onChange(
  worker: Worker & typeof runnerWorker,
  state: RunnerState,
  sync: boolean,
  verbose: boolean,
) {
  const {compilerVersion, isCompilerBuildValid, mode, filter, debug} = state;
  if (isCompilerBuildValid) {
    const start = performance.now();

    // console.clear() only works when stdout is connected to a TTY device.
    // we're currently piping stdout (see main.ts), so let's do a 'hack'
    console.log('\u001Bc');

    // we don't clear console after this point, since
    // it may contain debug console logging
    const results = await runFixtures(
      worker,
      mode.filter ? filter : null,
      compilerVersion,
      debug,
      true, // requireSingleFixture in watch mode
      sync,
    );
    const end = performance.now();

    // Track fixture status for autocomplete suggestions
    for (const [basename, result] of results) {
      const failed =
        result.actual !== result.expected || result.unexpectedError != null;
      state.fixtureLastRunStatus.set(basename, failed ? 'fail' : 'pass');
    }

    if (mode.action === RunnerAction.Update) {
      update(results);
      state.lastUpdate = end;
    } else {
      report(results, verbose);
    }
    console.log(`Completed in ${Math.floor(end - start)} ms`);
  } else {
    console.error(
      `${mode}: Found errors in compiler source code, skipping test fixtures.`,
    );
  }
  console.log(
    '\n' +
      (mode.filter
        ? `Current mode = FILTER, pattern = "${filter?.paths[0] ?? ''}".`
        : 'Current mode = NORMAL, run all test fixtures.') +
      '\nWaiting for input or file changes...\n' +
      'u     - update all fixtures\n' +
      `d     - toggle (turn ${debug ? 'off' : 'on'}) debug logging\n` +
      'p     - enter pattern to filter fixtures\n' +
      (mode.filter ? 'a     - run all tests (exit filter mode)\n' : '') +
      'q     - quit\n' +
      '[any] - rerun tests\n',
  );
}
