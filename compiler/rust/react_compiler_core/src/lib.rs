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
    pub detected_react_functions: usize,
    pub react_functions: Vec<ReactFunction>,
    pub placeholder_transforms_applied: usize,
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
            "detected_react_functions={}",
            metadata.detected_react_functions
        ),
        format!(
            "placeholder_transforms_applied={}",
            metadata.placeholder_transforms_applied
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
        let transformed_count = if options.apply_placeholder_transforms {
            apply_placeholder_compilation_to_module(&mut module, &react_functions)
        } else {
            0
        };
        let metadata = ParseMetadata {
            statement_count: original_statement_count,
            detected_react_functions: react_functions.len(),
            react_functions,
            placeholder_transforms_applied: transformed_count,
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
        let transformed_count = if options.apply_placeholder_transforms {
            apply_placeholder_compilation_to_script(&mut script, &react_functions)
        } else {
            0
        };
        let metadata = ParseMetadata {
            statement_count: script.body.len(),
            detected_react_functions: react_functions.len(),
            react_functions,
            placeholder_transforms_applied: transformed_count,
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

fn apply_placeholder_compilation_to_module(
    module: &mut Module,
    react_functions: &[ReactFunction],
) -> usize {
    let bindings = collect_top_level_bindings(module);
    let should_transform_default_export = module_has_default_export_component_candidate(module);
    let mut transform_candidate_names: HashSet<String> = react_functions
        .iter()
        .filter_map(|function| {
            react_function_kind(function.name.as_str()).map(|_| function.name.clone())
        })
        .collect();
    transform_candidate_names.extend(collect_default_export_function_names(module, &bindings));
    if transform_candidate_names.is_empty() && !should_transform_default_export {
        return 0;
    }

    let existing_runtime_callee_name = runtime_memo_callee_name(module);
    let runtime_callee_name = existing_runtime_callee_name
        .as_deref()
        .unwrap_or("_c")
        .to_string();

    let mut transformed_count = 0;
    for item in module.body.iter_mut() {
        match item {
            ModuleItem::Stmt(stmt) => {
                transformed_count += apply_placeholder_compilation_to_stmt(
                    stmt,
                    &transform_candidate_names,
                    runtime_callee_name.as_str(),
                );
            }
            ModuleItem::ModuleDecl(module_decl) => {
                transformed_count += apply_placeholder_compilation_to_module_decl(
                    module_decl,
                    &transform_candidate_names,
                    runtime_callee_name.as_str(),
                    should_transform_default_export,
                );
            }
        }
    }

    if transformed_count > 0 && existing_runtime_callee_name.is_none() {
        module.body.insert(
            0,
            ModuleItem::ModuleDecl(ModuleDecl::Import(make_runtime_import_decl())),
        );
    }

    transformed_count
}

fn apply_placeholder_compilation_to_script(
    script: &mut Script,
    react_functions: &[ReactFunction],
) -> usize {
    let transform_candidate_names: HashSet<String> = react_functions
        .iter()
        .filter_map(|function| {
            react_function_kind(function.name.as_str()).map(|_| function.name.clone())
        })
        .collect();
    if transform_candidate_names.is_empty() {
        return 0;
    }

    let Some(runtime_callee_name) = runtime_memo_callee_name_in_script(script) else {
        return 0;
    };

    script.body.iter_mut().fold(0, |count, stmt| {
        count
            + apply_placeholder_compilation_to_stmt(
                stmt,
                &transform_candidate_names,
                runtime_callee_name.as_str(),
            )
    })
}

fn apply_placeholder_compilation_to_module_decl(
    module_decl: &mut ModuleDecl,
    react_function_names: &HashSet<String>,
    runtime_callee_name: &str,
    should_transform_default_export: bool,
) -> usize {
    match module_decl {
        ModuleDecl::ExportDecl(export_decl) => apply_placeholder_compilation_to_decl(
            &mut export_decl.decl,
            react_function_names,
            runtime_callee_name,
        ),
        ModuleDecl::ExportDefaultDecl(default_decl) => match &mut default_decl.decl {
            DefaultDecl::Fn(fn_expr) => {
                if !should_transform_default_export {
                    return 0;
                }
                if inject_placeholder_memo_init_into_function(
                    &mut fn_expr.function,
                    runtime_callee_name,
                ) {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        },
        ModuleDecl::ExportDefaultExpr(default_expr) => {
            if !should_transform_default_export {
                return 0;
            }
            match unwrap_expression_mut(default_expr.expr.as_mut()) {
                Expr::Fn(fn_expr) => {
                    if inject_placeholder_memo_init_into_function(
                        &mut fn_expr.function,
                        runtime_callee_name,
                    ) {
                        1
                    } else {
                        0
                    }
                }
                Expr::Arrow(arrow_expr) => {
                    if inject_placeholder_memo_init_into_arrow_function(
                        arrow_expr,
                        runtime_callee_name,
                    ) {
                        1
                    } else {
                        0
                    }
                }
                _ => 0,
            }
        }
        _ => 0,
    }
}

fn apply_placeholder_compilation_to_stmt(
    stmt: &mut Stmt,
    react_function_names: &HashSet<String>,
    runtime_callee_name: &str,
) -> usize {
    match stmt {
        Stmt::Decl(decl) => {
            apply_placeholder_compilation_to_decl(decl, react_function_names, runtime_callee_name)
        }
        Stmt::Expr(expr_stmt) => apply_placeholder_compilation_to_expr(
            expr_stmt.expr.as_mut(),
            react_function_names,
            runtime_callee_name,
        ),
        _ => 0,
    }
}

fn apply_placeholder_compilation_to_expr(
    expr: &mut Expr,
    react_function_names: &HashSet<String>,
    runtime_callee_name: &str,
) -> usize {
    let Expr::Assign(assign_expr) = unwrap_expression_mut(expr) else {
        return 0;
    };
    if assign_expr.op != swc_ecma_ast::AssignOp::Assign {
        return 0;
    }
    let Some(target_name) = assign_target_ident(&assign_expr.left) else {
        return 0;
    };
    if !react_function_names.contains(target_name.as_str()) {
        return 0;
    }
    match unwrap_expression_mut(assign_expr.right.as_mut()) {
        Expr::Fn(fn_expr) => {
            if inject_placeholder_memo_init_into_function(
                &mut fn_expr.function,
                runtime_callee_name,
            ) {
                1
            } else {
                0
            }
        }
        Expr::Arrow(arrow_expr) => {
            if inject_placeholder_memo_init_into_arrow_function(arrow_expr, runtime_callee_name) {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn apply_placeholder_compilation_to_decl(
    decl: &mut Decl,
    react_function_names: &HashSet<String>,
    runtime_callee_name: &str,
) -> usize {
    match decl {
        Decl::Fn(fn_decl) => {
            if react_function_names.contains(fn_decl.ident.sym.as_ref()) {
                if inject_placeholder_memo_init_into_function(
                    &mut fn_decl.function,
                    runtime_callee_name,
                ) {
                    return 1;
                }
            }
            0
        }
        Decl::Var(var_decl) => {
            let mut transformed_count = 0;
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
                            transformed_count += 1;
                        }
                    }
                    Expr::Arrow(arrow_expr) => {
                        if inject_placeholder_memo_init_into_arrow_function(
                            arrow_expr,
                            runtime_callee_name,
                        ) {
                            transformed_count += 1;
                        }
                    }
                    _ => {}
                }
            }
            transformed_count
        }
        _ => 0,
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

fn runtime_memo_callee_name(module: &Module) -> Option<String> {
    module.body.iter().find_map(|item| {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) = item else {
            return None;
        };
        if import_decl.src.value != *"react/compiler-runtime" {
            return None;
        }
        import_decl
            .specifiers
            .iter()
            .find_map(runtime_memo_callee_name_from_specifier)
    })
}

fn runtime_memo_callee_name_from_specifier(specifier: &ImportSpecifier) -> Option<String> {
    let ImportSpecifier::Named(named) = specifier else {
        return None;
    };
    if named.is_type_only {
        return None;
    }
    let is_memo_runtime_import = named
        .imported
        .as_ref()
        .map(|imported| imported.atom() == &"c")
        .unwrap_or(named.local.sym == *"c");
    if !is_memo_runtime_import {
        return None;
    }
    Some(named.local.sym.to_string())
}

fn runtime_memo_callee_name_in_script(script: &Script) -> Option<String> {
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
            Stmt::Expr(expr_stmt) => collect_runtime_bindings_from_script_assignment_expr(
                expr_stmt.expr.as_ref(),
                &mut runtime_namespace_bindings,
                &mut runtime_callee_bindings,
            ),
            _ => {}
        }
    }
    runtime_callee_bindings.into_iter().next()
}

fn collect_runtime_bindings_from_script_declarator(
    declarator: &VarDeclarator,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    let Some(init) = declarator.init.as_deref().map(unwrap_expression) else {
        return;
    };
    if is_require_runtime_call(init) {
        match &declarator.name {
            Pat::Ident(binding) => {
                runtime_namespace_bindings.insert(binding.id.sym.to_string());
            }
            Pat::Object(object_pat) => {
                if let Some(callee_name) = extract_runtime_callee_from_object_pat(object_pat) {
                    runtime_callee_bindings.insert(callee_name);
                }
            }
            _ => {}
        }
        return;
    }

    if let Some(namespace_name) = expression_ident(init) {
        if runtime_namespace_bindings.contains(namespace_name.as_str()) {
            if let Pat::Object(object_pat) = &declarator.name {
                if let Some(callee_name) = extract_runtime_callee_from_object_pat(object_pat) {
                    runtime_callee_bindings.insert(callee_name);
                }
            } else if let Pat::Ident(binding) = &declarator.name {
                runtime_namespace_bindings.insert(binding.id.sym.to_string());
            }
            return;
        }
    }

    if member_expr_is_runtime_namespace_c(init, runtime_namespace_bindings) {
        if let Pat::Ident(binding) = &declarator.name {
            runtime_callee_bindings.insert(binding.id.sym.to_string());
        }
    }
}

fn collect_runtime_bindings_from_script_assignment_expr(
    expr: &Expr,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    let Expr::Assign(assign_expr) = unwrap_expression(expr) else {
        return;
    };
    if assign_expr.op != swc_ecma_ast::AssignOp::Assign {
        return;
    }
    let target_ident = assign_target_ident(&assign_expr.left);
    let target_object_pat = assign_target_object_pat(&assign_expr.left);
    let right = unwrap_expression(assign_expr.right.as_ref());
    if is_require_runtime_call(right) {
        if let Some(target_name) = target_ident.clone() {
            runtime_namespace_bindings.insert(target_name);
        }
        if let Some(object_pat) = target_object_pat {
            if let Some(callee_name) = extract_runtime_callee_from_object_pat(object_pat) {
                runtime_callee_bindings.insert(callee_name);
            }
        }
        return;
    }
    if let Some(namespace_name) = expression_ident(right) {
        if runtime_namespace_bindings.contains(namespace_name.as_str()) {
            if let Some(target_name) = target_ident.clone() {
                runtime_namespace_bindings.insert(target_name);
            }
            if let Some(object_pat) = target_object_pat {
                if let Some(callee_name) = extract_runtime_callee_from_object_pat(object_pat) {
                    runtime_callee_bindings.insert(callee_name);
                }
            }
            return;
        }
    }
    if member_expr_is_runtime_namespace_c(right, runtime_namespace_bindings) {
        if let Some(target_name) = target_ident {
            runtime_callee_bindings.insert(target_name);
        }
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
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions.len(), 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
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
        assert!(output.code.contains("const $ = cache(0);"));
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
        assert_eq!(
            output.metadata.react_functions[0].name,
            super::DEFAULT_EXPORT_COMPONENT_NAME
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
        assert_eq!(output.metadata.react_functions[0].name, "useValue");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Hook
        );
        assert!(!output.code.contains("react/compiler-runtime"));
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
        assert!(debug.contains("detected_react_functions=2"));
        assert!(debug.contains("placeholder_transforms_applied=0"));
        assert!(debug.contains("name=Component kind=Component"));
        assert!(debug.contains("name=useThing kind=Hook"));
    }
}
