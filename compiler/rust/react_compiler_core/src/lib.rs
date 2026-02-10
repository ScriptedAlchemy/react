use std::collections::{HashMap, HashSet};
use swc_common::{
    comments::SingleThreadedComments, sync::Lrc, FileName, SourceMap, Span, Spanned, DUMMY_SP,
};
use swc_ecma_ast::{
    AssignTarget, BindingIdent, BlockStmt, BlockStmtOrExpr, CallExpr, Callee, Decl, DefaultDecl,
    EsVersion, Expr, ExprOrSpread, Ident, ImportDecl, ImportNamedSpecifier, ImportSpecifier, Lit,
    MemberExpr, MemberProp, Module, ModuleDecl, ModuleExportName, ModuleItem, Number, Pat, Prop,
    PropName, PropOrSpread, Script, SimpleAssignTarget, Stmt, VarDecl, VarDeclKind, VarDeclarator,
};
use swc_ecma_codegen::{text_writer::JsWriter, Config as CodegenConfig, Emitter};
use swc_ecma_parser::{
    error::SyntaxError as ParserSyntaxError, lexer::Lexer, EsSyntax, Parser, StringInput, Syntax,
    TsSyntax,
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDialect {
    JavaScript,
    TypeScript,
    Flow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerOptions {
    pub dialect: InputDialect,
    pub is_module: bool,
    pub filename: String,
    pub apply_placeholder_transforms: bool,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            dialect: InputDialect::JavaScript,
            is_module: true,
            filename: "unknown.js".to_string(),
            apply_placeholder_transforms: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseMetadata {
    pub statement_count: usize,
    pub statement_count_after_transform: usize,
    pub placeholder_runtime_helper_import_count_before_transform: usize,
    pub placeholder_runtime_helper_import_count_after_transform: usize,
    pub placeholder_runtime_helper_import_added: bool,
    pub placeholder_runtime_callee_reused: bool,
    pub placeholder_runtime_callee_generated: bool,
    pub placeholder_transform_status: String,
    pub placeholder_transform_candidates: Vec<String>,
    pub placeholder_transform_skipped_functions: Vec<String>,
    pub placeholder_transform_candidate_count: usize,
    pub placeholder_transform_skipped_count: usize,
    pub placeholder_transform_candidate_component_count: usize,
    pub placeholder_transform_candidate_hook_count: usize,
    pub placeholder_transform_transformed_component_count: usize,
    pub placeholder_transform_transformed_hook_count: usize,
    pub placeholder_transform_skipped_component_count: usize,
    pub placeholder_transform_skipped_hook_count: usize,
    pub detected_component_function_count: usize,
    pub detected_hook_function_count: usize,
    pub detected_react_functions: usize,
    pub react_functions: Vec<ReactFunction>,
    pub placeholder_transforms_applied: usize,
    pub placeholder_transformed_functions: Vec<String>,
    pub placeholder_runtime_callee_name_before_transform: Option<String>,
    pub placeholder_runtime_callee_candidates_before_transform: Vec<String>,
    pub placeholder_runtime_callee_candidate_count_before_transform: usize,
    pub placeholder_runtime_namespace_candidates_before_transform: Vec<String>,
    pub placeholder_runtime_namespace_candidate_count_before_transform: usize,
    pub placeholder_runtime_callee_name: Option<String>,
    pub placeholder_runtime_callee_candidates: Vec<String>,
    pub placeholder_runtime_callee_candidate_count: usize,
    pub placeholder_runtime_namespace_candidates: Vec<String>,
    pub placeholder_runtime_namespace_candidate_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactFunctionKind {
    Component,
    Hook,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactFunction {
    pub name: String,
    pub kind: ReactFunctionKind,
    pub loc: Option<SourceLocation>,
}

const DEFAULT_EXPORT_COMPONENT_NAME: &str = "__default_export_component__";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileOutput {
    pub code: String,
    pub metadata: ParseMetadata,
}

pub fn render_react_functions_debug(metadata: &ParseMetadata) -> String {
    let mut lines = vec![
        "ReactiveFunctionsDebug v0".to_string(),
        format!("statement_count={}", metadata.statement_count),
        format!(
            "statement_count_after_transform={}",
            metadata.statement_count_after_transform
        ),
        format!(
            "placeholder_runtime_helper_import_count_before_transform={}",
            metadata.placeholder_runtime_helper_import_count_before_transform
        ),
        format!(
            "placeholder_runtime_helper_import_count_after_transform={}",
            metadata.placeholder_runtime_helper_import_count_after_transform
        ),
        format!(
            "placeholder_runtime_helper_import_added={}",
            metadata.placeholder_runtime_helper_import_added
        ),
        format!(
            "placeholder_runtime_callee_reused={}",
            metadata.placeholder_runtime_callee_reused
        ),
        format!(
            "placeholder_runtime_callee_generated={}",
            metadata.placeholder_runtime_callee_generated
        ),
        format!(
            "placeholder_transform_status={}",
            metadata.placeholder_transform_status
        ),
        format!(
            "placeholder_transform_candidates={}",
            metadata.placeholder_transform_candidates.join(",")
        ),
        format!(
            "placeholder_transform_skipped_functions={}",
            metadata.placeholder_transform_skipped_functions.join(",")
        ),
        format!(
            "placeholder_transform_candidate_count={}",
            metadata.placeholder_transform_candidate_count
        ),
        format!(
            "placeholder_transform_skipped_count={}",
            metadata.placeholder_transform_skipped_count
        ),
        format!(
            "placeholder_transform_candidate_component_count={}",
            metadata.placeholder_transform_candidate_component_count
        ),
        format!(
            "placeholder_transform_candidate_hook_count={}",
            metadata.placeholder_transform_candidate_hook_count
        ),
        format!(
            "placeholder_transform_transformed_component_count={}",
            metadata.placeholder_transform_transformed_component_count
        ),
        format!(
            "placeholder_transform_transformed_hook_count={}",
            metadata.placeholder_transform_transformed_hook_count
        ),
        format!(
            "placeholder_transform_skipped_component_count={}",
            metadata.placeholder_transform_skipped_component_count
        ),
        format!(
            "placeholder_transform_skipped_hook_count={}",
            metadata.placeholder_transform_skipped_hook_count
        ),
        format!(
            "detected_component_function_count={}",
            metadata.detected_component_function_count
        ),
        format!(
            "detected_hook_function_count={}",
            metadata.detected_hook_function_count
        ),
        format!(
            "detected_react_functions={}",
            metadata.detected_react_functions
        ),
        format!(
            "placeholder_transforms_applied={}",
            metadata.placeholder_transforms_applied
        ),
        format!(
            "placeholder_transformed_functions={}",
            metadata.placeholder_transformed_functions.join(",")
        ),
        format!(
            "placeholder_runtime_callee_name={}",
            metadata
                .placeholder_runtime_callee_name
                .as_deref()
                .unwrap_or("none")
        ),
        format!(
            "placeholder_runtime_callee_name_before_transform={}",
            metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref()
                .unwrap_or("none")
        ),
        format!(
            "placeholder_runtime_callee_candidates={}",
            metadata.placeholder_runtime_callee_candidates.join(",")
        ),
        format!(
            "placeholder_runtime_callee_candidate_count={}",
            metadata.placeholder_runtime_callee_candidate_count
        ),
        format!(
            "placeholder_runtime_callee_candidates_before_transform={}",
            metadata
                .placeholder_runtime_callee_candidates_before_transform
                .join(",")
        ),
        format!(
            "placeholder_runtime_callee_candidate_count_before_transform={}",
            metadata.placeholder_runtime_callee_candidate_count_before_transform
        ),
        format!(
            "placeholder_runtime_namespace_candidates={}",
            metadata.placeholder_runtime_namespace_candidates.join(",")
        ),
        format!(
            "placeholder_runtime_namespace_candidate_count={}",
            metadata.placeholder_runtime_namespace_candidate_count
        ),
        format!(
            "placeholder_runtime_namespace_candidates_before_transform={}",
            metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .join(",")
        ),
        format!(
            "placeholder_runtime_namespace_candidate_count_before_transform={}",
            metadata.placeholder_runtime_namespace_candidate_count_before_transform
        ),
    ];
    for (index, function) in metadata.react_functions.iter().enumerate() {
        let kind = match function.kind {
            ReactFunctionKind::Component => "Component",
            ReactFunctionKind::Hook => "Hook",
        };
        let loc = function
            .loc
            .as_ref()
            .map(|location| {
                format!(
                    "{}:{}-{}:{}",
                    location.start_line,
                    location.start_column,
                    location.end_line,
                    location.end_column
                )
            })
            .unwrap_or_else(|| "none".to_string());
        lines.push(format!(
            "fn[{index}] name={} kind={} loc={loc}",
            function.name, kind
        ));
    }
    lines.join("\n")
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompilerError {
    #[error("Flow syntax is not supported by the Rust frontend yet")]
    UnsupportedFlowSyntax { location: Option<SourceLocation> },
    #[error("Failed to parse input: {message}")]
    ParseFailure {
        message: String,
        reason: &'static str,
        location: Option<SourceLocation>,
    },
    #[error("Failed to emit compiled output: {message}")]
    CodegenFailure { message: String },
}

impl CompilerError {
    pub fn code(&self) -> &'static str {
        match self {
            CompilerError::UnsupportedFlowSyntax { .. } => "unsupported_flow_syntax",
            CompilerError::ParseFailure { .. } => "parse_failure",
            CompilerError::CodegenFailure { .. } => "codegen_failure",
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            CompilerError::UnsupportedFlowSyntax { .. } => "syntax",
            CompilerError::ParseFailure { .. } => "syntax",
            CompilerError::CodegenFailure { .. } => "internal",
        }
    }

    pub fn reason(&self) -> &'static str {
        match self {
            CompilerError::UnsupportedFlowSyntax { .. } => "flow_syntax_not_supported",
            CompilerError::ParseFailure { reason, .. } => reason,
            CompilerError::CodegenFailure { .. } => "codegen_error",
        }
    }

    pub fn severity(&self) -> &'static str {
        "error"
    }

    pub fn location(&self) -> Option<&SourceLocation> {
        match self {
            CompilerError::UnsupportedFlowSyntax { location } => location.as_ref(),
            CompilerError::ParseFailure { location, .. } => location.as_ref(),
            _ => None,
        }
    }
}

pub fn compile(source: &str, options: &CompilerOptions) -> Result<CompileOutput, CompilerError> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom(options.filename.clone()).into(),
        source.to_string(),
    );

    let syntax = match options.dialect {
        InputDialect::JavaScript => Syntax::Es(EsSyntax {
            jsx: true,
            fn_bind: true,
            decorators: true,
            decorators_before_export: true,
            export_default_from: true,
            import_attributes: true,
            allow_super_outside_method: true,
            allow_return_outside_function: true,
            auto_accessors: true,
            explicit_resource_management: true,
        }),
        InputDialect::TypeScript => Syntax::Typescript(TsSyntax {
            tsx: true,
            decorators: true,
            dts: false,
            no_early_errors: true,
            disallow_ambiguous_jsx_like: false,
        }),
        InputDialect::Flow => Syntax::Es(EsSyntax {
            jsx: true,
            fn_bind: true,
            decorators: true,
            decorators_before_export: true,
            export_default_from: true,
            import_attributes: true,
            allow_super_outside_method: true,
            allow_return_outside_function: true,
            auto_accessors: true,
            explicit_resource_management: true,
        }),
    };

    let comments = SingleThreadedComments::default();
    let lexer = Lexer::new(
        syntax,
        EsVersion::EsNext,
        StringInput::from(&*fm),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);
    let (metadata, code) = if options.is_module {
        let mut module = parser.parse_module().map_err(|err| {
            let message = err.kind().msg().to_string();
            let reason = parse_syntax_error_reason(err.kind());
            let location = span_to_location(&cm, err.span());
            if options.dialect == InputDialect::Flow {
                CompilerError::UnsupportedFlowSyntax { location }
            } else {
                CompilerError::ParseFailure {
                    message,
                    reason,
                    location,
                }
            }
        })?;
        let react_functions = collect_react_functions_in_module(&cm, &module);
        let original_statement_count = module.body.len();
        let mut placeholder_transform_candidates =
            collect_placeholder_transform_candidate_names_for_module(&module, &react_functions)
                .into_iter()
                .collect::<Vec<_>>();
        sort_and_dedup_names(&mut placeholder_transform_candidates);
        let runtime_helper_import_count_before_transform = if options.apply_placeholder_transforms {
            count_runtime_helper_imports(&module)
        } else {
            0
        };
        let (
            placeholder_runtime_callee_name_before_transform,
            placeholder_runtime_callee_candidates_before_transform,
            placeholder_runtime_namespace_candidates_before_transform,
        ) = if options.apply_placeholder_transforms {
            let runtime_scan_before = runtime_memo_callee_scan_for_module(&module);
            (
                select_runtime_callee_name(&runtime_scan_before.runtime_callee_bindings),
                sorted_runtime_callee_candidates(&runtime_scan_before.runtime_callee_bindings),
                sorted_runtime_namespace_candidates(&runtime_scan_before.runtime_namespace_bindings),
            )
        } else {
            (None, Vec::new(), Vec::new())
        };
        let mut placeholder_transformed_functions = if options.apply_placeholder_transforms {
            apply_placeholder_compilation_to_module(&mut module, &react_functions)
        } else {
            Vec::new()
        };
        let transformed_count = placeholder_transformed_functions.len();
        let transformed_statement_count = module.body.len();
        let runtime_helper_import_count_after_transform = if options.apply_placeholder_transforms {
            count_runtime_helper_imports(&module)
        } else {
            0
        };
        let runtime_helper_import_added = runtime_helper_import_count_after_transform
            > runtime_helper_import_count_before_transform;
        let (
            placeholder_runtime_callee_name,
            placeholder_runtime_callee_candidates,
            placeholder_runtime_namespace_candidates,
        ) =
            if options.apply_placeholder_transforms {
                let runtime_scan_after = runtime_memo_callee_scan_for_module(&module);
                (
                    select_runtime_callee_name(&runtime_scan_after.runtime_callee_bindings),
                    sorted_runtime_callee_candidates(&runtime_scan_after.runtime_callee_bindings),
                    sorted_runtime_namespace_candidates(
                        &runtime_scan_after.runtime_namespace_bindings,
                    ),
                )
            } else {
                (None, Vec::new(), Vec::new())
            };
        let placeholder_runtime_callee_reused = options.apply_placeholder_transforms
            && transformed_count > 0
            && placeholder_runtime_callee_name_before_transform.is_some()
            && placeholder_runtime_callee_name_before_transform == placeholder_runtime_callee_name;
        let placeholder_runtime_callee_generated = options.apply_placeholder_transforms
            && transformed_count > 0
            && placeholder_runtime_callee_name_before_transform.is_none()
            && placeholder_runtime_callee_name.is_some()
            && runtime_helper_import_added;
        sort_and_dedup_names(&mut placeholder_transformed_functions);
        let placeholder_transform_skipped_functions = compute_placeholder_transform_skipped_functions(
            &placeholder_transform_candidates,
            &placeholder_transformed_functions,
        );
        let candidate_kind_counts = count_placeholder_transform_names_by_kind(
            &placeholder_transform_candidates,
            &react_functions,
        );
        let transformed_kind_counts = count_placeholder_transform_names_by_kind(
            &placeholder_transformed_functions,
            &react_functions,
        );
        let skipped_kind_counts = count_placeholder_transform_names_by_kind(
            &placeholder_transform_skipped_functions,
            &react_functions,
        );
        let placeholder_transform_status = derive_placeholder_transform_status(
            options.apply_placeholder_transforms,
            true,
            placeholder_transform_candidates.len(),
            transformed_count,
            placeholder_runtime_callee_name_before_transform.is_some(),
        )
        .to_string();
        let placeholder_runtime_callee_candidate_count_before_transform =
            placeholder_runtime_callee_candidates_before_transform.len();
        let placeholder_runtime_namespace_candidate_count_before_transform =
            placeholder_runtime_namespace_candidates_before_transform.len();
        let placeholder_runtime_callee_candidate_count = placeholder_runtime_callee_candidates.len();
        let placeholder_runtime_namespace_candidate_count =
            placeholder_runtime_namespace_candidates.len();
        let detected_kind_counts = count_detected_react_functions_by_kind(&react_functions);
        let metadata = ParseMetadata {
            statement_count: original_statement_count,
            statement_count_after_transform: transformed_statement_count,
            placeholder_runtime_helper_import_count_before_transform:
                runtime_helper_import_count_before_transform,
            placeholder_runtime_helper_import_count_after_transform:
                runtime_helper_import_count_after_transform,
            placeholder_runtime_helper_import_added: runtime_helper_import_added,
            placeholder_runtime_callee_reused,
            placeholder_runtime_callee_generated,
            placeholder_transform_status,
            placeholder_transform_candidate_count: placeholder_transform_candidates.len(),
            placeholder_transform_skipped_count: placeholder_transform_skipped_functions.len(),
            placeholder_transform_candidate_component_count: candidate_kind_counts.component_count,
            placeholder_transform_candidate_hook_count: candidate_kind_counts.hook_count,
            placeholder_transform_transformed_component_count:
                transformed_kind_counts.component_count,
            placeholder_transform_transformed_hook_count: transformed_kind_counts.hook_count,
            placeholder_transform_skipped_component_count: skipped_kind_counts.component_count,
            placeholder_transform_skipped_hook_count: skipped_kind_counts.hook_count,
            detected_component_function_count: detected_kind_counts.component_count,
            detected_hook_function_count: detected_kind_counts.hook_count,
            placeholder_transform_candidates,
            placeholder_transform_skipped_functions,
            detected_react_functions: react_functions.len(),
            react_functions,
            placeholder_transforms_applied: transformed_count,
            placeholder_transformed_functions,
            placeholder_runtime_callee_name_before_transform,
            placeholder_runtime_callee_candidates_before_transform,
            placeholder_runtime_callee_candidate_count_before_transform,
            placeholder_runtime_namespace_candidates_before_transform,
            placeholder_runtime_namespace_candidate_count_before_transform,
            placeholder_runtime_callee_name,
            placeholder_runtime_callee_candidates,
            placeholder_runtime_callee_candidate_count,
            placeholder_runtime_namespace_candidates,
            placeholder_runtime_namespace_candidate_count,
        };
        (metadata, emit_module(&cm, &comments, &module)?)
    } else {
        let mut script = parser.parse_script().map_err(|err| {
            let message = err.kind().msg().to_string();
            let reason = parse_syntax_error_reason(err.kind());
            let location = span_to_location(&cm, err.span());
            if options.dialect == InputDialect::Flow {
                CompilerError::UnsupportedFlowSyntax { location }
            } else {
                CompilerError::ParseFailure {
                    message,
                    reason,
                    location,
                }
            }
        })?;
        let react_functions = collect_react_functions_in_script(&cm, &script);
        let original_statement_count = script.body.len();
        let mut placeholder_transform_candidates =
            collect_placeholder_transform_candidate_names_for_script(&react_functions)
                .into_iter()
                .collect::<Vec<_>>();
        sort_and_dedup_names(&mut placeholder_transform_candidates);
        let (
            placeholder_runtime_callee_name_before_transform,
            placeholder_runtime_callee_candidates_before_transform,
            placeholder_runtime_namespace_candidates_before_transform,
        ) = if options.apply_placeholder_transforms {
            let runtime_scan_before = runtime_memo_callee_scan_for_script(&script);
            (
                select_runtime_callee_name(&runtime_scan_before.runtime_callee_bindings),
                sorted_runtime_callee_candidates(&runtime_scan_before.runtime_callee_bindings),
                sorted_runtime_namespace_candidates(&runtime_scan_before.runtime_namespace_bindings),
            )
        } else {
            (None, Vec::new(), Vec::new())
        };
        let mut placeholder_transformed_functions = if options.apply_placeholder_transforms {
            apply_placeholder_compilation_to_script(&mut script, &react_functions)
        } else {
            Vec::new()
        };
        let transformed_count = placeholder_transformed_functions.len();
        let transformed_statement_count = script.body.len();
        let (
            placeholder_runtime_callee_name,
            placeholder_runtime_callee_candidates,
            placeholder_runtime_namespace_candidates,
        ) =
            if options.apply_placeholder_transforms {
                let runtime_scan_after = runtime_memo_callee_scan_for_script(&script);
                (
                    select_runtime_callee_name(&runtime_scan_after.runtime_callee_bindings),
                    sorted_runtime_callee_candidates(&runtime_scan_after.runtime_callee_bindings),
                    sorted_runtime_namespace_candidates(
                        &runtime_scan_after.runtime_namespace_bindings,
                    ),
                )
            } else {
                (None, Vec::new(), Vec::new())
            };
        let placeholder_runtime_callee_reused = options.apply_placeholder_transforms
            && transformed_count > 0
            && placeholder_runtime_callee_name_before_transform.is_some()
            && placeholder_runtime_callee_name_before_transform == placeholder_runtime_callee_name;
        sort_and_dedup_names(&mut placeholder_transformed_functions);
        let placeholder_transform_skipped_functions = compute_placeholder_transform_skipped_functions(
            &placeholder_transform_candidates,
            &placeholder_transformed_functions,
        );
        let candidate_kind_counts = count_placeholder_transform_names_by_kind(
            &placeholder_transform_candidates,
            &react_functions,
        );
        let transformed_kind_counts = count_placeholder_transform_names_by_kind(
            &placeholder_transformed_functions,
            &react_functions,
        );
        let skipped_kind_counts = count_placeholder_transform_names_by_kind(
            &placeholder_transform_skipped_functions,
            &react_functions,
        );
        let placeholder_transform_status = derive_placeholder_transform_status(
            options.apply_placeholder_transforms,
            false,
            placeholder_transform_candidates.len(),
            transformed_count,
            placeholder_runtime_callee_name_before_transform.is_some(),
        )
        .to_string();
        let placeholder_runtime_callee_candidate_count_before_transform =
            placeholder_runtime_callee_candidates_before_transform.len();
        let placeholder_runtime_namespace_candidate_count_before_transform =
            placeholder_runtime_namespace_candidates_before_transform.len();
        let placeholder_runtime_callee_candidate_count = placeholder_runtime_callee_candidates.len();
        let placeholder_runtime_namespace_candidate_count =
            placeholder_runtime_namespace_candidates.len();
        let detected_kind_counts = count_detected_react_functions_by_kind(&react_functions);
        let metadata = ParseMetadata {
            statement_count: original_statement_count,
            statement_count_after_transform: transformed_statement_count,
            placeholder_runtime_helper_import_count_before_transform: 0,
            placeholder_runtime_helper_import_count_after_transform: 0,
            placeholder_runtime_helper_import_added: false,
            placeholder_runtime_callee_reused,
            placeholder_runtime_callee_generated: false,
            placeholder_transform_status,
            placeholder_transform_candidate_count: placeholder_transform_candidates.len(),
            placeholder_transform_skipped_count: placeholder_transform_skipped_functions.len(),
            placeholder_transform_candidate_component_count: candidate_kind_counts.component_count,
            placeholder_transform_candidate_hook_count: candidate_kind_counts.hook_count,
            placeholder_transform_transformed_component_count:
                transformed_kind_counts.component_count,
            placeholder_transform_transformed_hook_count: transformed_kind_counts.hook_count,
            placeholder_transform_skipped_component_count: skipped_kind_counts.component_count,
            placeholder_transform_skipped_hook_count: skipped_kind_counts.hook_count,
            detected_component_function_count: detected_kind_counts.component_count,
            detected_hook_function_count: detected_kind_counts.hook_count,
            placeholder_transform_candidates,
            placeholder_transform_skipped_functions,
            detected_react_functions: react_functions.len(),
            react_functions,
            placeholder_transforms_applied: transformed_count,
            placeholder_transformed_functions,
            placeholder_runtime_callee_name_before_transform,
            placeholder_runtime_callee_candidates_before_transform,
            placeholder_runtime_callee_candidate_count_before_transform,
            placeholder_runtime_namespace_candidates_before_transform,
            placeholder_runtime_namespace_candidate_count_before_transform,
            placeholder_runtime_callee_name,
            placeholder_runtime_callee_candidates,
            placeholder_runtime_callee_candidate_count,
            placeholder_runtime_namespace_candidates,
            placeholder_runtime_namespace_candidate_count,
        };
        (metadata, emit_script(&cm, &comments, &script)?)
    };

    Ok(CompileOutput { code, metadata })
}

fn parse_syntax_error_reason(kind: &ParserSyntaxError) -> &'static str {
    match kind {
        ParserSyntaxError::Unexpected { got, .. }
            if got.contains("eof") || got.contains("EOF") || got.contains("<eof>") =>
        {
            "unexpected_eof"
        }
        ParserSyntaxError::Unexpected { .. }
        | ParserSyntaxError::UnexpectedTokenWithSuggestions { .. }
        | ParserSyntaxError::UnexpectedChar { .. }
        | ParserSyntaxError::Hash => "unexpected_token",
        ParserSyntaxError::Expected(..)
        | ParserSyntaxError::ExpectedIdent
        | ParserSyntaxError::ExpectedSemi
        | ParserSyntaxError::ExpectedSemiForExprStmt { .. }
        | ParserSyntaxError::ExpectedUnicodeEscape
        | ParserSyntaxError::ExpectedDigit { .. } => "expected_token",
        ParserSyntaxError::UnterminatedBlockComment
        | ParserSyntaxError::UnterminatedStrLit
        | ParserSyntaxError::UnterminatedRegExp
        | ParserSyntaxError::UnterminatedTpl
        | ParserSyntaxError::UnterminatedJSXContents => "unterminated_syntax",
        ParserSyntaxError::InvalidIdentChar
        | ParserSyntaxError::InvalidStrEscape
        | ParserSyntaxError::InvalidUnicodeEscape
        | ParserSyntaxError::BadCharacterEscapeSequence { .. } => "invalid_syntax",
        ParserSyntaxError::Eof => "unexpected_eof",
        _ => classify_parse_error_message(kind.msg().as_ref()),
    }
}

fn classify_parse_error_message(message: &str) -> &'static str {
    let normalized = message.to_ascii_lowercase();
    if message.contains("Unexpected token") || message.starts_with("Unexpected character") {
        return "unexpected_token";
    }
    if message.starts_with("Unexpected eof") || normalized.contains("unexpected eof") {
        return "unexpected_eof";
    }
    if message.starts_with("Expected ") || normalized.contains(" expected") {
        return "expected_token";
    }
    if message.starts_with("Unterminated ") {
        return "unterminated_syntax";
    }
    if message.starts_with("Invalid ") {
        return "invalid_syntax";
    }
    "parse_error"
}

fn emit_module(
    cm: &Lrc<SourceMap>,
    comments: &SingleThreadedComments,
    module: &Module,
) -> Result<String, CompilerError> {
    let mut output = vec![];
    {
        let writer = JsWriter::new(cm.clone(), "\n", &mut output, None);
        let mut emitter = Emitter {
            cfg: CodegenConfig::default(),
            comments: Some(comments),
            cm: cm.clone(),
            wr: writer,
        };
        emitter
            .emit_module(module)
            .map_err(|err| CompilerError::CodegenFailure {
                message: err.to_string(),
            })?;
    }
    String::from_utf8(output).map_err(|err| CompilerError::CodegenFailure {
        message: err.to_string(),
    })
}

fn emit_script(
    cm: &Lrc<SourceMap>,
    comments: &SingleThreadedComments,
    script: &Script,
) -> Result<String, CompilerError> {
    let mut output = vec![];
    {
        let writer = JsWriter::new(cm.clone(), "\n", &mut output, None);
        let mut emitter = Emitter {
            cfg: CodegenConfig::default(),
            comments: Some(comments),
            cm: cm.clone(),
            wr: writer,
        };
        emitter
            .emit_script(script)
            .map_err(|err| CompilerError::CodegenFailure {
                message: err.to_string(),
            })?;
    }
    String::from_utf8(output).map_err(|err| CompilerError::CodegenFailure {
        message: err.to_string(),
    })
}

fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .map(|ch| ch.is_ascii_uppercase())
        .unwrap_or(false)
}

fn is_hook_name(name: &str) -> bool {
    let mut chars = name.chars();
    if chars.next() != Some('u') || chars.next() != Some('s') || chars.next() != Some('e') {
        return false;
    }

    chars
        .next()
        .map(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        .unwrap_or(false)
}

fn react_function_kind(name: &str) -> Option<ReactFunctionKind> {
    if is_component_name(name) {
        Some(ReactFunctionKind::Component)
    } else if is_hook_name(name) {
        Some(ReactFunctionKind::Hook)
    } else {
        None
    }
}

fn span_to_location(cm: &Lrc<SourceMap>, span: Span) -> Option<SourceLocation> {
    if span.is_dummy() {
        return None;
    }
    let start = cm.lookup_char_pos(span.lo());
    let end = cm.lookup_char_pos(span.hi());
    Some(SourceLocation {
        start_line: start.line,
        start_column: start.col_display,
        end_line: end.line,
        end_column: end.col_display,
    })
}

fn unwrap_expression(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(paren_expr) => unwrap_expression(paren_expr.expr.as_ref()),
        Expr::TsAs(ts_as_expr) => unwrap_expression(ts_as_expr.expr.as_ref()),
        Expr::TsTypeAssertion(ts_type_assertion) => {
            unwrap_expression(ts_type_assertion.expr.as_ref())
        }
        Expr::TsConstAssertion(ts_const_assertion) => {
            unwrap_expression(ts_const_assertion.expr.as_ref())
        }
        Expr::TsNonNull(ts_non_null_expr) => unwrap_expression(ts_non_null_expr.expr.as_ref()),
        Expr::TsSatisfies(ts_satisfies_expr) => unwrap_expression(ts_satisfies_expr.expr.as_ref()),
        Expr::TsInstantiation(ts_instantiation_expr) => {
            unwrap_expression(ts_instantiation_expr.expr.as_ref())
        }
        _ => expr,
    }
}

fn unwrap_expression_mut(expr: &mut Expr) -> &mut Expr {
    match expr {
        Expr::Paren(paren_expr) => unwrap_expression_mut(paren_expr.expr.as_mut()),
        Expr::TsAs(ts_as_expr) => unwrap_expression_mut(ts_as_expr.expr.as_mut()),
        Expr::TsTypeAssertion(ts_type_assertion) => {
            unwrap_expression_mut(ts_type_assertion.expr.as_mut())
        }
        Expr::TsConstAssertion(ts_const_assertion) => {
            unwrap_expression_mut(ts_const_assertion.expr.as_mut())
        }
        Expr::TsNonNull(ts_non_null_expr) => unwrap_expression_mut(ts_non_null_expr.expr.as_mut()),
        Expr::TsSatisfies(ts_satisfies_expr) => {
            unwrap_expression_mut(ts_satisfies_expr.expr.as_mut())
        }
        Expr::TsInstantiation(ts_instantiation_expr) => {
            unwrap_expression_mut(ts_instantiation_expr.expr.as_mut())
        }
        _ => expr,
    }
}

fn collect_react_functions_in_decl(cm: &Lrc<SourceMap>, decl: &Decl) -> Vec<ReactFunction> {
    match decl {
        Decl::Fn(fn_decl) => react_function_kind(fn_decl.ident.sym.as_ref())
            .map(|kind| ReactFunction {
                name: fn_decl.ident.sym.to_string(),
                kind,
                loc: span_to_location(cm, fn_decl.function.span),
            })
            .into_iter()
            .collect(),
        Decl::Var(var_decl) => var_decl
            .decls
            .iter()
            .filter_map(|declarator| {
                let Pat::Ident(binding) = &declarator.name else {
                    return None;
                };
                let function_span = match declarator.init.as_deref().map(unwrap_expression) {
                    Some(Expr::Fn(fn_expr)) => Some(fn_expr.function.span),
                    Some(Expr::Arrow(arrow_expr)) => Some(arrow_expr.span),
                    _ => None,
                };
                if function_span.is_none() {
                    return None;
                }

                react_function_kind(binding.id.sym.as_ref()).map(|kind| ReactFunction {
                    name: binding.id.sym.to_string(),
                    kind,
                    loc: function_span
                        .and_then(|span| span_to_location(cm, span))
                        .or_else(|| span_to_location(cm, binding.id.span)),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn collect_react_functions_in_stmt(cm: &Lrc<SourceMap>, stmt: &Stmt) -> Vec<ReactFunction> {
    match stmt {
        Stmt::Decl(decl) => collect_react_functions_in_decl(cm, decl),
        Stmt::Expr(expr_stmt) => collect_react_functions_in_expr(cm, expr_stmt.expr.as_ref()),
        _ => Vec::new(),
    }
}

fn collect_react_functions_in_expr(cm: &Lrc<SourceMap>, expr: &Expr) -> Vec<ReactFunction> {
    let Expr::Assign(assign_expr) = unwrap_expression(expr) else {
        return Vec::new();
    };
    if assign_expr.op != swc_ecma_ast::AssignOp::Assign {
        return Vec::new();
    }
    let Some(target_name) = assign_target_ident(&assign_expr.left) else {
        return Vec::new();
    };
    let Some(kind) = react_function_kind(target_name.as_str()) else {
        return Vec::new();
    };
    let function_span = match unwrap_expression(assign_expr.right.as_ref()) {
        Expr::Fn(fn_expr) => Some(fn_expr.function.span),
        Expr::Arrow(arrow_expr) => Some(arrow_expr.span),
        _ => None,
    };
    let Some(function_span) = function_span else {
        return Vec::new();
    };
    vec![ReactFunction {
        name: target_name,
        kind,
        loc: span_to_location(cm, function_span),
    }]
}

fn collect_react_functions_in_module(cm: &Lrc<SourceMap>, module: &Module) -> Vec<ReactFunction> {
    let bindings = collect_top_level_bindings(module);
    let mut functions: Vec<ReactFunction> = module
        .body
        .iter()
        .flat_map(|item| match item {
            ModuleItem::Stmt(stmt) => collect_react_functions_in_stmt(cm, stmt),
            ModuleItem::ModuleDecl(module_decl) => match module_decl {
                ModuleDecl::ExportDecl(export_decl) => {
                    collect_react_functions_in_decl(cm, &export_decl.decl)
                }
                ModuleDecl::ExportDefaultDecl(default_decl) => match &default_decl.decl {
                    DefaultDecl::Fn(fn_expr) => match fn_expr.ident.as_ref() {
                        Some(ident) => vec![ReactFunction {
                            name: ident.sym.to_string(),
                            kind: react_function_kind(ident.sym.as_ref())
                                .unwrap_or(ReactFunctionKind::Component),
                            loc: span_to_location(cm, fn_expr.function.span),
                        }],
                        None => vec![ReactFunction {
                            name: DEFAULT_EXPORT_COMPONENT_NAME.to_string(),
                            kind: ReactFunctionKind::Component,
                            loc: span_to_location(cm, fn_expr.function.span),
                        }],
                    },
                    _ => Vec::new(),
                },
                ModuleDecl::ExportDefaultExpr(default_expr) => {
                    collect_react_functions_from_default_export_expr(cm, default_expr)
                }
                _ => Vec::new(),
            },
        })
        .collect();

    let fixture_entrypoint_names = collect_fixture_entrypoint_function_names(module);
    if !fixture_entrypoint_names.is_empty() {
        let resolved_fixture_entrypoint_names =
            resolve_function_binding_names(&bindings, &fixture_entrypoint_names);
        let fixture_entrypoint_functions =
            collect_named_functions_in_module(cm, module, &resolved_fixture_entrypoint_names);
        functions = merge_react_functions(functions, fixture_entrypoint_functions);
    }
    let default_export_function_names = collect_default_export_function_names(module, &bindings);
    if !default_export_function_names.is_empty() {
        let default_export_functions =
            collect_named_functions_in_module(cm, module, &default_export_function_names);
        functions = merge_react_functions(functions, default_export_functions);
    }

    sort_react_functions(&mut functions);
    functions
}

fn collect_react_functions_in_script(cm: &Lrc<SourceMap>, script: &Script) -> Vec<ReactFunction> {
    let mut functions: Vec<ReactFunction> = script
        .body
        .iter()
        .flat_map(|stmt| collect_react_functions_in_stmt(cm, stmt))
        .collect();
    sort_react_functions(&mut functions);
    functions
}

fn collect_react_functions_from_default_export_expr(
    cm: &Lrc<SourceMap>,
    default_expr: &swc_ecma_ast::ExportDefaultExpr,
) -> Vec<ReactFunction> {
    match unwrap_expression(default_expr.expr.as_ref()) {
        Expr::Fn(fn_expr) => match fn_expr.ident.as_ref() {
            Some(ident) => vec![ReactFunction {
                name: ident.sym.to_string(),
                kind: react_function_kind(ident.sym.as_ref())
                    .unwrap_or(ReactFunctionKind::Component),
                loc: span_to_location(cm, fn_expr.function.span),
            }],
            None => vec![ReactFunction {
                name: DEFAULT_EXPORT_COMPONENT_NAME.to_string(),
                kind: ReactFunctionKind::Component,
                loc: span_to_location(cm, fn_expr.function.span),
            }],
        },
        Expr::Arrow(arrow_expr) => vec![ReactFunction {
            name: DEFAULT_EXPORT_COMPONENT_NAME.to_string(),
            kind: ReactFunctionKind::Component,
            loc: span_to_location(cm, arrow_expr.span),
        }],
        _ => Vec::new(),
    }
}

fn merge_react_functions(
    mut existing: Vec<ReactFunction>,
    additional: Vec<ReactFunction>,
) -> Vec<ReactFunction> {
    let mut seen: HashSet<String> = existing.iter().map(react_function_key).collect();
    for function in additional {
        let key = react_function_key(&function);
        if seen.insert(key) {
            existing.push(function);
        }
    }
    existing
}

fn react_function_key(function: &ReactFunction) -> String {
    match &function.loc {
        Some(loc) => format!(
            "{}:{}:{}:{}:{}",
            function.name, loc.start_line, loc.start_column, loc.end_line, loc.end_column
        ),
        None => format!("{}:none", function.name),
    }
}

fn sort_react_functions(functions: &mut [ReactFunction]) {
    functions.sort_by(|a, b| react_function_sort_key(a).cmp(&react_function_sort_key(b)));
}

fn sort_and_dedup_names(names: &mut Vec<String>) {
    names.sort();
    names.dedup();
}

fn compute_placeholder_transform_skipped_functions(
    candidates: &[String],
    transformed: &[String],
) -> Vec<String> {
    let transformed_names: HashSet<&str> = transformed.iter().map(String::as_str).collect();
    candidates
        .iter()
        .filter(|candidate| !transformed_names.contains(candidate.as_str()))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct PlaceholderTransformKindCounts {
    component_count: usize,
    hook_count: usize,
}

fn count_placeholder_transform_names_by_kind(
    names: &[String],
    react_functions: &[ReactFunction],
) -> PlaceholderTransformKindCounts {
    names
        .iter()
        .fold(PlaceholderTransformKindCounts::default(), |mut counts, name| {
            let kind = react_functions
                .iter()
                .find(|function| function.name == *name)
                .map(|function| function.kind.clone())
                .unwrap_or_else(|| {
                    if is_hook_name(name) {
                        ReactFunctionKind::Hook
                    } else {
                        ReactFunctionKind::Component
                    }
                });
            match kind {
                ReactFunctionKind::Component => counts.component_count += 1,
                ReactFunctionKind::Hook => counts.hook_count += 1,
            }
            counts
        })
}

fn count_detected_react_functions_by_kind(
    react_functions: &[ReactFunction],
) -> PlaceholderTransformKindCounts {
    react_functions
        .iter()
        .fold(PlaceholderTransformKindCounts::default(), |mut counts, function| {
            match function.kind {
                ReactFunctionKind::Component => counts.component_count += 1,
                ReactFunctionKind::Hook => counts.hook_count += 1,
            }
            counts
        })
}

fn derive_placeholder_transform_status(
    apply_placeholder_transforms: bool,
    is_module: bool,
    candidate_count: usize,
    transformed_count: usize,
    runtime_callee_available_before_transform: bool,
) -> &'static str {
    if !apply_placeholder_transforms {
        return "disabled";
    }
    if candidate_count == 0 {
        return "no_candidates";
    }
    if transformed_count > 0 {
        return "transformed";
    }
    if !is_module && !runtime_callee_available_before_transform {
        return "blocked_missing_runtime_callee";
    }
    "no_op"
}

fn react_function_sort_key(function: &ReactFunction) -> (usize, usize, usize, usize, &str, &str) {
    let (start_line, start_column, end_line, end_column) = match &function.loc {
        Some(loc) => (
            loc.start_line,
            loc.start_column,
            loc.end_line,
            loc.end_column,
        ),
        None => (usize::MAX, usize::MAX, usize::MAX, usize::MAX),
    };
    let kind = match function.kind {
        ReactFunctionKind::Component => "component",
        ReactFunctionKind::Hook => "hook",
    };
    (
        start_line,
        start_column,
        end_line,
        end_column,
        function.name.as_str(),
        kind,
    )
}

fn collect_fixture_entrypoint_function_names(module: &Module) -> HashSet<String> {
    module
        .body
        .iter()
        .flat_map(|item| match item {
            ModuleItem::Stmt(Stmt::Expr(expr_stmt)) => {
                collect_fixture_entrypoint_names_from_expr(expr_stmt.expr.as_ref())
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) => {
                collect_fixture_entrypoint_names_from_var_decl(var_decl)
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
                match &export_decl.decl {
                    Decl::Var(var_decl) => collect_fixture_entrypoint_names_from_var_decl(var_decl),
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        })
        .collect()
}

enum TopLevelBinding {
    FunctionLike,
    Alias(String),
}

fn collect_top_level_bindings(module: &Module) -> HashMap<String, TopLevelBinding> {
    let mut bindings = HashMap::new();
    for item in &module.body {
        match item {
            ModuleItem::Stmt(stmt) => match stmt {
                Stmt::Decl(decl) => record_top_level_bindings_from_decl(decl, &mut bindings),
                Stmt::Expr(expr_stmt) => {
                    record_top_level_bindings_from_expr(expr_stmt.expr.as_ref(), &mut bindings)
                }
                _ => {}
            },
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
                record_top_level_bindings_from_decl(&export_decl.decl, &mut bindings)
            }
            _ => {}
        }
    }
    bindings
}

fn record_top_level_bindings_from_decl(
    decl: &Decl,
    bindings: &mut HashMap<String, TopLevelBinding>,
) {
    match decl {
        Decl::Fn(fn_decl) => {
            bindings.insert(fn_decl.ident.sym.to_string(), TopLevelBinding::FunctionLike);
        }
        Decl::Var(var_decl) => {
            for declarator in &var_decl.decls {
                let Pat::Ident(binding) = &declarator.name else {
                    continue;
                };
                let Some(init) = declarator.init.as_deref().map(unwrap_expression) else {
                    continue;
                };
                match init {
                    Expr::Fn(_) | Expr::Arrow(_) => {
                        bindings.insert(binding.id.sym.to_string(), TopLevelBinding::FunctionLike);
                    }
                    Expr::Ident(ident) => {
                        bindings.insert(
                            binding.id.sym.to_string(),
                            TopLevelBinding::Alias(ident.sym.to_string()),
                        );
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn record_top_level_bindings_from_expr(
    expr: &Expr,
    bindings: &mut HashMap<String, TopLevelBinding>,
) {
    let Expr::Assign(assign_expr) = unwrap_expression(expr) else {
        return;
    };
    if assign_expr.op != swc_ecma_ast::AssignOp::Assign {
        return;
    }
    let Some(target_name) = assign_target_ident(&assign_expr.left) else {
        return;
    };
    match unwrap_expression(assign_expr.right.as_ref()) {
        Expr::Fn(_) | Expr::Arrow(_) => {
            bindings.insert(target_name, TopLevelBinding::FunctionLike);
        }
        Expr::Ident(ident) => {
            if matches!(
                bindings.get(target_name.as_str()),
                Some(TopLevelBinding::FunctionLike)
            ) {
                return;
            }
            bindings.insert(target_name, TopLevelBinding::Alias(ident.sym.to_string()));
        }
        _ => {}
    }
}

fn assign_target_ident(target: &AssignTarget) -> Option<String> {
    match target {
        AssignTarget::Simple(simple) => simple_assign_target_ident(simple),
        _ => None,
    }
}

fn simple_assign_target_ident(target: &SimpleAssignTarget) -> Option<String> {
    match target {
        SimpleAssignTarget::Ident(binding) => Some(binding.id.sym.to_string()),
        SimpleAssignTarget::Paren(paren_expr) => expression_ident(paren_expr.expr.as_ref()),
        SimpleAssignTarget::TsAs(ts_as_expr) => expression_ident(ts_as_expr.expr.as_ref()),
        SimpleAssignTarget::TsSatisfies(ts_satisfies_expr) => {
            expression_ident(ts_satisfies_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsNonNull(ts_non_null_expr) => {
            expression_ident(ts_non_null_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsTypeAssertion(ts_type_assertion) => {
            expression_ident(ts_type_assertion.expr.as_ref())
        }
        SimpleAssignTarget::TsInstantiation(ts_instantiation) => {
            expression_ident(ts_instantiation.expr.as_ref())
        }
        _ => None,
    }
}

fn expression_ident(expr: &Expr) -> Option<String> {
    match unwrap_expression(expr) {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        _ => None,
    }
}

fn resolve_function_binding_name(
    bindings: &HashMap<String, TopLevelBinding>,
    name: &str,
) -> Option<String> {
    let mut current = name.to_string();
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current.clone()) {
            return None;
        }
        match bindings.get(current.as_str()) {
            Some(TopLevelBinding::FunctionLike) => return Some(current),
            Some(TopLevelBinding::Alias(next)) => {
                current = next.clone();
            }
            None => return None,
        }
    }
}

fn resolve_function_binding_names(
    bindings: &HashMap<String, TopLevelBinding>,
    names: &HashSet<String>,
) -> HashSet<String> {
    names
        .iter()
        .filter_map(|name| resolve_function_binding_name(bindings, name))
        .collect()
}

fn collect_fixture_entrypoint_names_from_expr(expr: &Expr) -> Vec<String> {
    let Expr::Assign(assign_expr) = unwrap_expression(expr) else {
        return Vec::new();
    };
    if assign_expr.op != swc_ecma_ast::AssignOp::Assign {
        return Vec::new();
    }

    if let Some(target_name) = assign_target_ident(&assign_expr.left) {
        if target_name == "FIXTURE_ENTRYPOINT" {
            if let Expr::Object(object_literal) = unwrap_expression(assign_expr.right.as_ref()) {
                if let Some(name) = fixture_entrypoint_fn_name_from_object_literal(object_literal) {
                    return vec![name];
                }
            }
        }
    }

    if assign_target_is_fixture_entrypoint_fn(&assign_expr.left) {
        if let Some(name) = function_name_from_expr(assign_expr.right.as_ref()) {
            return vec![name];
        }
    }

    Vec::new()
}

fn collect_default_export_function_names(
    module: &Module,
    bindings: &HashMap<String, TopLevelBinding>,
) -> HashSet<String> {
    module
        .body
        .iter()
        .filter_map(|item| {
            let ModuleItem::ModuleDecl(module_decl) = item else {
                return None;
            };
            match module_decl {
                ModuleDecl::ExportDefaultDecl(default_decl) => match &default_decl.decl {
                    DefaultDecl::Fn(fn_expr) => {
                        fn_expr.ident.as_ref().map(|ident| ident.sym.to_string())
                    }
                    _ => None,
                },
                ModuleDecl::ExportDefaultExpr(default_expr) => {
                    match unwrap_expression(default_expr.expr.as_ref()) {
                        Expr::Ident(ident) => Some(ident.sym.to_string()),
                        Expr::Fn(fn_expr) => {
                            fn_expr.ident.as_ref().map(|ident| ident.sym.to_string())
                        }
                        _ => None,
                    }
                }
                ModuleDecl::ExportNamed(named_export)
                    if named_export.src.is_none() && !named_export.type_only =>
                {
                    named_export
                        .specifiers
                        .iter()
                        .find_map(|specifier| match specifier {
                            swc_ecma_ast::ExportSpecifier::Named(named_specifier)
                                if !named_specifier.is_type_only
                                    && named_specifier
                                        .exported
                                        .as_ref()
                                        .map(|name| name.atom() == &"default")
                                        .unwrap_or(false) =>
                            {
                                match &named_specifier.orig {
                                    ModuleExportName::Ident(ident) => Some(ident.sym.to_string()),
                                    ModuleExportName::Str(_) => None,
                                }
                            }
                            _ => None,
                        })
                }
                _ => None,
            }
        })
        .filter_map(|name| resolve_function_binding_name(bindings, &name))
        .collect()
}

fn module_has_default_export_component_candidate(module: &Module) -> bool {
    module.body.iter().any(|item| {
        let ModuleItem::ModuleDecl(module_decl) = item else {
            return false;
        };
        match module_decl {
            ModuleDecl::ExportDefaultDecl(default_decl) => {
                matches!(&default_decl.decl, DefaultDecl::Fn(_))
            }
            ModuleDecl::ExportDefaultExpr(default_expr) => {
                matches!(
                    unwrap_expression(default_expr.expr.as_ref()),
                    Expr::Fn(_) | Expr::Arrow(_)
                )
            }
            _ => false,
        }
    })
}

fn collect_fixture_entrypoint_names_from_var_decl(var_decl: &VarDecl) -> Vec<String> {
    var_decl
        .decls
        .iter()
        .filter_map(|declarator| {
            let Pat::Ident(binding) = &declarator.name else {
                return None;
            };
            if binding.id.sym != *"FIXTURE_ENTRYPOINT" {
                return None;
            }
            let Some(init) = declarator.init.as_ref() else {
                return None;
            };
            let Expr::Object(object_literal) = init.as_ref() else {
                return None;
            };
            fixture_entrypoint_fn_name_from_object_literal(object_literal)
        })
        .collect()
}

fn assign_target_is_fixture_entrypoint_fn(target: &AssignTarget) -> bool {
    match target {
        AssignTarget::Simple(simple) => simple_assign_target_is_fixture_entrypoint_fn(simple),
        _ => false,
    }
}

fn simple_assign_target_is_fixture_entrypoint_fn(target: &SimpleAssignTarget) -> bool {
    match target {
        SimpleAssignTarget::Member(member_expr) => {
            member_expr_is_fixture_entrypoint_fn(member_expr)
        }
        SimpleAssignTarget::Paren(paren_expr) => {
            expression_is_fixture_entrypoint_fn_target(paren_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsAs(ts_as_expr) => {
            expression_is_fixture_entrypoint_fn_target(ts_as_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsSatisfies(ts_satisfies_expr) => {
            expression_is_fixture_entrypoint_fn_target(ts_satisfies_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsNonNull(ts_non_null_expr) => {
            expression_is_fixture_entrypoint_fn_target(ts_non_null_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsTypeAssertion(ts_type_assertion) => {
            expression_is_fixture_entrypoint_fn_target(ts_type_assertion.expr.as_ref())
        }
        SimpleAssignTarget::TsInstantiation(ts_instantiation) => {
            expression_is_fixture_entrypoint_fn_target(ts_instantiation.expr.as_ref())
        }
        _ => false,
    }
}

fn expression_is_fixture_entrypoint_fn_target(expr: &Expr) -> bool {
    match unwrap_expression(expr) {
        Expr::Member(member_expr) => member_expr_is_fixture_entrypoint_fn(member_expr),
        _ => false,
    }
}

fn member_expr_is_fixture_entrypoint_fn(member_expr: &MemberExpr) -> bool {
    let object_name = match unwrap_expression(member_expr.obj.as_ref()) {
        Expr::Ident(ident) => ident.sym.as_ref(),
        _ => return false,
    };
    if object_name != "FIXTURE_ENTRYPOINT" {
        return false;
    }
    is_member_prop_with(&member_expr.prop, "fn")
}

fn function_name_from_expr(expr: &Expr) -> Option<String> {
    match unwrap_expression(expr) {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        Expr::Fn(fn_expr) => fn_expr.ident.as_ref().map(|ident| ident.sym.to_string()),
        _ => None,
    }
}

fn fixture_entrypoint_fn_name_from_object_literal(
    object_literal: &swc_ecma_ast::ObjectLit,
) -> Option<String> {
    object_literal.props.iter().find_map(|prop_or_spread| {
        let PropOrSpread::Prop(prop) = prop_or_spread else {
            return None;
        };
        match prop.as_ref() {
            Prop::KeyValue(key_value) => {
                if !is_fn_property_name(&key_value.key) {
                    return None;
                }
                function_name_from_expr(key_value.value.as_ref())
            }
            Prop::Shorthand(ident) if ident.sym == *"fn" => Some(ident.sym.to_string()),
            _ => None,
        }
    })
}

fn is_fn_property_name(name: &PropName) -> bool {
    is_prop_name_with(name, "fn")
}

fn is_prop_name_with(name: &PropName, expected: &str) -> bool {
    match name {
        PropName::Ident(ident) => ident.sym == *expected,
        PropName::Str(str_lit) => str_lit.value == *expected,
        PropName::Computed(computed) => match unwrap_expression(computed.expr.as_ref()) {
            Expr::Lit(Lit::Str(str_lit)) => str_lit.value == *expected,
            _ => false,
        },
        _ => false,
    }
}

fn is_member_prop_with(prop: &MemberProp, expected: &str) -> bool {
    match prop {
        MemberProp::Ident(ident_name) => ident_name.sym == *expected,
        MemberProp::Computed(computed_prop) => match unwrap_expression(computed_prop.expr.as_ref())
        {
            Expr::Lit(Lit::Str(str_lit)) => str_lit.value == *expected,
            _ => false,
        },
        _ => false,
    }
}

fn collect_named_functions_in_module(
    cm: &Lrc<SourceMap>,
    module: &Module,
    target_names: &HashSet<String>,
) -> Vec<ReactFunction> {
    module
        .body
        .iter()
        .flat_map(|item| match item {
            ModuleItem::Stmt(stmt) => collect_named_functions_in_stmt(cm, stmt, target_names),
            ModuleItem::ModuleDecl(module_decl) => match module_decl {
                ModuleDecl::ExportDecl(export_decl) => {
                    collect_named_functions_in_decl(cm, &export_decl.decl, target_names)
                }
                ModuleDecl::ExportDefaultDecl(default_decl) => match &default_decl.decl {
                    DefaultDecl::Fn(fn_expr) => fn_expr
                        .ident
                        .as_ref()
                        .filter(|ident| target_names.contains(ident.sym.as_ref()))
                        .map(|ident| ReactFunction {
                            name: ident.sym.to_string(),
                            kind: ReactFunctionKind::Component,
                            loc: span_to_location(cm, fn_expr.function.span),
                        })
                        .into_iter()
                        .collect(),
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            },
        })
        .collect()
}

fn collect_named_functions_in_stmt(
    cm: &Lrc<SourceMap>,
    stmt: &Stmt,
    target_names: &HashSet<String>,
) -> Vec<ReactFunction> {
    match stmt {
        Stmt::Decl(decl) => collect_named_functions_in_decl(cm, decl, target_names),
        Stmt::Expr(expr_stmt) => {
            collect_named_functions_in_expr(cm, expr_stmt.expr.as_ref(), target_names)
        }
        _ => Vec::new(),
    }
}

fn collect_named_functions_in_expr(
    cm: &Lrc<SourceMap>,
    expr: &Expr,
    target_names: &HashSet<String>,
) -> Vec<ReactFunction> {
    let Expr::Assign(assign_expr) = unwrap_expression(expr) else {
        return Vec::new();
    };
    if assign_expr.op != swc_ecma_ast::AssignOp::Assign {
        return Vec::new();
    }
    let Some(target_name) = assign_target_ident(&assign_expr.left) else {
        return Vec::new();
    };
    if !target_names.contains(target_name.as_str()) {
        return Vec::new();
    }
    let function_span = match unwrap_expression(assign_expr.right.as_ref()) {
        Expr::Fn(fn_expr) => Some(fn_expr.function.span),
        Expr::Arrow(arrow_expr) => Some(arrow_expr.span),
        _ => None,
    };
    let Some(function_span) = function_span else {
        return Vec::new();
    };
    vec![ReactFunction {
        name: target_name,
        kind: ReactFunctionKind::Component,
        loc: span_to_location(cm, function_span),
    }]
}

fn collect_named_functions_in_decl(
    cm: &Lrc<SourceMap>,
    decl: &Decl,
    target_names: &HashSet<String>,
) -> Vec<ReactFunction> {
    match decl {
        Decl::Fn(fn_decl) => {
            if target_names.contains(fn_decl.ident.sym.as_ref()) {
                vec![ReactFunction {
                    name: fn_decl.ident.sym.to_string(),
                    kind: ReactFunctionKind::Component,
                    loc: span_to_location(cm, fn_decl.function.span),
                }]
            } else {
                Vec::new()
            }
        }
        Decl::Var(var_decl) => var_decl
            .decls
            .iter()
            .filter_map(|declarator| {
                let Pat::Ident(binding) = &declarator.name else {
                    return None;
                };
                if !target_names.contains(binding.id.sym.as_ref()) {
                    return None;
                }
                let function_span = match declarator.init.as_deref().map(unwrap_expression) {
                    Some(Expr::Fn(fn_expr)) => Some(fn_expr.function.span),
                    Some(Expr::Arrow(arrow_expr)) => Some(arrow_expr.span),
                    _ => None,
                };
                if function_span.is_none() {
                    return None;
                }
                Some(ReactFunction {
                    name: binding.id.sym.to_string(),
                    kind: ReactFunctionKind::Component,
                    loc: function_span
                        .and_then(|span| span_to_location(cm, span))
                        .or_else(|| span_to_location(cm, binding.id.span)),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn module_has_anonymous_default_export_component_candidate(module: &Module) -> bool {
    module.body.iter().any(|item| {
        let ModuleItem::ModuleDecl(module_decl) = item else {
            return false;
        };
        match module_decl {
            ModuleDecl::ExportDefaultDecl(default_decl) => {
                matches!(&default_decl.decl, DefaultDecl::Fn(fn_expr) if fn_expr.ident.is_none())
            }
            ModuleDecl::ExportDefaultExpr(default_expr) => {
                match unwrap_expression(default_expr.expr.as_ref()) {
                    Expr::Arrow(_) => true,
                    Expr::Fn(fn_expr) => fn_expr.ident.is_none(),
                    _ => false,
                }
            }
            _ => false,
        }
    })
}

fn collect_placeholder_transform_candidate_names_for_script(
    react_functions: &[ReactFunction],
) -> HashSet<String> {
    react_functions
        .iter()
        .filter_map(|function| {
            react_function_kind(function.name.as_str()).map(|_| function.name.clone())
        })
        .collect()
}

fn collect_placeholder_transform_candidate_names_for_module(
    module: &Module,
    react_functions: &[ReactFunction],
) -> HashSet<String> {
    let bindings = collect_top_level_bindings(module);
    let mut transform_candidate_names =
        collect_placeholder_transform_candidate_names_for_script(react_functions);
    transform_candidate_names.extend(collect_default_export_function_names(module, &bindings));
    if module_has_anonymous_default_export_component_candidate(module) {
        transform_candidate_names.insert(DEFAULT_EXPORT_COMPONENT_NAME.to_string());
    }
    transform_candidate_names
}

fn apply_placeholder_compilation_to_module(
    module: &mut Module,
    react_functions: &[ReactFunction],
) -> Vec<String> {
    let should_transform_default_export = module_has_default_export_component_candidate(module);
    let transform_candidate_names =
        collect_placeholder_transform_candidate_names_for_module(module, react_functions);
    if transform_candidate_names.is_empty() && !should_transform_default_export {
        return Vec::new();
    }

    let existing_runtime_callee_name = runtime_memo_callee_name(module);
    let runtime_callee_name = existing_runtime_callee_name
        .as_deref()
        .unwrap_or("_c")
        .to_string();

    let mut transformed_functions = Vec::new();
    for item in module.body.iter_mut() {
        match item {
            ModuleItem::Stmt(stmt) => {
                transformed_functions.extend(apply_placeholder_compilation_to_stmt(
                    stmt,
                    &transform_candidate_names,
                    runtime_callee_name.as_str(),
                ));
            }
            ModuleItem::ModuleDecl(module_decl) => {
                transformed_functions.extend(apply_placeholder_compilation_to_module_decl(
                    module_decl,
                    &transform_candidate_names,
                    runtime_callee_name.as_str(),
                    should_transform_default_export,
                ));
            }
        }
    }

    if !transformed_functions.is_empty() && existing_runtime_callee_name.is_none() {
        module.body.insert(
            0,
            ModuleItem::ModuleDecl(ModuleDecl::Import(make_runtime_import_decl())),
        );
    }

    transformed_functions
}

fn apply_placeholder_compilation_to_script(
    script: &mut Script,
    react_functions: &[ReactFunction],
) -> Vec<String> {
    let transform_candidate_names =
        collect_placeholder_transform_candidate_names_for_script(react_functions);
    if transform_candidate_names.is_empty() {
        return Vec::new();
    }

    let Some(runtime_callee_name) = runtime_memo_callee_name_in_script(script) else {
        return Vec::new();
    };

    let mut transformed_functions = Vec::new();
    for stmt in &mut script.body {
        transformed_functions.extend(apply_placeholder_compilation_to_stmt(
            stmt,
            &transform_candidate_names,
            runtime_callee_name.as_str(),
        ));
    }
    transformed_functions
}

fn apply_placeholder_compilation_to_module_decl(
    module_decl: &mut ModuleDecl,
    react_function_names: &HashSet<String>,
    runtime_callee_name: &str,
    should_transform_default_export: bool,
) -> Vec<String> {
    match module_decl {
        ModuleDecl::ExportDecl(export_decl) => apply_placeholder_compilation_to_decl(
            &mut export_decl.decl,
            react_function_names,
            runtime_callee_name,
        ),
        ModuleDecl::ExportDefaultDecl(default_decl) => match &mut default_decl.decl {
            DefaultDecl::Fn(fn_expr) => {
                if !should_transform_default_export {
                    return Vec::new();
                }
                if inject_placeholder_memo_init_into_function(
                    &mut fn_expr.function,
                    runtime_callee_name,
                ) {
                    vec![fn_expr
                        .ident
                        .as_ref()
                        .map(|ident| ident.sym.to_string())
                        .unwrap_or_else(|| DEFAULT_EXPORT_COMPONENT_NAME.to_string())]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        },
        ModuleDecl::ExportDefaultExpr(default_expr) => {
            if !should_transform_default_export {
                return Vec::new();
            }
            match unwrap_expression_mut(default_expr.expr.as_mut()) {
                Expr::Fn(fn_expr) => {
                    if inject_placeholder_memo_init_into_function(
                        &mut fn_expr.function,
                        runtime_callee_name,
                    ) {
                        vec![fn_expr
                            .ident
                            .as_ref()
                            .map(|ident| ident.sym.to_string())
                            .unwrap_or_else(|| DEFAULT_EXPORT_COMPONENT_NAME.to_string())]
                    } else {
                        Vec::new()
                    }
                }
                Expr::Arrow(arrow_expr) => {
                    if inject_placeholder_memo_init_into_arrow_function(
                        arrow_expr,
                        runtime_callee_name,
                    ) {
                        vec![DEFAULT_EXPORT_COMPONENT_NAME.to_string()]
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

fn apply_placeholder_compilation_to_stmt(
    stmt: &mut Stmt,
    react_function_names: &HashSet<String>,
    runtime_callee_name: &str,
) -> Vec<String> {
    match stmt {
        Stmt::Decl(decl) => {
            apply_placeholder_compilation_to_decl(decl, react_function_names, runtime_callee_name)
        }
        Stmt::Expr(expr_stmt) => apply_placeholder_compilation_to_expr(
            expr_stmt.expr.as_mut(),
            react_function_names,
            runtime_callee_name,
        ),
        _ => Vec::new(),
    }
}

fn apply_placeholder_compilation_to_expr(
    expr: &mut Expr,
    react_function_names: &HashSet<String>,
    runtime_callee_name: &str,
) -> Vec<String> {
    let Expr::Assign(assign_expr) = unwrap_expression_mut(expr) else {
        return Vec::new();
    };
    if assign_expr.op != swc_ecma_ast::AssignOp::Assign {
        return Vec::new();
    }
    let Some(target_name) = assign_target_ident(&assign_expr.left) else {
        return Vec::new();
    };
    if !react_function_names.contains(target_name.as_str()) {
        return Vec::new();
    }
    match unwrap_expression_mut(assign_expr.right.as_mut()) {
        Expr::Fn(fn_expr) => {
            if inject_placeholder_memo_init_into_function(
                &mut fn_expr.function,
                runtime_callee_name,
            ) {
                vec![target_name]
            } else {
                Vec::new()
            }
        }
        Expr::Arrow(arrow_expr) => {
            if inject_placeholder_memo_init_into_arrow_function(arrow_expr, runtime_callee_name) {
                vec![target_name]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn apply_placeholder_compilation_to_decl(
    decl: &mut Decl,
    react_function_names: &HashSet<String>,
    runtime_callee_name: &str,
) -> Vec<String> {
    match decl {
        Decl::Fn(fn_decl) => {
            if react_function_names.contains(fn_decl.ident.sym.as_ref()) {
                if inject_placeholder_memo_init_into_function(
                    &mut fn_decl.function,
                    runtime_callee_name,
                ) {
                    return vec![fn_decl.ident.sym.to_string()];
                }
            }
            Vec::new()
        }
        Decl::Var(var_decl) => {
            let mut transformed_functions = Vec::new();
            for declarator in var_decl.decls.iter_mut() {
                let Pat::Ident(binding) = &declarator.name else {
                    continue;
                };
                if !react_function_names.contains(binding.id.sym.as_ref()) {
                    continue;
                }
                let Some(init) = declarator.init.as_mut() else {
                    continue;
                };
                match unwrap_expression_mut(init.as_mut()) {
                    Expr::Fn(fn_expr) => {
                        if inject_placeholder_memo_init_into_function(
                            &mut fn_expr.function,
                            runtime_callee_name,
                        ) {
                            transformed_functions.push(binding.id.sym.to_string());
                        }
                    }
                    Expr::Arrow(arrow_expr) => {
                        if inject_placeholder_memo_init_into_arrow_function(
                            arrow_expr,
                            runtime_callee_name,
                        ) {
                            transformed_functions.push(binding.id.sym.to_string());
                        }
                    }
                    _ => {}
                }
            }
            transformed_functions
        }
        _ => Vec::new(),
    }
}

fn inject_placeholder_memo_init_into_function(
    function: &mut swc_ecma_ast::Function,
    runtime_callee_name: &str,
) -> bool {
    if function_has_placeholder_memo_init(function, runtime_callee_name) {
        return false;
    }
    let memo_stmt = make_placeholder_memo_stmt(runtime_callee_name);
    match function.body.as_mut() {
        Some(body) => body.stmts.insert(0, memo_stmt),
        None => {
            function.body = Some(BlockStmt {
                span: DUMMY_SP,
                ctxt: Default::default(),
                stmts: vec![memo_stmt],
            });
        }
    }
    true
}

fn inject_placeholder_memo_init_into_arrow_function(
    arrow: &mut swc_ecma_ast::ArrowExpr,
    runtime_callee_name: &str,
) -> bool {
    if arrow_has_placeholder_memo_init(arrow, runtime_callee_name) {
        return false;
    }
    let memo_stmt = make_placeholder_memo_stmt(runtime_callee_name);
    match arrow.body.as_mut() {
        BlockStmtOrExpr::BlockStmt(block) => {
            block.stmts.insert(0, memo_stmt);
        }
        BlockStmtOrExpr::Expr(expr) => {
            let return_stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
                span: DUMMY_SP,
                arg: Some(expr.clone()),
            });
            arrow.body = Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
                span: DUMMY_SP,
                ctxt: Default::default(),
                stmts: vec![memo_stmt, return_stmt],
            }));
        }
    }
    true
}

fn make_placeholder_memo_stmt(runtime_callee_name: &str) -> Stmt {
    Stmt::Decl(Decl::Var(Box::new(VarDecl {
        span: DUMMY_SP,
        ctxt: Default::default(),
        kind: VarDeclKind::Const,
        declare: false,
        decls: vec![VarDeclarator {
            span: DUMMY_SP,
            name: Pat::Ident(BindingIdent::from(Ident::new_no_ctxt("$".into(), DUMMY_SP))),
            init: Some(Box::new(Expr::Call(CallExpr {
                span: DUMMY_SP,
                ctxt: Default::default(),
                callee: Callee::Expr(Box::new(Expr::Ident(Ident::new_no_ctxt(
                    runtime_callee_name.into(),
                    DUMMY_SP,
                )))),
                args: vec![ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Lit(Lit::Num(Number {
                        span: DUMMY_SP,
                        value: 0.0,
                        raw: None,
                    }))),
                }],
                type_args: None,
            }))),
            definite: false,
        }],
    })))
}

fn function_has_placeholder_memo_init(
    function: &swc_ecma_ast::Function,
    runtime_callee_name: &str,
) -> bool {
    function
        .body
        .as_ref()
        .and_then(|body| body.stmts.first())
        .map(|stmt| stmt_is_placeholder_memo_init(stmt, runtime_callee_name))
        .unwrap_or(false)
}

fn arrow_has_placeholder_memo_init(
    arrow: &swc_ecma_ast::ArrowExpr,
    runtime_callee_name: &str,
) -> bool {
    let BlockStmtOrExpr::BlockStmt(block) = arrow.body.as_ref() else {
        return false;
    };
    block
        .stmts
        .first()
        .map(|stmt| stmt_is_placeholder_memo_init(stmt, runtime_callee_name))
        .unwrap_or(false)
}

fn stmt_is_placeholder_memo_init(stmt: &Stmt, runtime_callee_name: &str) -> bool {
    let Stmt::Decl(Decl::Var(var_decl)) = stmt else {
        return false;
    };
    if var_decl.kind != VarDeclKind::Const || var_decl.decls.len() != 1 {
        return false;
    }
    let Some(declarator) = var_decl.decls.first() else {
        return false;
    };
    let Pat::Ident(binding) = &declarator.name else {
        return false;
    };
    if binding.id.sym != *"$" {
        return false;
    }

    let Some(init) = declarator.init.as_ref() else {
        return false;
    };
    let Expr::Call(call_expr) = init.as_ref() else {
        return false;
    };

    let Callee::Expr(callee_expr) = &call_expr.callee else {
        return false;
    };
    let Expr::Ident(callee_ident) = callee_expr.as_ref() else {
        return false;
    };
    if callee_ident.sym != *runtime_callee_name {
        return false;
    }

    if call_expr.args.len() != 1 {
        return false;
    }
    let Some(first_arg) = call_expr.args.first() else {
        return false;
    };
    let Expr::Lit(Lit::Num(number_literal)) = first_arg.expr.as_ref() else {
        return false;
    };
    number_literal.value == 0.0
}

fn make_runtime_import_decl() -> ImportDecl {
    ImportDecl {
        span: DUMMY_SP,
        specifiers: vec![ImportSpecifier::Named(ImportNamedSpecifier {
            span: DUMMY_SP,
            local: Ident::new_no_ctxt("_c".into(), DUMMY_SP),
            imported: Some(ModuleExportName::Ident(Ident::new_no_ctxt(
                "c".into(),
                DUMMY_SP,
            ))),
            is_type_only: false,
        })],
        src: Box::new("react/compiler-runtime".into()),
        type_only: false,
        with: None,
        phase: Default::default(),
    }
}

fn count_runtime_helper_imports(module: &Module) -> usize {
    module
        .body
        .iter()
        .filter_map(|item| match item {
            ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl))
                if import_decl.src.value == *"react/compiler-runtime" =>
            {
                Some(import_decl)
            }
            _ => None,
        })
        .map(|import_decl| {
            import_decl
                .specifiers
                .iter()
                .filter(|specifier| match specifier {
                    ImportSpecifier::Named(named) if !named.is_type_only => named
                        .imported
                        .as_ref()
                        .map(|imported| imported.atom() == &"c")
                        .unwrap_or(named.local.sym == *"c"),
                    _ => false,
                })
                .count()
        })
        .sum()
}

struct RuntimeMemoCalleeScan {
    runtime_namespace_bindings: HashSet<String>,
    runtime_callee_bindings: HashSet<String>,
}

fn runtime_memo_callee_scan_for_module(module: &Module) -> RuntimeMemoCalleeScan {
    let mut runtime_namespace_bindings: HashSet<String> = HashSet::new();
    let mut runtime_callee_bindings: HashSet<String> = HashSet::new();
    for item in &module.body {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) => {
                if import_decl.src.value != *"react/compiler-runtime" {
                    continue;
                }
                collect_runtime_bindings_from_import_decl(
                    import_decl,
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
                if let Decl::Var(var_decl) = &export_decl.decl {
                    for declarator in &var_decl.decls {
                        collect_runtime_bindings_from_script_declarator(
                            declarator,
                            &mut runtime_namespace_bindings,
                            &mut runtime_callee_bindings,
                        );
                    }
                } else if let Decl::Class(class_decl) = &export_decl.decl {
                    collect_runtime_bindings_from_class(
                        &class_decl.class,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                        false,
                    );
                } else if let Decl::Using(using_decl) = &export_decl.decl {
                    for declarator in &using_decl.decls {
                        collect_runtime_bindings_from_script_declarator(
                            declarator,
                            &mut runtime_namespace_bindings,
                            &mut runtime_callee_bindings,
                        );
                    }
                } else if let Decl::TsEnum(ts_enum_decl) = &export_decl.decl {
                    collect_runtime_bindings_from_ts_enum_decl(
                        ts_enum_decl.as_ref(),
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                        false,
                    );
                } else if let Decl::TsModule(ts_module_decl) = &export_decl.decl {
                    collect_runtime_bindings_from_ts_module_decl(
                        ts_module_decl.as_ref(),
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                        false,
                    );
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(default_decl)) => {
                if let DefaultDecl::Class(class_expr) = &default_decl.decl {
                    collect_runtime_bindings_from_class(
                        &class_expr.class,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                        false,
                    );
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(default_expr)) => {
                collect_runtime_bindings_from_script_assignment_expr(
                    default_expr.expr.as_ref(),
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::ModuleDecl(ModuleDecl::TsExportAssignment(export_assignment)) => {
                collect_runtime_bindings_from_script_assignment_expr(
                    export_assignment.expr.as_ref(),
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::ModuleDecl(ModuleDecl::TsImportEquals(import_equals_decl)) => {
                collect_runtime_bindings_from_ts_import_equals_decl(
                    import_equals_decl.as_ref(),
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) => {
                for declarator in &var_decl.decls {
                    collect_runtime_bindings_from_script_declarator(
                        declarator,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                    );
                }
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Class(class_decl))) => {
                collect_runtime_bindings_from_class(
                    &class_decl.class,
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                    false,
                );
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Using(using_decl))) => {
                for declarator in &using_decl.decls {
                    collect_runtime_bindings_from_script_declarator(
                        declarator,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                    );
                }
            }
            ModuleItem::Stmt(Stmt::Expr(expr_stmt)) => {
                collect_runtime_bindings_from_script_assignment_expr(
                    expr_stmt.expr.as_ref(),
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::Stmt(stmt) => collect_runtime_bindings_from_static_block_stmt(
                stmt,
                &mut runtime_namespace_bindings,
                &mut runtime_callee_bindings,
                false,
            ),
            _ => {}
        }
    }
    RuntimeMemoCalleeScan {
        runtime_namespace_bindings,
        runtime_callee_bindings,
    }
}

fn collect_runtime_bindings_from_ts_import_equals_decl(
    import_equals_decl: &swc_ecma_ast::TsImportEqualsDecl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    let binding_name = import_equals_decl.id.sym.to_string();
    clear_runtime_bindings_for_name(
        binding_name.as_str(),
        runtime_namespace_bindings,
        runtime_callee_bindings,
    );
    if import_equals_decl.is_type_only {
        return;
    }
    match &import_equals_decl.module_ref {
        swc_ecma_ast::TsModuleRef::TsExternalModuleRef(module_ref) => {
            if module_ref.expr.value == *"react/compiler-runtime" {
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
        }
        swc_ecma_ast::TsModuleRef::TsEntityName(entity_name) => {
            if let swc_ecma_ast::TsEntityName::Ident(ident) = entity_name {
                let source_name = ident.sym.as_ref();
                if runtime_namespace_bindings.contains(source_name) {
                    runtime_namespace_bindings.insert(binding_name.clone());
                    runtime_callee_bindings.remove(binding_name.as_str());
                } else if runtime_callee_bindings.contains(source_name) {
                    runtime_callee_bindings.insert(binding_name.clone());
                    runtime_namespace_bindings.remove(binding_name.as_str());
                }
                return;
            }
            let source_root_name = ts_entity_name_root_name(entity_name);
            if runtime_namespace_bindings.contains(source_root_name) {
                let source_leaf_name = ts_entity_name_leaf_name(entity_name);
                if source_leaf_name == "c" {
                    runtime_callee_bindings.insert(binding_name.clone());
                    runtime_namespace_bindings.remove(binding_name.as_str());
                }
            }
        }
    }
}

fn collect_runtime_bindings_from_ts_enum_decl(
    ts_enum_decl: &swc_ecma_ast::TsEnumDecl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    for member in &ts_enum_decl.members {
        if let Some(init) = &member.init {
            collect_runtime_bindings_from_expression(
                runtime_initializer_expr(init.as_ref()),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
    }
}

fn collect_runtime_bindings_from_ts_module_decl(
    ts_module_decl: &swc_ecma_ast::TsModuleDecl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    if ts_module_decl.declare {
        return;
    }
    let Some(body) = &ts_module_decl.body else {
        return;
    };
    collect_runtime_bindings_from_ts_namespace_body(
        body,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );
}

fn collect_runtime_bindings_from_ts_namespace_body(
    namespace_body: &swc_ecma_ast::TsNamespaceBody,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    match namespace_body {
        swc_ecma_ast::TsNamespaceBody::TsModuleBlock(module_block) => {
            collect_runtime_bindings_from_ts_module_block(
                module_block,
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        swc_ecma_ast::TsNamespaceBody::TsNamespaceDecl(namespace_decl) => {
            if namespace_decl.declare {
                return;
            }
            collect_runtime_bindings_from_ts_namespace_body(
                namespace_decl.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
    }
}

fn collect_runtime_bindings_from_ts_module_block(
    module_block: &swc_ecma_ast::TsModuleBlock,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let shadowed_bindings = collect_declared_binding_names_from_module_items(&module_block.body);
    with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            for item in &module_block.body {
                collect_runtime_bindings_from_module_item(
                    item,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
        },
    );
}

fn collect_runtime_bindings_from_module_item(
    item: &ModuleItem,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    match item {
        ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) => {
            if import_decl.src.value != *"react/compiler-runtime" {
                return;
            }
            collect_runtime_bindings_from_import_decl(
                import_decl,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
            collect_runtime_bindings_from_decl(
                &export_decl.decl,
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(default_decl)) => {
            if let DefaultDecl::Class(class_expr) = &default_decl.decl {
                collect_runtime_bindings_from_class(
                    &class_expr.class,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
        }
        ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(default_expr)) => {
            collect_runtime_bindings_from_script_assignment_expr(
                default_expr.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        ModuleItem::ModuleDecl(ModuleDecl::TsExportAssignment(export_assignment)) => {
            collect_runtime_bindings_from_script_assignment_expr(
                export_assignment.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        ModuleItem::ModuleDecl(ModuleDecl::TsImportEquals(import_equals_decl)) => {
            collect_runtime_bindings_from_ts_import_equals_decl(
                import_equals_decl.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        ModuleItem::Stmt(stmt) => collect_runtime_bindings_from_static_block_stmt(
            stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        _ => {}
    }
}

fn collect_runtime_bindings_from_decl(
    decl: &Decl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    match decl {
        Decl::Var(var_decl) => {
            if may_be_conditional {
                collect_runtime_bindings_from_var_decl_in_static_block(
                    var_decl,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            } else {
                for declarator in &var_decl.decls {
                    collect_runtime_bindings_from_script_declarator(
                        declarator,
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                    );
                }
            }
        }
        Decl::Using(using_decl) => {
            if may_be_conditional {
                for declarator in &using_decl.decls {
                    if let Some(init) = declarator.init.as_deref() {
                        collect_runtime_bindings_from_expression(
                            runtime_initializer_expr(init),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                }
            } else {
                for declarator in &using_decl.decls {
                    collect_runtime_bindings_from_script_declarator(
                        declarator,
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                    );
                }
            }
        }
        Decl::Class(class_decl) => collect_runtime_bindings_from_class(
            &class_decl.class,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Decl::TsEnum(ts_enum_decl) => collect_runtime_bindings_from_ts_enum_decl(
            ts_enum_decl.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Decl::TsModule(ts_module_decl) => collect_runtime_bindings_from_ts_module_decl(
            ts_module_decl.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        _ => {}
    }
}

fn ts_entity_name_root_name(entity_name: &swc_ecma_ast::TsEntityName) -> &str {
    match entity_name {
        swc_ecma_ast::TsEntityName::Ident(ident) => ident.sym.as_ref(),
        swc_ecma_ast::TsEntityName::TsQualifiedName(qualified_name) => {
            ts_entity_name_root_name(&qualified_name.left)
        }
    }
}

fn ts_entity_name_leaf_name(entity_name: &swc_ecma_ast::TsEntityName) -> &str {
    match entity_name {
        swc_ecma_ast::TsEntityName::Ident(ident) => ident.sym.as_ref(),
        swc_ecma_ast::TsEntityName::TsQualifiedName(qualified_name) => {
            qualified_name.right.sym.as_ref()
        }
    }
}

fn runtime_memo_callee_name(module: &Module) -> Option<String> {
    let runtime_scan = runtime_memo_callee_scan_for_module(module);
    select_runtime_callee_name(&runtime_scan.runtime_callee_bindings)
}

fn collect_runtime_bindings_from_import_decl(
    import_decl: &ImportDecl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    for specifier in &import_decl.specifiers {
        match specifier {
            ImportSpecifier::Named(named) => {
                if named.is_type_only {
                    continue;
                }
                let is_memo_runtime_import = named
                    .imported
                    .as_ref()
                    .map(|imported| imported.atom() == &"c")
                    .unwrap_or(named.local.sym == *"c");
                if is_memo_runtime_import {
                    let binding_name = named.local.sym.to_string();
                    runtime_callee_bindings.insert(binding_name.clone());
                    runtime_namespace_bindings.remove(binding_name.as_str());
                }
            }
            ImportSpecifier::Namespace(namespace) => {
                let binding_name = namespace.local.sym.to_string();
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
            ImportSpecifier::Default(default_import) => {
                let binding_name = default_import.local.sym.to_string();
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
        }
    }
}

fn runtime_memo_callee_scan_for_script(script: &Script) -> RuntimeMemoCalleeScan {
    let mut runtime_namespace_bindings: HashSet<String> = HashSet::new();
    let mut runtime_callee_bindings: HashSet<String> = HashSet::new();
    for stmt in &script.body {
        match stmt {
            Stmt::Decl(Decl::Var(var_decl)) => {
                for declarator in &var_decl.decls {
                    collect_runtime_bindings_from_script_declarator(
                        declarator,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                    );
                }
            }
            Stmt::Decl(Decl::Class(class_decl)) => {
                collect_runtime_bindings_from_class(
                    &class_decl.class,
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                    false,
                );
            }
            Stmt::Decl(Decl::Using(using_decl)) => {
                for declarator in &using_decl.decls {
                    collect_runtime_bindings_from_script_declarator(
                        declarator,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                    );
                }
            }
            Stmt::Expr(expr_stmt) => collect_runtime_bindings_from_script_assignment_expr(
                expr_stmt.expr.as_ref(),
                &mut runtime_namespace_bindings,
                &mut runtime_callee_bindings,
            ),
            stmt => collect_runtime_bindings_from_static_block_stmt(
                stmt,
                &mut runtime_namespace_bindings,
                &mut runtime_callee_bindings,
                false,
            ),
        }
    }
    RuntimeMemoCalleeScan {
        runtime_namespace_bindings,
        runtime_callee_bindings,
    }
}

fn runtime_memo_callee_name_in_script(script: &Script) -> Option<String> {
    let runtime_scan = runtime_memo_callee_scan_for_script(script);
    select_runtime_callee_name(&runtime_scan.runtime_callee_bindings)
}

fn select_runtime_callee_name(runtime_callee_bindings: &HashSet<String>) -> Option<String> {
    runtime_callee_bindings.iter().min().cloned()
}

fn sorted_runtime_callee_candidates(runtime_callee_bindings: &HashSet<String>) -> Vec<String> {
    let mut candidates: Vec<String> = runtime_callee_bindings.iter().cloned().collect();
    candidates.sort();
    candidates
}

fn sorted_runtime_namespace_candidates(
    runtime_namespace_bindings: &HashSet<String>,
) -> Vec<String> {
    let mut candidates: Vec<String> = runtime_namespace_bindings.iter().cloned().collect();
    candidates.sort();
    candidates
}

fn runtime_initializer_expr(expr: &Expr) -> &Expr {
    let expression = unwrap_expression(expr);
    if let Expr::Seq(sequence_expr) = expression {
        if let Some(last_expression) = sequence_expr.exprs.last() {
            return runtime_initializer_expr(last_expression.as_ref());
        }
    }
    expression
}

fn collect_runtime_bindings_from_script_declarator(
    declarator: &VarDeclarator,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    let Some(init) = declarator.init.as_deref().map(runtime_initializer_expr) else {
        return;
    };
    if is_require_runtime_call(init) {
        match &declarator.name {
            Pat::Ident(binding) => {
                let binding_name = binding.id.sym.to_string();
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
            Pat::Object(object_pat) => {
                clear_runtime_bindings_for_object_pat_bindings(
                    object_pat,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                );
                if let Some(callee_name) = extract_runtime_callee_from_object_pat(object_pat) {
                    runtime_callee_bindings.insert(callee_name.clone());
                    runtime_namespace_bindings.remove(callee_name.as_str());
                }
            }
            _ => {}
        }
        return;
    }

    if let Some(namespace_name) = expression_ident(init) {
        if runtime_namespace_bindings.contains(namespace_name.as_str()) {
            if let Pat::Object(object_pat) = &declarator.name {
                clear_runtime_bindings_for_object_pat_bindings(
                    object_pat,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                );
                if let Some(callee_name) = extract_runtime_callee_from_object_pat(object_pat) {
                    runtime_callee_bindings.insert(callee_name.clone());
                    runtime_namespace_bindings.remove(callee_name.as_str());
                }
            } else if let Pat::Ident(binding) = &declarator.name {
                let binding_name = binding.id.sym.to_string();
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
            return;
        }
        if runtime_callee_bindings.contains(namespace_name.as_str()) {
            if let Pat::Ident(binding) = &declarator.name {
                let binding_name = binding.id.sym.to_string();
                runtime_callee_bindings.insert(binding_name.clone());
                runtime_namespace_bindings.remove(binding_name.as_str());
            }
            return;
        }
    }

    if member_expr_is_runtime_namespace_c(init, runtime_namespace_bindings) {
        if let Pat::Ident(binding) = &declarator.name {
            let binding_name = binding.id.sym.to_string();
            runtime_callee_bindings.insert(binding_name.clone());
            runtime_namespace_bindings.remove(binding_name.as_str());
        }
        return;
    }

    collect_runtime_bindings_from_expression(
        init,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        false,
    );
    clear_runtime_bindings_for_pat(
        &declarator.name,
        runtime_namespace_bindings,
        runtime_callee_bindings,
    );
}

fn clear_runtime_bindings_for_pat(
    pattern: &Pat,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    for binding_name in collect_binding_names_from_pat(pattern) {
        runtime_namespace_bindings.remove(binding_name.as_str());
        runtime_callee_bindings.remove(binding_name.as_str());
    }
}

fn clear_runtime_bindings_for_object_pat_bindings(
    object_pat: &swc_ecma_ast::ObjectPat,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    for binding_name in collect_binding_names_from_object_pat(object_pat) {
        runtime_namespace_bindings.remove(binding_name.as_str());
        runtime_callee_bindings.remove(binding_name.as_str());
    }
}

fn collect_runtime_bindings_from_script_assignment_expr(
    expr: &Expr,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    collect_runtime_bindings_from_expression(
        expr,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        false,
    );
}

fn collect_runtime_bindings_from_expression(
    expr: &Expr,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let expression = unwrap_expression(expr);
    if let Expr::Seq(sequence_expr) = expression {
        for expression in &sequence_expr.exprs {
            collect_runtime_bindings_from_expression(
                expression.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        return;
    }
    if let Expr::Cond(cond_expr) = expression {
        collect_runtime_bindings_from_expression(
            cond_expr.test.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        collect_runtime_bindings_from_expression(
            cond_expr.cons.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            true,
        );
        collect_runtime_bindings_from_expression(
            cond_expr.alt.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            true,
        );
        return;
    }
    if let Expr::Bin(binary_expr) = expression {
        let right_may_be_conditional = may_be_conditional || binary_expr.op.may_short_circuit();
        collect_runtime_bindings_from_expression(
            binary_expr.left.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        collect_runtime_bindings_from_expression(
            binary_expr.right.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            right_may_be_conditional,
        );
        return;
    }
    if let Expr::Array(array_literal) = expression {
        for elem in array_literal.elems.iter().flatten() {
            collect_runtime_bindings_from_expression(
                elem.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        return;
    }
    if let Expr::Object(object_literal) = expression {
        for prop_or_spread in &object_literal.props {
            match prop_or_spread {
                PropOrSpread::Prop(prop) => match prop.as_ref() {
                    Prop::KeyValue(key_value) => {
                        collect_runtime_bindings_from_prop_name(
                            &key_value.key,
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                        collect_runtime_bindings_from_expression(
                            key_value.value.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                    Prop::Assign(assign_prop) => {
                        collect_runtime_bindings_from_expression(
                            assign_prop.value.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                    Prop::Getter(getter_prop) => {
                        collect_runtime_bindings_from_prop_name(
                            &getter_prop.key,
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                    Prop::Setter(setter_prop) => {
                        collect_runtime_bindings_from_prop_name(
                            &setter_prop.key,
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                    Prop::Method(method_prop) => {
                        collect_runtime_bindings_from_prop_name(
                            &method_prop.key,
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                    Prop::Shorthand(_) => {}
                },
                PropOrSpread::Spread(spread) => {
                    collect_runtime_bindings_from_expression(
                        spread.expr.as_ref(),
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        may_be_conditional,
                    );
                }
            }
        }
        return;
    }
    if let Expr::Member(member_expr) = expression {
        collect_runtime_bindings_from_expression(
            member_expr.obj.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        if let MemberProp::Computed(computed_prop) = &member_expr.prop {
            collect_runtime_bindings_from_expression(
                computed_prop.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        return;
    }
    if let Expr::SuperProp(super_prop_expr) = expression {
        if let swc_ecma_ast::SuperProp::Computed(computed_prop) = &super_prop_expr.prop {
            collect_runtime_bindings_from_expression(
                computed_prop.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        return;
    }
    if let Expr::JSXElement(jsx_element) = expression {
        collect_runtime_bindings_from_jsx_element(
            jsx_element.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        return;
    }
    if let Expr::JSXFragment(jsx_fragment) = expression {
        collect_runtime_bindings_from_jsx_fragment(
            jsx_fragment,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        return;
    }
    if let Expr::Tpl(template_literal) = expression {
        for expression in &template_literal.exprs {
            collect_runtime_bindings_from_expression(
                expression.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        return;
    }
    if let Expr::TaggedTpl(tagged_template) = expression {
        collect_runtime_bindings_from_expression(
            tagged_template.tag.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        for expression in &tagged_template.tpl.exprs {
            collect_runtime_bindings_from_expression(
                expression.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        return;
    }
    if let Expr::Unary(unary_expr) = expression {
        if unary_expr.op == swc_ecma_ast::UnaryOp::Delete {
            clear_runtime_bindings_for_side_effect_expression(
                expression,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        collect_runtime_bindings_from_expression(
            unary_expr.arg.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        return;
    }
    if let Expr::Await(await_expr) = expression {
        collect_runtime_bindings_from_expression(
            await_expr.arg.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        return;
    }
    if let Expr::Class(class_expr) = expression {
        collect_runtime_bindings_from_class(
            &class_expr.class,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        return;
    }
    if let Expr::New(new_expr) = expression {
        collect_runtime_bindings_from_expression(
            new_expr.callee.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        if let Some(args) = &new_expr.args {
            for arg in args {
                collect_runtime_bindings_from_expression(
                    arg.expr.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
        }
        return;
    }
    if let Expr::OptChain(opt_chain_expr) = expression {
        match opt_chain_expr.base.as_ref() {
            swc_ecma_ast::OptChainBase::Member(member_expr) => {
                collect_runtime_bindings_from_expression(
                    member_expr.obj.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                if let MemberProp::Computed(computed_prop) = &member_expr.prop {
                    collect_runtime_bindings_from_expression(
                        computed_prop.expr.as_ref(),
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        true,
                    );
                }
            }
            swc_ecma_ast::OptChainBase::Call(opt_call) => {
                collect_runtime_bindings_from_expression(
                    opt_call.callee.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                for arg in &opt_call.args {
                    collect_runtime_bindings_from_expression(
                        arg.expr.as_ref(),
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        true,
                    );
                }
            }
        }
        return;
    }
    if let Expr::Yield(yield_expr) = expression {
        if let Some(arg) = &yield_expr.arg {
            collect_runtime_bindings_from_expression(
                arg.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        return;
    }
    if let Expr::Call(call_expr) = expression {
        if let Callee::Expr(callee_expr) = &call_expr.callee {
            collect_runtime_bindings_from_expression(
                callee_expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        for arg in &call_expr.args {
            collect_runtime_bindings_from_expression(
                arg.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        return;
    }
    let Expr::Assign(assign_expr) = expression else {
        clear_runtime_bindings_for_side_effect_expression(
            expression,
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
        return;
    };
    if may_be_conditional {
        clear_runtime_bindings_for_assign_target(
            &assign_expr.left,
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
        collect_runtime_bindings_from_expression(
            assign_expr.right.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            true,
        );
        return;
    }
    if assign_expr.op != swc_ecma_ast::AssignOp::Assign {
        let rhs_may_be_conditional = assign_expr.op.may_short_circuit();
        clear_runtime_bindings_for_assign_target(
            &assign_expr.left,
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
        collect_runtime_bindings_from_expression(
            assign_expr.right.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            rhs_may_be_conditional,
        );
        return;
    }
    let target_ident = assign_target_ident(&assign_expr.left);
    let target_object_pat = assign_target_object_pat(&assign_expr.left);
    let right = runtime_initializer_expr(assign_expr.right.as_ref());
    if is_require_runtime_call(right) {
        clear_runtime_bindings_for_assign_target(
            &assign_expr.left,
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
        if let Some(target_name) = target_ident.as_ref() {
            runtime_namespace_bindings.insert(target_name.clone());
            runtime_callee_bindings.remove(target_name.as_str());
        } else if let Some(object_pat) = target_object_pat {
            if let Some(callee_name) = extract_runtime_callee_from_object_pat(object_pat) {
                runtime_callee_bindings.insert(callee_name.clone());
                runtime_namespace_bindings.remove(callee_name.as_str());
            }
        }
        return;
    }
    if let Some(namespace_name) = expression_ident(right) {
        if runtime_namespace_bindings.contains(namespace_name.as_str()) {
            clear_runtime_bindings_for_assign_target(
                &assign_expr.left,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
            if let Some(target_name) = target_ident.as_ref() {
                runtime_namespace_bindings.insert(target_name.clone());
                runtime_callee_bindings.remove(target_name.as_str());
            } else if let Some(object_pat) = target_object_pat {
                if let Some(callee_name) = extract_runtime_callee_from_object_pat(object_pat) {
                    runtime_callee_bindings.insert(callee_name.clone());
                    runtime_namespace_bindings.remove(callee_name.as_str());
                }
            }
            return;
        }
        if runtime_callee_bindings.contains(namespace_name.as_str()) {
            clear_runtime_bindings_for_assign_target(
                &assign_expr.left,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
            if let Some(target_name) = target_ident.as_ref() {
                runtime_callee_bindings.insert(target_name.clone());
                runtime_namespace_bindings.remove(target_name.as_str());
            }
            return;
        }
    }
    if member_expr_is_runtime_namespace_c(right, runtime_namespace_bindings) {
        clear_runtime_bindings_for_assign_target(
            &assign_expr.left,
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
        if let Some(target_name) = target_ident.as_ref() {
            runtime_callee_bindings.insert(target_name.clone());
            runtime_namespace_bindings.remove(target_name.as_str());
        }
        return;
    }

    clear_runtime_bindings_for_assign_target(
        &assign_expr.left,
        runtime_namespace_bindings,
        runtime_callee_bindings,
    );
}

fn collect_runtime_bindings_from_jsx_element(
    jsx_element: &swc_ecma_ast::JSXElement,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    for attr_or_spread in &jsx_element.opening.attrs {
        match attr_or_spread {
            swc_ecma_ast::JSXAttrOrSpread::SpreadElement(spread) => {
                collect_runtime_bindings_from_expression(
                    spread.expr.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
            swc_ecma_ast::JSXAttrOrSpread::JSXAttr(attr) => {
                if let Some(value) = &attr.value {
                    collect_runtime_bindings_from_jsx_attr_value(
                        value,
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        may_be_conditional,
                    );
                }
            }
        }
    }
    for child in &jsx_element.children {
        collect_runtime_bindings_from_jsx_child(
            child,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}

fn collect_runtime_bindings_from_jsx_fragment(
    jsx_fragment: &swc_ecma_ast::JSXFragment,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    for child in &jsx_fragment.children {
        collect_runtime_bindings_from_jsx_child(
            child,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}

fn collect_runtime_bindings_from_jsx_child(
    child: &swc_ecma_ast::JSXElementChild,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    match child {
        swc_ecma_ast::JSXElementChild::JSXText(_) => {}
        swc_ecma_ast::JSXElementChild::JSXExprContainer(container) => {
            collect_runtime_bindings_from_jsx_expr_container(
                container,
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        swc_ecma_ast::JSXElementChild::JSXSpreadChild(spread_child) => {
            collect_runtime_bindings_from_expression(
                spread_child.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        swc_ecma_ast::JSXElementChild::JSXElement(element) => {
            collect_runtime_bindings_from_jsx_element(
                element.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        swc_ecma_ast::JSXElementChild::JSXFragment(fragment) => {
            collect_runtime_bindings_from_jsx_fragment(
                fragment,
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
    }
}

fn collect_runtime_bindings_from_jsx_attr_value(
    value: &swc_ecma_ast::JSXAttrValue,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    match value {
        swc_ecma_ast::JSXAttrValue::Lit(_) => {}
        swc_ecma_ast::JSXAttrValue::JSXExprContainer(container) => {
            collect_runtime_bindings_from_jsx_expr_container(
                container,
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        swc_ecma_ast::JSXAttrValue::JSXElement(element) => {
            collect_runtime_bindings_from_jsx_element(
                element.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        swc_ecma_ast::JSXAttrValue::JSXFragment(fragment) => {
            collect_runtime_bindings_from_jsx_fragment(
                fragment,
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
    }
}

fn collect_runtime_bindings_from_jsx_expr_container(
    container: &swc_ecma_ast::JSXExprContainer,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    if let swc_ecma_ast::JSXExpr::Expr(expr) = &container.expr {
        collect_runtime_bindings_from_expression(
            expr.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}

fn clear_runtime_bindings_for_assign_target(
    target: &AssignTarget,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    clear_runtime_namespace_bindings_for_assign_target_member(
        target,
        runtime_namespace_bindings,
        runtime_callee_bindings,
    );
    for binding_name in collect_binding_names_from_assign_target(target) {
        clear_runtime_bindings_for_name(
            binding_name.as_str(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
    }
}

fn clear_runtime_bindings_for_name(
    binding_name: &str,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    runtime_namespace_bindings.remove(binding_name);
    runtime_callee_bindings.remove(binding_name);
}

fn clear_runtime_bindings_for_side_effect_expression(
    expr: &Expr,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    match unwrap_expression(expr) {
        Expr::Update(update_expr) => clear_runtime_bindings_for_side_effect_target_expression(
            update_expr.arg.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
        ),
        Expr::Unary(unary_expr) if unary_expr.op == swc_ecma_ast::UnaryOp::Delete => {
            clear_runtime_bindings_for_side_effect_target_expression(
                unary_expr.arg.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        _ => {}
    }
}

fn clear_runtime_bindings_for_side_effect_target_expression(
    target_expr: &Expr,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    if let Some(binding_name) = expression_ident(target_expr) {
        clear_runtime_bindings_for_name(
            binding_name.as_str(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
        return;
    }
    match unwrap_expression(target_expr) {
        Expr::Member(member_expr) => {
            clear_runtime_namespace_binding_for_member_expr(
                member_expr,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        Expr::OptChain(opt_chain_expr) => {
            clear_runtime_namespace_binding_for_opt_chain_expr(
                opt_chain_expr,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        _ => {}
    }
}

fn clear_runtime_namespace_bindings_for_assign_target_member(
    target: &AssignTarget,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    let AssignTarget::Simple(simple_target) = target else {
        return;
    };
    clear_runtime_namespace_binding_for_simple_assign_target(
        simple_target,
        runtime_namespace_bindings,
        runtime_callee_bindings,
    );
}

fn clear_runtime_namespace_binding_for_simple_assign_target(
    target: &SimpleAssignTarget,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    match target {
        SimpleAssignTarget::Member(member_expr) => clear_runtime_namespace_binding_for_member_expr(
            member_expr,
            runtime_namespace_bindings,
            runtime_callee_bindings,
        ),
        SimpleAssignTarget::Paren(paren_expr) => {
            clear_runtime_bindings_for_side_effect_target_expression(
                paren_expr.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        SimpleAssignTarget::TsAs(ts_as_expr) => {
            clear_runtime_bindings_for_side_effect_target_expression(
                ts_as_expr.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        SimpleAssignTarget::TsSatisfies(ts_satisfies_expr) => {
            clear_runtime_bindings_for_side_effect_target_expression(
                ts_satisfies_expr.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        SimpleAssignTarget::TsNonNull(ts_non_null_expr) => {
            clear_runtime_bindings_for_side_effect_target_expression(
                ts_non_null_expr.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        SimpleAssignTarget::TsTypeAssertion(ts_type_assertion) => {
            clear_runtime_bindings_for_side_effect_target_expression(
                ts_type_assertion.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        SimpleAssignTarget::TsInstantiation(ts_instantiation) => {
            clear_runtime_bindings_for_side_effect_target_expression(
                ts_instantiation.expr.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
        }
        _ => {}
    }
}

fn clear_runtime_namespace_binding_for_member_expr(
    member_expr: &swc_ecma_ast::MemberExpr,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    if !member_prop_may_target_runtime_c(&member_expr.prop) {
        return;
    }
    let Some(object_name) = expression_ident(member_expr.obj.as_ref()) else {
        return;
    };
    if runtime_namespace_bindings.contains(object_name.as_str()) {
        clear_runtime_bindings_for_name(
            object_name.as_str(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
        runtime_callee_bindings.clear();
    }
}

fn clear_runtime_namespace_binding_for_opt_chain_expr(
    opt_chain_expr: &swc_ecma_ast::OptChainExpr,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    if let swc_ecma_ast::OptChainBase::Member(member_expr) = opt_chain_expr.base.as_ref() {
        clear_runtime_namespace_binding_for_member_expr(
            member_expr,
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
    }
}

fn member_prop_may_target_runtime_c(prop: &MemberProp) -> bool {
    match prop {
        MemberProp::Ident(ident_name) => ident_name.sym == *"c",
        MemberProp::Computed(computed_prop) => match unwrap_expression(computed_prop.expr.as_ref()) {
            Expr::Lit(Lit::Str(str_lit)) => str_lit.value == *"c",
            Expr::Lit(_) => false,
            _ => true,
        },
        MemberProp::PrivateName(_) => false,
    }
}

fn collect_runtime_bindings_from_class(
    class: &swc_ecma_ast::Class,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    collect_runtime_bindings_from_decorators(
        &class.decorators,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );
    if let Some(super_class) = &class.super_class {
        collect_runtime_bindings_from_expression(
            super_class.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
    for class_member in &class.body {
        match class_member {
            swc_ecma_ast::ClassMember::Method(class_method) => {
                collect_runtime_bindings_from_class_key(
                    &class_method.key,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
            swc_ecma_ast::ClassMember::ClassProp(class_prop) => {
                collect_runtime_bindings_from_class_key(
                    &class_prop.key,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                collect_runtime_bindings_from_decorators(
                    &class_prop.decorators,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                if class_prop.is_static {
                    if let Some(value) = &class_prop.value {
                        collect_runtime_bindings_from_expression(
                            value.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                }
            }
            swc_ecma_ast::ClassMember::PrivateProp(private_prop) => {
                collect_runtime_bindings_from_decorators(
                    &private_prop.decorators,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                if private_prop.is_static {
                    if let Some(value) = &private_prop.value {
                        collect_runtime_bindings_from_expression(
                            value.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                }
            }
            swc_ecma_ast::ClassMember::StaticBlock(static_block) => {
                collect_runtime_bindings_from_static_block(
                    static_block,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
            swc_ecma_ast::ClassMember::AutoAccessor(auto_accessor) => {
                if let swc_ecma_ast::Key::Public(prop_name) = &auto_accessor.key {
                    collect_runtime_bindings_from_class_key(
                        prop_name,
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        may_be_conditional,
                    );
                }
                collect_runtime_bindings_from_decorators(
                    &auto_accessor.decorators,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                if auto_accessor.is_static {
                    if let Some(value) = &auto_accessor.value {
                        collect_runtime_bindings_from_expression(
                            value.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_runtime_bindings_from_decorators(
    decorators: &[swc_ecma_ast::Decorator],
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    for decorator in decorators {
        collect_runtime_bindings_from_expression(
            decorator.expr.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}

fn collect_runtime_bindings_from_class_key(
    key: &PropName,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    collect_runtime_bindings_from_prop_name(
        key,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );
}

fn collect_runtime_bindings_from_prop_name(
    key: &PropName,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    if let PropName::Computed(computed_key) = key {
        collect_runtime_bindings_from_expression(
            computed_key.expr.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}

fn collect_runtime_bindings_from_static_block(
    static_block: &swc_ecma_ast::StaticBlock,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    collect_runtime_bindings_from_static_block_stmts(
        &static_block.body.stmts,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );
}

fn with_shadowed_runtime_bindings<F>(
    shadowed_bindings: &[String],
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    scan: F,
) where
    F: FnOnce(&mut HashSet<String>, &mut HashSet<String>),
{
    let mut snapshots: Vec<(String, bool, bool)> = Vec::new();
    for binding_name in shadowed_bindings {
        let was_runtime_namespace = runtime_namespace_bindings.remove(binding_name.as_str());
        let was_runtime_callee = runtime_callee_bindings.remove(binding_name.as_str());
        snapshots.push((
            binding_name.clone(),
            was_runtime_namespace,
            was_runtime_callee,
        ));
    }

    scan(runtime_namespace_bindings, runtime_callee_bindings);

    for (binding_name, was_runtime_namespace, was_runtime_callee) in snapshots {
        runtime_namespace_bindings.remove(binding_name.as_str());
        runtime_callee_bindings.remove(binding_name.as_str());
        if was_runtime_namespace {
            runtime_namespace_bindings.insert(binding_name.clone());
        }
        if was_runtime_callee {
            runtime_callee_bindings.insert(binding_name);
        }
    }
}

fn collect_declared_binding_names_from_stmts(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in stmts {
        collect_declared_binding_names_from_stmt(stmt, &mut names);
    }
    names.sort();
    names.dedup();
    names
}

fn collect_declared_binding_names_from_stmt(stmt: &Stmt, names: &mut Vec<String>) {
    if let Stmt::Decl(decl) = stmt {
        collect_declared_binding_names_from_decl(decl, names);
    }
}

fn collect_declared_binding_names_from_decl(decl: &Decl, names: &mut Vec<String>) {
    match decl {
        Decl::Var(var_decl) => {
            for declarator in &var_decl.decls {
                collect_binding_names_from_pat_into(&declarator.name, names);
            }
        }
        Decl::Using(using_decl) => {
            for declarator in &using_decl.decls {
                collect_binding_names_from_pat_into(&declarator.name, names);
            }
        }
        Decl::Class(class_decl) => names.push(class_decl.ident.sym.to_string()),
        Decl::Fn(fn_decl) => names.push(fn_decl.ident.sym.to_string()),
        Decl::TsEnum(enum_decl) => names.push(enum_decl.id.sym.to_string()),
        Decl::TsModule(module_decl) => match &module_decl.id {
            swc_ecma_ast::TsModuleName::Ident(ident) => names.push(ident.sym.to_string()),
            swc_ecma_ast::TsModuleName::Str(_) => {}
        },
        _ => {}
    }
}

fn collect_declared_binding_names_from_module_items(items: &[ModuleItem]) -> Vec<String> {
    let mut names = Vec::new();
    for item in items {
        collect_declared_binding_names_from_module_item(item, &mut names);
    }
    names.sort();
    names.dedup();
    names
}

fn collect_declared_binding_names_from_module_item(item: &ModuleItem, names: &mut Vec<String>) {
    match item {
        ModuleItem::Stmt(stmt) => collect_declared_binding_names_from_stmt(stmt, names),
        ModuleItem::ModuleDecl(module_decl) => match module_decl {
            ModuleDecl::Import(import_decl) => {
                for specifier in &import_decl.specifiers {
                    match specifier {
                        ImportSpecifier::Named(named) => names.push(named.local.sym.to_string()),
                        ImportSpecifier::Default(default_import) => {
                            names.push(default_import.local.sym.to_string())
                        }
                        ImportSpecifier::Namespace(namespace_import) => {
                            names.push(namespace_import.local.sym.to_string())
                        }
                    }
                }
            }
            ModuleDecl::ExportDecl(export_decl) => {
                collect_declared_binding_names_from_decl(&export_decl.decl, names)
            }
            ModuleDecl::ExportDefaultDecl(default_decl) => match &default_decl.decl {
                DefaultDecl::Class(class_expr) => {
                    if let Some(ident) = &class_expr.ident {
                        names.push(ident.sym.to_string());
                    }
                }
                DefaultDecl::Fn(fn_expr) => {
                    if let Some(ident) = &fn_expr.ident {
                        names.push(ident.sym.to_string());
                    }
                }
                _ => {}
            },
            ModuleDecl::TsImportEquals(import_equals_decl) => {
                names.push(import_equals_decl.id.sym.to_string())
            }
            _ => {}
        },
    }
}

fn collect_runtime_bindings_from_static_block_stmts(
    stmts: &[Stmt],
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let shadowed_bindings = collect_declared_binding_names_from_stmts(stmts);
    with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            for stmt in stmts {
                collect_runtime_bindings_from_static_block_stmt(
                    stmt,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
        },
    );
}

fn for_init_declared_binding_names(for_stmt: &swc_ecma_ast::ForStmt) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(var_decl)) = &for_stmt.init {
        for declarator in &var_decl.decls {
            collect_binding_names_from_pat_into(&declarator.name, &mut names);
        }
    }
    names.sort();
    names.dedup();
    names
}

fn for_head_declared_binding_names(for_head: &swc_ecma_ast::ForHead) -> Vec<String> {
    let mut names = Vec::new();
    match for_head {
        swc_ecma_ast::ForHead::VarDecl(var_decl) => {
            for declarator in &var_decl.decls {
                collect_binding_names_from_pat_into(&declarator.name, &mut names);
            }
        }
        swc_ecma_ast::ForHead::UsingDecl(using_decl) => {
            for declarator in &using_decl.decls {
                collect_binding_names_from_pat_into(&declarator.name, &mut names);
            }
        }
        swc_ecma_ast::ForHead::Pat(_) => {}
    }
    names.sort();
    names.dedup();
    names
}

fn catch_param_declared_binding_names(catch_clause: &swc_ecma_ast::CatchClause) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(param) = &catch_clause.param {
        collect_binding_names_from_pat_into(param, &mut names);
    }
    names.sort();
    names.dedup();
    names
}

fn collect_runtime_bindings_from_for_stmt(
    for_stmt: &swc_ecma_ast::ForStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let shadowed_bindings = for_init_declared_binding_names(for_stmt);
    with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            if let Some(init) = &for_stmt.init {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(var_decl) => {
                        collect_runtime_bindings_from_var_decl_in_static_block(
                            var_decl,
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                    swc_ecma_ast::VarDeclOrExpr::Expr(expr) => {
                        collect_runtime_bindings_from_expression(
                            expr.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                }
            }
            if let Some(test) = &for_stmt.test {
                collect_runtime_bindings_from_expression(
                    test.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
            if let Some(update) = &for_stmt.update {
                collect_runtime_bindings_from_expression(
                    update.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    true,
                );
            }
            collect_runtime_bindings_from_static_block_stmt(
                for_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
        },
    );
}

fn collect_runtime_bindings_from_for_in_stmt(
    for_in_stmt: &swc_ecma_ast::ForInStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let shadowed_bindings = for_head_declared_binding_names(&for_in_stmt.left);
    with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            collect_runtime_bindings_from_expression(
                for_in_stmt.right.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            clear_runtime_bindings_for_for_head(
                &for_in_stmt.left,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
            collect_runtime_bindings_from_static_block_stmt(
                for_in_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
        },
    );
}

fn collect_runtime_bindings_from_for_of_stmt(
    for_of_stmt: &swc_ecma_ast::ForOfStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let shadowed_bindings = for_head_declared_binding_names(&for_of_stmt.left);
    with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            collect_runtime_bindings_from_expression(
                for_of_stmt.right.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            clear_runtime_bindings_for_for_head(
                &for_of_stmt.left,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
            collect_runtime_bindings_from_static_block_stmt(
                for_of_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
        },
    );
}

fn collect_runtime_bindings_from_try_stmt(
    try_stmt: &swc_ecma_ast::TryStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    collect_runtime_bindings_from_static_block_stmts(
        &try_stmt.block.stmts,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );
    if let Some(handler) = &try_stmt.handler {
        let shadowed_bindings = catch_param_declared_binding_names(handler);
        with_shadowed_runtime_bindings(
            &shadowed_bindings,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            |runtime_namespace_bindings, runtime_callee_bindings| {
                collect_runtime_bindings_from_static_block_stmts(
                    &handler.body.stmts,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    true,
                );
            },
        );
    }
    if let Some(finalizer) = &try_stmt.finalizer {
        collect_runtime_bindings_from_static_block_stmts(
            &finalizer.stmts,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}

fn collect_runtime_bindings_from_switch_stmt(
    switch_stmt: &swc_ecma_ast::SwitchStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    collect_runtime_bindings_from_expression(
        switch_stmt.discriminant.as_ref(),
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );

    let mut shadowed_bindings = Vec::new();
    for case in &switch_stmt.cases {
        for stmt in &case.cons {
            collect_declared_binding_names_from_stmt(stmt, &mut shadowed_bindings);
        }
    }
    shadowed_bindings.sort();
    shadowed_bindings.dedup();

    with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            for case in &switch_stmt.cases {
                if let Some(test) = &case.test {
                    collect_runtime_bindings_from_expression(
                        test.as_ref(),
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        true,
                    );
                }
                for stmt in &case.cons {
                    collect_runtime_bindings_from_static_block_stmt(
                        stmt,
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        true,
                    );
                }
            }
        },
    );
}

fn collect_runtime_bindings_from_static_block_stmt(
    stmt: &Stmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    match stmt {
        Stmt::Expr(expr_stmt) => collect_runtime_bindings_from_expression(
            expr_stmt.expr.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Decl(Decl::Var(var_decl)) => collect_runtime_bindings_from_var_decl_in_static_block(
            var_decl,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Decl(Decl::Using(using_decl)) => {
            for declarator in &using_decl.decls {
                if let Some(init) = declarator.init.as_deref() {
                    collect_runtime_bindings_from_expression(
                        runtime_initializer_expr(init),
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        may_be_conditional,
                    );
                }
            }
        }
        Stmt::Decl(Decl::Class(class_decl)) => collect_runtime_bindings_from_class(
            &class_decl.class,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Decl(Decl::TsEnum(ts_enum_decl)) => collect_runtime_bindings_from_ts_enum_decl(
            ts_enum_decl.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Decl(Decl::TsModule(ts_module_decl)) => collect_runtime_bindings_from_ts_module_decl(
            ts_module_decl.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::If(if_stmt) => {
            collect_runtime_bindings_from_expression(
                if_stmt.test.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            collect_runtime_bindings_from_static_block_stmt(
                if_stmt.cons.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
            if let Some(alt) = &if_stmt.alt {
                collect_runtime_bindings_from_static_block_stmt(
                    alt.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    true,
                );
            }
        }
        Stmt::Block(block_stmt) => {
            collect_runtime_bindings_from_static_block_stmts(
                &block_stmt.stmts,
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        Stmt::For(for_stmt) => collect_runtime_bindings_from_for_stmt(
            for_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::ForIn(for_in_stmt) => collect_runtime_bindings_from_for_in_stmt(
            for_in_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::ForOf(for_of_stmt) => collect_runtime_bindings_from_for_of_stmt(
            for_of_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::While(while_stmt) => {
            collect_runtime_bindings_from_expression(
                while_stmt.test.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            collect_runtime_bindings_from_static_block_stmt(
                while_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
        }
        Stmt::DoWhile(do_while_stmt) => {
            collect_runtime_bindings_from_static_block_stmt(
                do_while_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            collect_runtime_bindings_from_expression(
                do_while_stmt.test.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        Stmt::Switch(switch_stmt) => collect_runtime_bindings_from_switch_stmt(
            switch_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Try(try_stmt) => collect_runtime_bindings_from_try_stmt(
            try_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Throw(throw_stmt) => collect_runtime_bindings_from_expression(
            throw_stmt.arg.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Labeled(labeled_stmt) => collect_runtime_bindings_from_static_block_stmt(
            labeled_stmt.body.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::With(with_stmt) => {
            collect_runtime_bindings_from_expression(
                with_stmt.obj.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            collect_runtime_bindings_from_static_block_stmt(
                with_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        _ => {}
    }
}

fn collect_runtime_bindings_from_var_decl_in_static_block(
    var_decl: &VarDecl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    for declarator in &var_decl.decls {
        if let Some(init) = declarator.init.as_deref() {
            collect_runtime_bindings_from_expression(
                runtime_initializer_expr(init),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
    }
}

fn clear_runtime_bindings_for_for_head(
    for_head: &swc_ecma_ast::ForHead,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    match for_head {
        swc_ecma_ast::ForHead::Pat(pattern) => clear_runtime_bindings_for_pat(
            pattern.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
        ),
        swc_ecma_ast::ForHead::VarDecl(_) | swc_ecma_ast::ForHead::UsingDecl(_) => {}
    }
}

fn collect_binding_names_from_pat(pattern: &Pat) -> Vec<String> {
    let mut names = Vec::new();
    collect_binding_names_from_pat_into(pattern, &mut names);
    names
}

fn collect_binding_names_from_assign_target(target: &AssignTarget) -> Vec<String> {
    let mut names = Vec::new();
    match target {
        AssignTarget::Simple(simple) => {
            if let Some(name) = simple_assign_target_ident(simple) {
                names.push(name);
            }
        }
        AssignTarget::Pat(pattern) => {
            collect_binding_names_from_assign_target_pat_into(pattern, &mut names)
        }
    }
    names
}

fn collect_binding_names_from_assign_target_pat_into(
    pattern: &swc_ecma_ast::AssignTargetPat,
    names: &mut Vec<String>,
) {
    match pattern {
        swc_ecma_ast::AssignTargetPat::Object(object_pat) => {
            collect_binding_names_from_object_pat_into(object_pat, names)
        }
        swc_ecma_ast::AssignTargetPat::Array(array_pat) => {
            for elem in array_pat.elems.iter().flatten() {
                collect_binding_names_from_pat_into(elem, names);
            }
        }
        swc_ecma_ast::AssignTargetPat::Invalid(_) => {}
    }
}

fn collect_binding_names_from_object_pat(object_pat: &swc_ecma_ast::ObjectPat) -> Vec<String> {
    let mut names = Vec::new();
    collect_binding_names_from_object_pat_into(object_pat, &mut names);
    names
}

fn collect_binding_names_from_object_pat_into(
    object_pat: &swc_ecma_ast::ObjectPat,
    names: &mut Vec<String>,
) {
    for prop in &object_pat.props {
        match prop {
            swc_ecma_ast::ObjectPatProp::Assign(assign) => {
                names.push(assign.key.id.sym.to_string());
            }
            swc_ecma_ast::ObjectPatProp::KeyValue(key_value) => {
                collect_binding_names_from_pat_into(key_value.value.as_ref(), names);
            }
            swc_ecma_ast::ObjectPatProp::Rest(rest) => {
                collect_binding_names_from_pat_into(rest.arg.as_ref(), names);
            }
        }
    }
}

fn collect_binding_names_from_pat_into(pattern: &Pat, names: &mut Vec<String>) {
    match pattern {
        Pat::Ident(binding) => names.push(binding.id.sym.to_string()),
        Pat::Array(array_pat) => {
            for elem in array_pat.elems.iter().flatten() {
                collect_binding_names_from_pat_into(elem, names);
            }
        }
        Pat::Object(object_pat) => collect_binding_names_from_object_pat_into(object_pat, names),
        Pat::Assign(assign_pat) => {
            collect_binding_names_from_pat_into(assign_pat.left.as_ref(), names)
        }
        Pat::Rest(rest_pat) => collect_binding_names_from_pat_into(rest_pat.arg.as_ref(), names),
        _ => {}
    }
}

fn assign_target_object_pat(target: &AssignTarget) -> Option<&swc_ecma_ast::ObjectPat> {
    let AssignTarget::Pat(pattern) = target else {
        return None;
    };
    let swc_ecma_ast::AssignTargetPat::Object(object_pat) = pattern else {
        return None;
    };
    Some(object_pat)
}

fn extract_runtime_callee_from_object_pat(object_pat: &swc_ecma_ast::ObjectPat) -> Option<String> {
    for prop in &object_pat.props {
        match prop {
            swc_ecma_ast::ObjectPatProp::KeyValue(key_value) => {
                if !is_prop_name_with(&key_value.key, "c") {
                    continue;
                }
                if let Pat::Ident(binding) = key_value.value.as_ref() {
                    return Some(binding.id.sym.to_string());
                }
            }
            swc_ecma_ast::ObjectPatProp::Assign(assign) if assign.key.id.sym == *"c" => {
                return Some(assign.key.id.sym.to_string());
            }
            _ => {}
        }
    }
    None
}

fn is_require_runtime_call(expr: &Expr) -> bool {
    let Expr::Call(call_expr) = unwrap_expression(expr) else {
        return false;
    };
    is_require_runtime_call_expr(call_expr)
}

fn is_require_runtime_call_expr(call_expr: &CallExpr) -> bool {
    let Callee::Expr(callee_expr) = &call_expr.callee else {
        return false;
    };
    let Expr::Ident(callee_ident) = unwrap_expression(callee_expr.as_ref()) else {
        return false;
    };
    if callee_ident.sym != *"require" {
        return false;
    }
    if call_expr.args.len() != 1 {
        return false;
    }
    let Some(first_arg) = call_expr.args.first() else {
        return false;
    };
    match unwrap_expression(first_arg.expr.as_ref()) {
        Expr::Lit(Lit::Str(str_lit)) => str_lit.value == *"react/compiler-runtime",
        _ => false,
    }
}

fn member_expr_is_runtime_namespace_c(expr: &Expr, runtime_namespaces: &HashSet<String>) -> bool {
    let Expr::Member(member_expr) = unwrap_expression(expr) else {
        return false;
    };
    if !is_member_prop_with(&member_expr.prop, "c") {
        return false;
    }
    if is_require_runtime_call(member_expr.obj.as_ref()) {
        return true;
    }
    let Some(namespace_name) = expression_ident(member_expr.obj.as_ref()) else {
        return false;
    };
    runtime_namespaces.contains(namespace_name.as_str())
}

#[cfg(test)]
mod tests {
    use super::{
        compile, render_react_functions_debug, CompilerError, CompilerOptions, InputDialect,
    };

    #[test]
    fn parses_javascript_source() {
        let output = compile(
            "export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.statement_count_after_transform, 2);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_before_transform,
            0
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_after_transform,
            1
        );
        assert!(output.metadata.placeholder_runtime_helper_import_added);
        assert!(!output.metadata.placeholder_runtime_callee_reused);
        assert!(output.metadata.placeholder_runtime_callee_generated);
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec!["Component".to_string()]
        );
        assert!(output
            .metadata
            .placeholder_transform_skipped_functions
            .is_empty());
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_status, "transformed");
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 1);
        assert_eq!(output.metadata.detected_hook_function_count, 0);
        assert_eq!(output.metadata.react_functions.len(), 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert_eq!(
            output.metadata.placeholder_transformed_functions,
            vec!["Component".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("_c")
        );
        assert!(output
            .metadata
            .placeholder_runtime_callee_name_before_transform
            .is_none());
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["_c".to_string()]
        );
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 1);
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            0
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates_before_transform
            .is_empty());
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidate_count_before_transform,
            0
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert_eq!(output.metadata.placeholder_runtime_namespace_candidate_count, 0);
        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn preserves_leading_pragma_comments() {
        let output = compile(
            "// @enableFlowSuppressions\nexport function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("@enableFlowSuppressions"));
    }

    #[test]
    fn detects_fixture_entrypoint_function_when_name_is_not_react_like() {
        let output = compile(
            "function component(){ return 1; } export const FIXTURE_ENTRYPOINT = { fn: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert!(!output.code.contains("react/compiler-runtime"));
    }

    #[test]
    fn detects_fixture_entrypoint_function_referenced_via_alias() {
        let output = compile(
            "function component(){ return 1; } const alias = component; export const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert!(!output.code.contains("react/compiler-runtime"));
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_computed_object_key() {
        let output = compile(
            "function component(){ return 1; } export const FIXTURE_ENTRYPOINT = { ['fn']: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_shorthand_object_key() {
        let output = compile(
            "function component(){ return 1; } const fn = component; export const FIXTURE_ENTRYPOINT = { fn, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_referenced_via_assigned_alias() {
        let output = compile(
            "function component(){ return 1; } let alias; alias = component; export const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_computed_member_assignment() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT['fn'] = component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_assigned_object_literal() {
        let output = compile(
            "function component(){ return 1; } let FIXTURE_ENTRYPOINT; FIXTURE_ENTRYPOINT = { fn: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_member_assignment() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT.fn = component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_assigned_function_expression() {
        let output = compile(
            "let alias; alias = () => 1; export const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "alias");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert!(!output.code.contains("react/compiler-runtime"));
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_assigned_function_expression_with_matching_name() {
        let output = compile(
            "let component; component = () => 1; export const FIXTURE_ENTRYPOINT = { fn: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_import_alias() {
        let output = compile(
            "import { c as cache } from 'react/compiler-runtime'; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("c as cache"));
        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_before_transform,
            1
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_after_transform,
            1
        );
        assert!(!output.metadata.placeholder_runtime_helper_import_added);
        assert!(output.metadata.placeholder_runtime_callee_reused);
        assert!(!output.metadata.placeholder_runtime_callee_generated);
    }

    #[test]
    fn reuses_existing_runtime_cache_require_member_alias_in_module() {
        let output = compile(
            "const cache = require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_require_namespace_alias_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            1
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidate_count_before_transform,
            1
        );
        assert_eq!(output.metadata.placeholder_runtime_namespace_candidate_count, 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_mutated_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_update_expression_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_compound_assignment_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x += 1; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_logical_assignment_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x &&= unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_conditionally_reassigned_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x = unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_conditionally_deleted_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime.x); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.x); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_update_expression_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime.x++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_compound_assignment_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_logical_assignment_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_conditionally_deleted_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime['x']); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.['x']); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_conditionally_reassigned_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] = unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_update_expression_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime['x']++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_compound_assignment_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_logical_assignment_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_mutated_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_deleted_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime['x']; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_update_expression_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x']++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_compound_assignment_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] += 1; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_logical_assignment_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] &&= unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_namespace_import_alias_in_module() {
        let output = compile(
            "import * as runtime from 'react/compiler-runtime'; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_default_import_alias_in_module() {
        let output = compile(
            "import runtime from 'react/compiler-runtime'; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_namespace_destructure_assignment_alias_in_module() {
        let output = compile(
            "import * as runtime from 'react/compiler-runtime'; let cache; ({ c: cache } = runtime); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_require_destructure_assignment_alias_in_module() {
        let output = compile(
            "let runtime; runtime = require('react/compiler-runtime'); let cache; ({ c: cache } = runtime); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_require_shorthand_destructure_alias_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c } = runtime; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = c(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_exported_require_member_alias_in_module() {
        let output = compile(
            "export const cache = require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_exported_require_namespace_alias_in_module() {
        let output = compile(
            "export const runtime = require('react/compiler-runtime'); export const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_import_alias_via_identifier_alias_in_module() {
        let output = compile(
            "import { c as cache0 } from 'react/compiler-runtime'; const cache = cache0; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_shadowed_runtime_callee_without_treating_it_as_namespace_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c: runtime } = require('react/compiler-runtime'); const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = runtime(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_require_alias_via_assignment_in_module() {
        let output = compile(
            "const { c: cache0 } = require('react/compiler-runtime'); let cache; cache = cache0; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_sequence_assignment_in_module() {
        let output = compile(
            "let cache; (cache = require('react/compiler-runtime').c, sideEffect()); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_sequence_assignment_rhs_in_module() {
        let output = compile(
            "let cache; cache = (sideEffect(), require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_call_argument_assignment_in_module() {
        let output = compile(
            "let cache; sideEffect(cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_new_expression_argument_assignment_in_module() {
        let output = compile(
            "let cache; new SideEffect(cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_tagged_template_assignment_in_module() {
        let output = compile(
            "let cache; tag`${cache = require('react/compiler-runtime').c}`; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_unary_assignment_in_module() {
        let output = compile(
            "let cache; void (cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_await_assignment_in_module() {
        let output = compile(
            "let cache; await (cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_for_init_assignment_in_module() {
        let output = compile(
            "let cache; for (cache = require('react/compiler-runtime').c; false;) {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_using_declaration_in_module() {
        let output = compile(
            "using cache = require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_jsx_child_assignment_in_module() {
        let output = compile(
            "let cache; <div>{cache = require('react/compiler-runtime').c}</div>; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_jsx_attr_assignment_in_module() {
        let output = compile(
            "let cache; <div data-cache={cache = require('react/compiler-runtime').c} />; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_block_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; { let cache; cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_class_static_block_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { let cache; cache = unknown; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_catch_param_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; try { throw 0; } catch (cache) { cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_switch_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; switch (value) { case 0: let cache; cache = unknown; break; default: break; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_computed_key_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { [cache = require('react/compiler-runtime').c](){} } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_static_block_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { cache = require('react/compiler-runtime').c; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_static_super_computed_assignment_in_module() {
        let output = compile(
            "let cache; class Base {} class RuntimeCarrier extends Base { static { super[cache = require('react/compiler-runtime').c]; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_super_class_assignment_in_module() {
        let output = compile(
            "let cache; class Base {} class RuntimeCarrier extends (cache = require('react/compiler-runtime').c, Base) {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_decorator_assignment_in_module() {
        let output = compile(
            "let cache; @((cache = require('react/compiler-runtime').c)) class RuntimeCarrier {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_static_for_init_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { for (cache = require('react/compiler-runtime').c; false;) {} } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_static_do_while_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { do { cache = require('react/compiler-runtime').c; } while (false); } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_when_class_static_for_in_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; class RuntimeCarrier { static { for (let cache in source) { break; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_class_static_for_of_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = []; class RuntimeCarrier { static { for (const cache of source) { break; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_nested_class_static_block_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { class Nested { static { cache = require('react/compiler-runtime').c; } } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_export_default_class_static_block_assignment_in_module() {
        let output = compile(
            "let cache; export default class RuntimeCarrier { static { cache = require('react/compiler-runtime').c; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_export_default_expression_assignment_in_module() {
        let output = compile(
            "let cache; export default (cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_export_assignment_in_module() {
        let output = compile(
            "let cache; export = (cache = require('react/compiler-runtime').c); function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_import_equals_runtime_namespace() {
        let output = compile(
            "import Runtime = require('react/compiler-runtime'); const cache = Runtime.c; function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_import_equals_qualified_callee_alias() {
        let output = compile(
            "import Runtime = require('react/compiler-runtime'); import cache = Runtime.c; function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_enum_member_assignment_in_module() {
        let output = compile(
            "let cache; enum RuntimeCarrier { Value = (cache = require('react/compiler-runtime').c) } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_module_assignment_in_module() {
        let output = compile(
            "let cache; namespace RuntimeCarrier { export const value = (cache = require('react/compiler-runtime').c); } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_ts_module_declares_shadowed_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; namespace RuntimeCarrier { export let cache; cache = unknown; } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_assigned_in_class_static_block(
    ) {
        let output = compile(
            "let cache; class RuntimeCarrier { static { if (cond) { cache = require('react/compiler-runtime').c; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_ts_import_equals_runtime_alias_is_reassigned() {
        let output = compile(
            "import Runtime = require('react/compiler-runtime'); let cache = Runtime.c; cache = unknown; function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_assigned_via_optional_call()
    {
        let output = compile(
            "let cache; maybe?.(cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_assigned_via_optional_computed_member(
    ) {
        let output = compile(
            "let cache; maybe?.[cache = require('react/compiler-runtime').c]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_object_literal_assignment_in_module() {
        let output = compile(
            "let cache; const payload = {value: (cache = require('react/compiler-runtime').c)}; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_object_literal_computed_key_assignment_in_module() {
        let output = compile(
            "let cache; const payload = {[cache = require('react/compiler-runtime').c]: 1}; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_non_short_circuit_binary_assignment_in_module() {
        let output = compile(
            "let cache; const value = 1 + (cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_template_literal_assignment_in_module() {
        let output = compile(
            "let cache; const value = `${cache = require('react/compiler-runtime').c}`; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_computed_member_assignment_in_module() {
        let output = compile(
            "let cache; const value = source[cache = require('react/compiler-runtime').c]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_sequence_declarator_in_module() {
        let output = compile(
            "const cache = (sideEffect(), require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache = unknown; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_uses_compound_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache += 1; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_uses_logical_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache &&= unknown; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_uses_update_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache++; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_overwritten_by_object_pattern_assignment()
    {
        let output = compile(
            "let cache; ({ c: cache } = require('react/compiler-runtime')); ({ x: cache } = source); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_sequence() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; (cache = unknown, sideEffect()); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cond && (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c = unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime.c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.['c']); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime.c++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime[prop]); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.[prop]); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime[prop]++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime[prop] += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime[prop] &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime['c']); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] = unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime['c']++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_call_argument() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; sideEffect(cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_new_expression_argument() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; new SideEffect(cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_tagged_template() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; tag`${cache = unknown}`; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_unary_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; void (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_top_level_await() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; await (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_in_top_level_while(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; while (cond) { cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_in_top_level_switch(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; switch (value) { case 0: cache = unknown; break; default: break; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_mutated_via_top_level_for_in_pattern() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; for (cache in source) { break; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_top_level_jsx_child() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; <div>{cache = unknown}</div>; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_after_top_level_using_declaration(
    ) {
        let output = compile(
            "using cache = require('react/compiler-runtime').c; cache = unknown; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_block_decl_shadows_alias_mutation_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; { let cache; cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_class_static_block_decl_shadows_alias_mutation_in_module()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { let cache; cache = unknown; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_catch_param_shadows_alias_mutation_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; try { throw 0; } catch (cache) { cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_via_optional_call(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; maybe?.(cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_via_optional_computed_member(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; maybe?.[cache = unknown]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_computed_key() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { [cache = unknown](){} } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_static_block() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { cache = unknown; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_static_super_computed()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class Base {} class RuntimeCarrier extends Base { static { super[cache = unknown]; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_super_class_expression()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class Base {} class RuntimeCarrier extends (cache = unknown, Base) {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_decorator_expression()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; @((cache = unknown)) class RuntimeCarrier {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_static_for_init() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { for (cache = unknown; false;) {} } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_mutated_via_class_static_for_in_pattern() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; class RuntimeCarrier { static { for (cache in source) { break; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_export_default_class_static_block(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; export default class RuntimeCarrier { static { cache = unknown; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_in_class_static_while(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { while (cond) { cache = unknown; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_nested_class_static_block() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { class Nested { static { cache = unknown; } } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_export_default_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; export default (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_ts_export_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; export = (cache = unknown); function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_ts_enum_member() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; enum RuntimeCarrier { Value = (cache = unknown) } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_ts_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; namespace RuntimeCarrier { export const value = (cache = unknown); } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_in_class_static_block(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { if (cond) { cache = unknown; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_object_literal() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const payload = {value: (cache = unknown)}; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_object_literal_computed_key()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const payload = {[cache = unknown]: 1}; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_non_short_circuit_binary() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = 1 + (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_template_literal() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = `${cache = unknown}`; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_computed_member() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = source[cache = unknown]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_reassigned() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_reassigned() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime.c; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_deleted_via_optional_chain()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.c; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_deleted_via_optional_chain(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.['c']; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_optional_chain_computed_member_may_target_c(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; delete runtime?.[prop]; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime['c']; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_update_expression() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_update_expression()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c']++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_compound_assignment() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c += 1; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_compound_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] += 1; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_logical_assignment() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c &&= unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_logical_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] &&= unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; runtime[prop] = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; delete runtime[prop]; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_uses_update_expression(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; runtime[prop]++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn transforms_script_component_with_runtime_require_destructure_alias() {
        let output = compile(
            "const { c: cache } = require('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert_eq!(
            output.metadata.placeholder_transformed_functions,
            vec!["Component".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_shorthand_destructure_alias() {
        let output = compile(
            "const { c } = require('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = c(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_chain() {
        let output = compile(
            "const { c: cache0 } = require('react/compiler-runtime'); const cache = cache0; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_using_shadowed_runtime_callee_alias() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c: runtime } = require('react/compiler-runtime'); const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = runtime(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_assignment_alias_chain() {
        let output = compile(
            "const { c: cache0 } = require('react/compiler-runtime'); let cache; cache = cache0; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_sequence_assignment() {
        let output = compile(
            "let cache; (cache = require('react/compiler-runtime').c, sideEffect()); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_sequence_assignment_rhs() {
        let output = compile(
            "let cache; cache = (sideEffect(), require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_call_argument_assignment() {
        let output = compile(
            "let cache; sideEffect(cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_new_expression_argument_assignment() {
        let output = compile(
            "let cache; new SideEffect(cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_tagged_template_assignment() {
        let output = compile(
            "let cache; tag`${cache = require('react/compiler-runtime').c}`; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_unary_assignment() {
        let output = compile(
            "let cache; void (cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_computed_key_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { [cache = require('react/compiler-runtime').c](){} } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_static_block_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { cache = require('react/compiler-runtime').c; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_static_super_computed_assignment()
    {
        let output = compile(
            "let cache; class Base {} class RuntimeCarrier extends Base { static { super[cache = require('react/compiler-runtime').c]; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_static_for_init_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { for (cache = require('react/compiler-runtime').c; false;) {} } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_top_level_for_init_assignment() {
        let output = compile(
            "let cache; for (cache = require('react/compiler-runtime').c; false;) {} function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_top_level_using_declaration() {
        let output = compile(
            "using cache = require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_ts_enum_member_assignment() {
        let output = compile(
            "let cache; enum RuntimeCarrier { Value = (cache = require('react/compiler-runtime').c) } function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_ts_module_assignment() {
        let output = compile(
            "let cache; namespace RuntimeCarrier { export const value = (cache = require('react/compiler-runtime').c); } function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_top_level_jsx_child_assignment() {
        let output = compile(
            "let cache; <div>{cache = require('react/compiler-runtime').c}</div>; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_top_level_block_decl_shadows_runtime_alias_mutation() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; { let cache; cache = unknown; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_class_static_block_decl_shadows_runtime_alias_mutation() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { let cache; cache = unknown; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_top_level_catch_param_shadows_runtime_alias_mutation() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; try { throw 0; } catch (cache) { cache = unknown; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_top_level_switch_decl_shadows_runtime_alias_mutation() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; switch (value) { case 0: let cache; cache = unknown; break; default: break; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_static_do_while_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { do { cache = require('react/compiler-runtime').c; } while (false); } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_class_static_for_in_decl_shadows_alias() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; class RuntimeCarrier { static { for (let cache in source) { break; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_nested_class_static_block_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { class Nested { static { cache = require('react/compiler-runtime').c; } } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_assigned_in_class_static_block(
    ) {
        let output = compile(
            "let cache; class RuntimeCarrier { static { if (cond) { cache = require('react/compiler-runtime').c; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 1);
        assert_eq!(output.metadata.detected_hook_function_count, 0);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_assigned_via_optional_call(
    ) {
        let output = compile(
            "let cache; maybe?.(cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_assigned_via_optional_computed_member(
    ) {
        let output = compile(
            "let cache; maybe?.[cache = require('react/compiler-runtime').c]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_object_literal_assignment() {
        let output = compile(
            "let cache; const payload = {value: (cache = require('react/compiler-runtime').c)}; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_object_literal_computed_key_assignment()
    {
        let output = compile(
            "let cache; const payload = {[cache = require('react/compiler-runtime').c]: 1}; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_non_short_circuit_binary_assignment() {
        let output = compile(
            "let cache; const value = 1 + (cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_template_literal_assignment() {
        let output = compile(
            "let cache; const value = `${cache = require('react/compiler-runtime').c}`; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_computed_member_assignment() {
        let output = compile(
            "let cache; const value = source[cache = require('react/compiler-runtime').c]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_sequence_declarator() {
        let output = compile(
            "const cache = (sideEffect(), require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache = unknown; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_sequence() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; (cache = unknown, sideEffect()); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cond && (cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c = unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime.c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.['c']); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime.c++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime[prop]); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.[prop]); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime[prop]++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime[prop] += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime[prop] &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime['c']); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] = unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime['c']++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_call_argument() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; sideEffect(cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_new_expression_argument(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; new SideEffect(cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_tagged_template() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; tag`${cache = unknown}`; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_unary_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; void (cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_via_optional_call(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; maybe?.(cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_via_optional_computed_member(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; maybe?.[cache = unknown]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_class_computed_key()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { [cache = unknown](){} } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_class_static_block()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { cache = unknown; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_class_static_super_computed(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class Base {} class RuntimeCarrier extends Base { static { super[cache = unknown]; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_class_static_for_init(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { for (cache = unknown; false;) {} } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_in_top_level_while(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; while (cond) { cache = unknown; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_in_top_level_switch(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; switch (value) { case 0: cache = unknown; break; default: break; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_mutated_via_top_level_for_in_pattern(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; for (cache in source) { break; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_top_level_jsx_child()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; <div>{cache = unknown}</div>; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_after_top_level_using_declaration(
    ) {
        let output = compile(
            "using cache = require('react/compiler-runtime').c; cache = unknown; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_ts_enum_member() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; enum RuntimeCarrier { Value = (cache = unknown) } function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_ts_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; namespace RuntimeCarrier { export const value = (cache = unknown); } function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_mutated_via_class_static_for_in_pattern(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; class RuntimeCarrier { static { for (cache in source) { break; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_in_class_static_block(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { if (cond) { cache = unknown; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_in_class_static_while(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { while (cond) { cache = unknown; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_nested_class_static_block(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { class Nested { static { cache = unknown; } } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_object_literal() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const payload = {value: (cache = unknown)}; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_object_literal_computed_key(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const payload = {[cache = unknown]: 1}; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_non_short_circuit_binary(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = 1 + (cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_template_literal() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = `${cache = unknown}`; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_computed_member() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = source[cache = unknown]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_reassigned() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_reassigned()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; runtime[prop] = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_is_deleted(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; delete runtime[prop]; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_uses_update_expression(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; runtime[prop]++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_uses_compound_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache += 1; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_uses_logical_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache &&= unknown; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_deleted() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; delete cache; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime.c; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_deleted_via_optional_chain(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.c; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_deleted_via_optional_chain(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.['c']; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_optional_chain_computed_member_may_target_c(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; delete runtime?.[prop]; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime['c']; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_update_expression() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_update_expression(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c']++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_compound_assignment()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c += 1; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_compound_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] += 1; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_logical_assignment()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c &&= unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_logical_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] &&= unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_uses_update_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache++; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_overwritten_by_object_pattern_assignment(
    ) {
        let output = compile(
            "let cache; ({ c: cache } = require('react/compiler-runtime')); ({ x: cache } = source); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_member_alias() {
        let output = compile(
            "const cache = require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_member_alias() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_mutated() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x = unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime.x); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.x); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime.x++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime['x']); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.['x']); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] = unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime['x']++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_deleted_via_optional_chain_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.x; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_deleted_via_optional_chain_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.['x']; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_deleted_via_optional_chain() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.x; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_deleted_via_optional_chain(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.['x']; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_update_expression() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_compound_assignment() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x += 1; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_logical_assignment() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x &&= unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_mutated() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime['x']; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_update_expression(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x']++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_compound_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] += 1; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_logical_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] &&= unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_with_assigned_runtime_namespace_member_alias() {
        let output = compile(
            "let runtime; runtime = require('react/compiler-runtime'); let cache; cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_destructure_alias() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c: cache } = runtime; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_assigned_runtime_destructure_alias() {
        let output = compile(
            "let runtime; runtime = require('react/compiler-runtime'); let cache; ({ c: cache } = runtime); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn does_not_transform_script_component_without_runtime_binding() {
        let output = compile(
            "function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec!["Component".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_transform_skipped_functions,
            vec!["Component".to_string()]
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_status,
            "blocked_missing_runtime_callee"
        );
        assert!(!output.metadata.placeholder_runtime_callee_reused);
        assert!(!output.metadata.placeholder_runtime_callee_generated);
        assert!(output.metadata.placeholder_transformed_functions.is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_name_before_transform
            .is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates
            .is_empty());
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 0);
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            0
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates_before_transform
            .is_empty());
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidate_count_before_transform,
            0
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert_eq!(output.metadata.placeholder_runtime_namespace_candidate_count, 0);
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_export_default_named_component() {
        let output = compile(
            "export default function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_export_default_anonymous_component() {
        let output = compile(
            "export default function (){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 1);
        assert_eq!(output.metadata.detected_hook_function_count, 0);
        assert_eq!(
            output.metadata.react_functions[0].name,
            super::DEFAULT_EXPORT_COMPONENT_NAME
        );
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec![super::DEFAULT_EXPORT_COMPONENT_NAME.to_string()]
        );
        assert!(output
            .metadata
            .placeholder_transform_skipped_functions
            .is_empty());
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_status, "transformed");
        assert!(!output.metadata.placeholder_runtime_callee_reused);
        assert!(output.metadata.placeholder_runtime_callee_generated);
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            0
        );
        assert_eq!(output.metadata.placeholder_runtime_namespace_candidate_count, 0);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidate_count_before_transform,
            0
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_export_default_arrow_component() {
        let output = compile(
            "export default () => <div />;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(
            output.metadata.react_functions[0].name,
            super::DEFAULT_EXPORT_COMPONENT_NAME
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_parenthesized_export_default_arrow_component() {
        let output = compile(
            "export default (() => <div />);",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(
            output.metadata.react_functions[0].name,
            super::DEFAULT_EXPORT_COMPONENT_NAME
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_typescript_asserted_export_default_arrow_component() {
        let output = compile(
            "export default ((() => <div />) as any);",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.tsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(
            output.metadata.react_functions[0].name,
            super::DEFAULT_EXPORT_COMPONENT_NAME
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_named_default_export_component_even_when_name_is_not_react_like() {
        let output = compile(
            "export default function component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_default_export_identifier() {
        let output = compile(
            "function component(){ return <div />; } export default component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_aliased_default_export_identifier() {
        let output = compile(
            "function component(){ return <div />; } const alias = component; export default alias;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_assigned_default_export_identifier() {
        let output = compile(
            "function component(){ return <div />; } let alias; alias = component; export default alias;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_assigned_function_expression_default_export_identifier() {
        let output = compile(
            "let alias; alias = () => <div />; export default alias;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "alias");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_assigned_function_expression_default_export_with_component_name(
    ) {
        let output = compile(
            "let component; component = () => <div />; export default component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_multi_aliased_default_export_identifier() {
        let output = compile(
            "function component(){ return <div />; } const aliasA = component; const aliasB = aliasA; export default aliasB;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_named_default_export_specifier() {
        let output = compile(
            "function component(){ return <div />; } export {component as default};",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_react_like_assignment_expression_in_module() {
        let output = compile(
            "let Component; Component = () => <div />; export {Component};",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
        assert_eq!(output.metadata.statement_count, 3);
        assert_eq!(output.metadata.statement_count_after_transform, 4);
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_assigned_function_expression_named_default_export_specifier(
    ) {
        let output = compile(
            "let component; component = () => <div />; export {component as default};",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_aliased_named_default_export_specifier() {
        let output = compile(
            "function component(){ return <div />; } const alias = component; export {alias as default};",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_duplicate_existing_placeholder_memo_stmt() {
        let output = compile(
            "import { c as _c } from 'react/compiler-runtime'; export function Component(){ const $ = _c(0); return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.code.matches("const $ = _c(0);").count(), 1);
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn parses_typescript_source() {
        let output = compile(
            "export function id<T>(value: T): T { return value; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.tsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.statement_count_after_transform, 1);
        assert_eq!(output.metadata.detected_react_functions, 0);
        assert!(output.metadata.react_functions.is_empty());
        assert!(!output.code.contains("react/compiler-runtime"));
    }

    #[test]
    fn detects_hook_by_name() {
        let output = compile(
            "function useValue() { return 1; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 0);
        assert_eq!(output.metadata.detected_hook_function_count, 1);
        assert_eq!(output.metadata.react_functions[0].name, "useValue");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Hook
        );
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec!["useValue".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_transform_skipped_functions,
            vec!["useValue".to_string()]
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_status,
            "blocked_missing_runtime_callee"
        );
        assert!(!output.code.contains("react/compiler-runtime"));
    }

    #[test]
    fn transforms_hook_in_module_mode_and_reports_hook_counts() {
        let output = compile(
            "export function useValue() { return 1; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: true,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 0);
        assert_eq!(output.metadata.detected_hook_function_count, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec!["useValue".to_string()]
        );
        assert!(output
            .metadata
            .placeholder_transform_skipped_functions
            .is_empty());
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_status, "transformed");
        assert!(!output.metadata.placeholder_runtime_callee_reused);
        assert!(output.metadata.placeholder_runtime_callee_generated);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_before_transform,
            0
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_after_transform,
            1
        );
        assert!(output.metadata.placeholder_runtime_helper_import_added);
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            0
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn detects_react_function_from_variable_declarator() {
        let output = compile(
            "const Component = () => <div />;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
        assert!(!output.code.contains("react/compiler-runtime"));
    }

    #[test]
    fn detects_react_function_from_assignment_expression() {
        let output = compile(
            "let Component; Component = () => <div />;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 2);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_react_function_from_parenthesized_variable_declarator() {
        let output = compile(
            "const Component = (() => <div />);",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
    }

    #[test]
    fn detects_react_function_from_typescript_asserted_variable_declarator() {
        let output = compile(
            "const Component = (() => <div />) as any;",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.tsx".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
    }

    #[test]
    fn parses_flow_dialect_when_file_uses_only_javascript_syntax() {
        let output = compile(
            "/* @flow */\nfunction Component() { return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::Flow,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("flow dialect should support plain JavaScript source");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
    }

    #[test]
    fn rejects_flow_until_ported() {
        let err = compile(
            "/* @flow */ function Component(props: {x: number}) { return props.x; }",
            &CompilerOptions {
                dialect: InputDialect::Flow,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect_err("flow is not implemented yet");

        match err {
            CompilerError::UnsupportedFlowSyntax { location } => {
                assert!(location.is_some());
            }
            other => panic!("expected unsupported flow syntax error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failures_include_source_location_metadata() {
        let err = compile(
            "const = 1;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { location, .. } => {
                assert!(location.is_some());
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_unexpected_token() {
        let err = compile(
            "const = 1;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "unexpected_token");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_unterminated_syntax() {
        let err = compile(
            "const value = /foo",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "unterminated_syntax");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_expected_token() {
        let err = compile(
            "const value = ;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "expected_token");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_unexpected_eof() {
        let err = compile(
            "function Component(",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "unexpected_eof");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_invalid_syntax() {
        let err = compile(
            "const \\u00ZZ = 1;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "invalid_syntax");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn react_function_metadata_order_is_stable() {
        let source = r#"
          function useAlpha() { return 1; }
          function Component() { return <div />; }
          function useBeta() { return 2; }
        "#;
        let options = CompilerOptions {
            dialect: InputDialect::JavaScript,
            filename: "fixture.jsx".to_string(),
            is_module: false,
            ..CompilerOptions::default()
        };

        let first = compile(source, &options)
            .expect("expected first compile to succeed")
            .metadata
            .react_functions;
        let second = compile(source, &options)
            .expect("expected second compile to succeed")
            .metadata
            .react_functions;

        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["useAlpha", "Component", "useBeta"]
        );
    }

    #[test]
    fn renders_react_function_debug_snapshot() {
        let output = compile(
            "function Component() { return <div />; } function useThing() { return 1; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                is_module: false,
                apply_placeholder_transforms: false,
            },
        )
        .expect("expected compile to succeed");

        let debug = render_react_functions_debug(&output.metadata);
        assert!(debug.contains("ReactiveFunctionsDebug v0"));
        assert!(debug.contains("statement_count=2"));
        assert!(debug.contains("statement_count_after_transform=2"));
        assert!(debug.contains("placeholder_runtime_helper_import_count_before_transform=0"));
        assert!(debug.contains("placeholder_runtime_helper_import_count_after_transform=0"));
        assert!(debug.contains("placeholder_runtime_helper_import_added=false"));
        assert!(debug.contains("placeholder_runtime_callee_reused=false"));
        assert!(debug.contains("placeholder_runtime_callee_generated=false"));
        assert!(debug.contains("placeholder_transform_candidates=Component,useThing"));
        assert!(debug.contains("placeholder_transform_skipped_functions=Component,useThing"));
        assert!(debug.contains("placeholder_transform_candidate_count=2"));
        assert!(debug.contains("placeholder_transform_skipped_count=2"));
        assert!(debug.contains("placeholder_transform_candidate_component_count=1"));
        assert!(debug.contains("placeholder_transform_candidate_hook_count=1"));
        assert!(debug.contains("placeholder_transform_transformed_component_count=0"));
        assert!(debug.contains("placeholder_transform_transformed_hook_count=0"));
        assert!(debug.contains("placeholder_transform_skipped_component_count=1"));
        assert!(debug.contains("placeholder_transform_skipped_hook_count=1"));
        assert!(debug.contains("placeholder_transform_status=disabled"));
        assert!(debug.contains("detected_component_function_count=1"));
        assert!(debug.contains("detected_hook_function_count=1"));
        assert!(debug.contains("detected_react_functions=2"));
        assert!(debug.contains("placeholder_transforms_applied=0"));
        assert!(debug.contains("placeholder_transformed_functions="));
        assert!(debug.contains("placeholder_runtime_callee_name=none"));
        assert!(debug.contains("placeholder_runtime_callee_name_before_transform=none"));
        assert!(debug.contains("placeholder_runtime_callee_candidates="));
        assert!(debug.contains("placeholder_runtime_callee_candidate_count=0"));
        assert!(debug.contains("placeholder_runtime_callee_candidates_before_transform="));
        assert!(debug.contains(
            "placeholder_runtime_callee_candidate_count_before_transform=0"
        ));
        assert!(debug.contains("placeholder_runtime_namespace_candidates="));
        assert!(debug.contains("placeholder_runtime_namespace_candidate_count=0"));
        assert!(debug.contains("placeholder_runtime_namespace_candidates_before_transform="));
        assert!(debug.contains(
            "placeholder_runtime_namespace_candidate_count_before_transform=0"
        ));
        assert!(debug.contains("name=Component kind=Component"));
        assert!(debug.contains("name=useThing kind=Hook"));
    }

    #[test]
    fn renders_react_function_debug_snapshot_with_transform_state() {
        let output = compile(
            "export function Component() { return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                is_module: true,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected compile to succeed");

        let debug = render_react_functions_debug(&output.metadata);
        assert!(debug.contains("ReactiveFunctionsDebug v0"));
        assert!(debug.contains("statement_count=1"));
        assert!(debug.contains("statement_count_after_transform=2"));
        assert!(debug.contains("placeholder_runtime_helper_import_count_before_transform=0"));
        assert!(debug.contains("placeholder_runtime_helper_import_count_after_transform=1"));
        assert!(debug.contains("placeholder_runtime_helper_import_added=true"));
        assert!(debug.contains("placeholder_runtime_callee_reused=false"));
        assert!(debug.contains("placeholder_runtime_callee_generated=true"));
        assert!(debug.contains("placeholder_transform_candidates=Component"));
        assert!(debug.contains("placeholder_transform_skipped_functions="));
        assert!(debug.contains("placeholder_transform_candidate_count=1"));
        assert!(debug.contains("placeholder_transform_skipped_count=0"));
        assert!(debug.contains("placeholder_transform_candidate_component_count=1"));
        assert!(debug.contains("placeholder_transform_candidate_hook_count=0"));
        assert!(debug.contains("placeholder_transform_transformed_component_count=1"));
        assert!(debug.contains("placeholder_transform_transformed_hook_count=0"));
        assert!(debug.contains("placeholder_transform_skipped_component_count=0"));
        assert!(debug.contains("placeholder_transform_skipped_hook_count=0"));
        assert!(debug.contains("placeholder_transform_status=transformed"));
        assert!(debug.contains("detected_component_function_count=1"));
        assert!(debug.contains("detected_hook_function_count=0"));
        assert!(debug.contains("placeholder_transforms_applied=1"));
        assert!(debug.contains("placeholder_transformed_functions=Component"));
        assert!(debug.contains("placeholder_runtime_callee_name=_c"));
        assert!(debug.contains("placeholder_runtime_callee_name_before_transform=none"));
        assert!(debug.contains("placeholder_runtime_callee_candidate_count=1"));
        assert!(debug.contains(
            "placeholder_runtime_callee_candidate_count_before_transform=0"
        ));
        assert!(debug.contains("placeholder_runtime_namespace_candidate_count=0"));
        assert!(debug.contains(
            "placeholder_runtime_namespace_candidate_count_before_transform=0"
        ));
        assert!(debug.contains("name=Component kind=Component"));
    }
}
