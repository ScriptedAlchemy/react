use std::collections::HashSet;
use swc_common::{
    comments::SingleThreadedComments, sync::Lrc, FileName, SourceMap, Span, Spanned, DUMMY_SP,
};
use swc_ecma_ast::{
    BindingIdent, BlockStmt, BlockStmtOrExpr, CallExpr, Callee, Decl, DefaultDecl, EsVersion, Expr,
    ExprOrSpread, Ident, ImportDecl, ImportNamedSpecifier, ImportSpecifier, Lit, Module,
    ModuleDecl, ModuleExportName, ModuleItem, Number, Pat, Prop, PropName, PropOrSpread, Script,
    Stmt, VarDecl, VarDeclKind, VarDeclarator,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileOutput {
    pub code: String,
    pub metadata: ParseMetadata,
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
        let metadata = ParseMetadata {
            statement_count: module.body.len(),
            detected_react_functions: react_functions.len(),
            react_functions,
        };
        if options.apply_placeholder_transforms {
            apply_placeholder_compilation_to_module(&mut module, &metadata.react_functions);
        }
        (metadata, emit_module(&cm, &comments, &module)?)
    } else {
        let script = parser.parse_script().map_err(|err| {
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
        let metadata = ParseMetadata {
            statement_count: script.body.len(),
            detected_react_functions: react_functions.len(),
            react_functions,
        };
        (metadata, emit_script(&cm, &comments, &script)?)
    };

    Ok(CompileOutput { code, metadata })
}

fn parse_syntax_error_reason(kind: &ParserSyntaxError) -> &'static str {
    match kind {
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
    if message.contains("Unexpected token") || message.starts_with("Unexpected character") {
        return "unexpected_token";
    }
    if message.starts_with("Unexpected eof") {
        return "unexpected_eof";
    }
    if message.starts_with("Expected ") {
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

fn collect_react_functions_in_decl(cm: &Lrc<SourceMap>, decl: &Decl) -> Vec<ReactFunction> {
    match decl {
        Decl::Fn(fn_decl) => react_function_kind(fn_decl.ident.sym.as_ref())
            .map(|kind| ReactFunction {
                name: fn_decl.ident.sym.to_string(),
                kind,
                loc: span_to_location(cm, fn_decl.ident.span),
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
                let is_function_value = declarator
                    .init
                    .as_ref()
                    .map(|expr| matches!(&**expr, Expr::Fn(_) | Expr::Arrow(_)))
                    .unwrap_or(false);
                if !is_function_value {
                    return None;
                }

                react_function_kind(binding.id.sym.as_ref()).map(|kind| ReactFunction {
                    name: binding.id.sym.to_string(),
                    kind,
                    loc: span_to_location(cm, binding.id.span),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn collect_react_functions_in_stmt(cm: &Lrc<SourceMap>, stmt: &Stmt) -> Vec<ReactFunction> {
    match stmt {
        Stmt::Decl(decl) => collect_react_functions_in_decl(cm, decl),
        _ => Vec::new(),
    }
}

fn collect_react_functions_in_module(cm: &Lrc<SourceMap>, module: &Module) -> Vec<ReactFunction> {
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
                    DefaultDecl::Fn(fn_expr) => fn_expr
                        .ident
                        .as_ref()
                        .and_then(|ident| {
                            react_function_kind(ident.sym.as_ref()).map(|kind| ReactFunction {
                                name: ident.sym.to_string(),
                                kind,
                                loc: span_to_location(cm, ident.span),
                            })
                        })
                        .into_iter()
                        .collect(),
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            },
        })
        .collect();

    let fixture_entrypoint_names = collect_fixture_entrypoint_function_names(module);
    if !fixture_entrypoint_names.is_empty() {
        let fixture_entrypoint_functions =
            collect_named_functions_in_module(cm, module, &fixture_entrypoint_names);
        functions = merge_react_functions(functions, fixture_entrypoint_functions);
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

fn fixture_entrypoint_fn_name_from_object_literal(
    object_literal: &swc_ecma_ast::ObjectLit,
) -> Option<String> {
    object_literal.props.iter().find_map(|prop_or_spread| {
        let PropOrSpread::Prop(prop) = prop_or_spread else {
            return None;
        };
        let Prop::KeyValue(key_value) = prop.as_ref() else {
            return None;
        };
        if !is_fn_property_name(&key_value.key) {
            return None;
        }
        match key_value.value.as_ref() {
            Expr::Ident(ident) => Some(ident.sym.to_string()),
            Expr::Fn(fn_expr) => fn_expr.ident.as_ref().map(|ident| ident.sym.to_string()),
            _ => None,
        }
    })
}

fn is_fn_property_name(name: &PropName) -> bool {
    match name {
        PropName::Ident(ident) => ident.sym == *"fn",
        PropName::Str(str_lit) => str_lit.value == *"fn",
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
                            loc: span_to_location(cm, ident.span),
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
        _ => Vec::new(),
    }
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
                    loc: span_to_location(cm, fn_decl.ident.span),
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
                let is_function_value = declarator
                    .init
                    .as_ref()
                    .map(|expr| matches!(&**expr, Expr::Fn(_) | Expr::Arrow(_)))
                    .unwrap_or(false);
                if !is_function_value {
                    return None;
                }
                Some(ReactFunction {
                    name: binding.id.sym.to_string(),
                    kind: ReactFunctionKind::Component,
                    loc: span_to_location(cm, binding.id.span),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn apply_placeholder_compilation_to_module(module: &mut Module, react_functions: &[ReactFunction]) {
    let transform_candidate_names: HashSet<&str> = react_functions
        .iter()
        .filter_map(|function| {
            react_function_kind(function.name.as_str()).map(|_| function.name.as_str())
        })
        .collect();
    if transform_candidate_names.is_empty() {
        return;
    }

    let existing_runtime_callee_name = runtime_memo_callee_name(module);
    let runtime_callee_name = existing_runtime_callee_name
        .as_deref()
        .unwrap_or("_c")
        .to_string();

    let mut transformed = false;
    for item in module.body.iter_mut() {
        match item {
            ModuleItem::Stmt(stmt) => {
                if apply_placeholder_compilation_to_stmt(
                    stmt,
                    &transform_candidate_names,
                    runtime_callee_name.as_str(),
                ) {
                    transformed = true;
                }
            }
            ModuleItem::ModuleDecl(module_decl) => {
                if apply_placeholder_compilation_to_module_decl(
                    module_decl,
                    &transform_candidate_names,
                    runtime_callee_name.as_str(),
                ) {
                    transformed = true;
                }
            }
        }
    }

    if transformed && existing_runtime_callee_name.is_none() {
        module.body.insert(
            0,
            ModuleItem::ModuleDecl(ModuleDecl::Import(make_runtime_import_decl())),
        );
    }
}

fn apply_placeholder_compilation_to_module_decl(
    module_decl: &mut ModuleDecl,
    react_function_names: &HashSet<&str>,
    runtime_callee_name: &str,
) -> bool {
    match module_decl {
        ModuleDecl::ExportDecl(export_decl) => apply_placeholder_compilation_to_decl(
            &mut export_decl.decl,
            react_function_names,
            runtime_callee_name,
        ),
        ModuleDecl::ExportDefaultDecl(default_decl) => match &mut default_decl.decl {
            DefaultDecl::Fn(fn_expr) => {
                let Some(ident) = fn_expr.ident.as_ref() else {
                    return false;
                };
                if !react_function_names.contains(ident.sym.as_ref()) {
                    return false;
                }
                inject_placeholder_memo_init_into_function(
                    &mut fn_expr.function,
                    runtime_callee_name,
                );
                true
            }
            _ => false,
        },
        _ => false,
    }
}

fn apply_placeholder_compilation_to_stmt(
    stmt: &mut Stmt,
    react_function_names: &HashSet<&str>,
    runtime_callee_name: &str,
) -> bool {
    match stmt {
        Stmt::Decl(decl) => {
            apply_placeholder_compilation_to_decl(decl, react_function_names, runtime_callee_name)
        }
        _ => false,
    }
}

fn apply_placeholder_compilation_to_decl(
    decl: &mut Decl,
    react_function_names: &HashSet<&str>,
    runtime_callee_name: &str,
) -> bool {
    match decl {
        Decl::Fn(fn_decl) => {
            if react_function_names.contains(fn_decl.ident.sym.as_ref()) {
                inject_placeholder_memo_init_into_function(
                    &mut fn_decl.function,
                    runtime_callee_name,
                );
                return true;
            }
            false
        }
        Decl::Var(var_decl) => {
            let mut transformed = false;
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
                match &mut **init {
                    Expr::Fn(fn_expr) => {
                        inject_placeholder_memo_init_into_function(
                            &mut fn_expr.function,
                            runtime_callee_name,
                        );
                        transformed = true;
                    }
                    Expr::Arrow(arrow_expr) => {
                        inject_placeholder_memo_init_into_arrow_function(
                            arrow_expr,
                            runtime_callee_name,
                        );
                        transformed = true;
                    }
                    _ => {}
                }
            }
            transformed
        }
        _ => false,
    }
}

fn inject_placeholder_memo_init_into_function(
    function: &mut swc_ecma_ast::Function,
    runtime_callee_name: &str,
) {
    if function_has_placeholder_memo_init(function, runtime_callee_name) {
        return;
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
}

fn inject_placeholder_memo_init_into_arrow_function(
    arrow: &mut swc_ecma_ast::ArrowExpr,
    runtime_callee_name: &str,
) {
    if arrow_has_placeholder_memo_init(arrow, runtime_callee_name) {
        return;
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

#[cfg(test)]
mod tests {
    use super::{compile, CompilerError, CompilerOptions, InputDialect};

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
}
