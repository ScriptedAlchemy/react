use std::collections::HashSet;
use swc_common::{
    comments::SingleThreadedComments, sync::Lrc, FileName, SourceMap, Spanned,
};
use swc_ecma_ast::{
    Callee, Decl, DefaultDecl, EsVersion, Expr, MemberProp, Module, ModuleDecl, ModuleItem, Pat,
    Prop, PropOrSpread, Script, Stmt, VarDeclarator,
};
use swc_ecma_parser::{
    lexer::Lexer, EsSyntax, Parser, StringInput, Syntax, TsSyntax,
};
mod error;
mod binding;
mod entrypoint;
mod emit;
mod helpers;
mod model;
mod parse;
mod placeholder;
mod react_fn;
mod runtime_binding_utils;
mod runtime_class;
mod runtime_clear;
mod runtime_helpers;
mod runtime_jsx;
mod runtime_scope;
mod runtime_scan;
mod runtime_stmt;

pub use error::CompilerError;
pub use model::{
    render_react_functions_debug, CompileOutput, CompilerOptions, InputDialect, ParseMetadata,
    ReactFunction, ReactFunctionKind, SourceLocation, DEFAULT_EXPORT_COMPONENT_NAME,
};
use binding::{
    assign_target_ident, collect_top_level_bindings, expression_ident,
    resolve_function_binding_names,
};
use entrypoint::{
    collect_default_export_function_names, collect_fixture_entrypoint_function_names,
    module_has_default_export_component_candidate,
};
use emit::{emit_module, emit_script};
use helpers::{span_to_location, unwrap_expression, unwrap_expression_mut};
use parse::parse_syntax_error_reason;
use placeholder::{
    count_runtime_helper_imports, inject_placeholder_memo_init_into_arrow_function,
    inject_placeholder_memo_init_into_function, make_runtime_import_decl,
};
use react_fn::{
    collect_detected_react_function_names_by_kind, compute_placeholder_transform_skipped_functions,
    count_detected_react_functions_by_kind, count_placeholder_transform_names_by_kind,
    derive_placeholder_transform_status, react_function_kind, sort_and_dedup_names,
    sort_react_functions,
};
use runtime_scan::{
    collect_runtime_bindings_from_import_decl, runtime_memo_callee_name,
    runtime_memo_callee_name_in_script, runtime_memo_callee_scan_for_module,
    runtime_memo_callee_scan_for_script, select_runtime_callee_name,
    sorted_runtime_callee_candidates, sorted_runtime_namespace_candidates,
};
use runtime_stmt::{
    collect_runtime_bindings_from_static_block_stmt, collect_runtime_bindings_from_var_decl_in_static_block,
};
use runtime_binding_utils::{
    assign_target_object_pat, extract_runtime_callee_from_object_pat,
    is_require_runtime_call, member_expr_is_runtime_namespace_c,
};
use runtime_class::{
    collect_runtime_bindings_from_class, collect_runtime_bindings_from_prop_name,
};
use runtime_clear::{
    clear_runtime_bindings_for_assign_target, clear_runtime_bindings_for_name,
    clear_runtime_bindings_for_object_pat_bindings, clear_runtime_bindings_for_pat,
    clear_runtime_bindings_for_side_effect_expression,
};
use runtime_helpers::{
    runtime_initializer_expr, ts_entity_name_leaf_name, ts_entity_name_root_name,
};
use runtime_jsx::{
    collect_runtime_bindings_from_jsx_element, collect_runtime_bindings_from_jsx_fragment,
};
use runtime_scope::{
    collect_declared_binding_names_from_module_items, collect_declared_binding_names_from_stmts,
    with_shadowed_runtime_bindings,
};

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
        let (detected_component_functions, detected_hook_functions) =
            collect_detected_react_function_names_by_kind(&react_functions);
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
            detected_component_functions,
            detected_hook_functions,
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
        let (detected_component_functions, detected_hook_functions) =
            collect_detected_react_function_names_by_kind(&react_functions);
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
            detected_component_functions,
            detected_hook_functions,
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

pub(crate) fn collect_runtime_bindings_from_ts_import_equals_decl(
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

pub(crate) fn collect_runtime_bindings_from_ts_enum_decl(
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

pub(crate) fn collect_runtime_bindings_from_ts_module_decl(
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

pub(crate) fn collect_runtime_bindings_from_script_declarator(
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

pub(crate) fn collect_runtime_bindings_from_script_assignment_expr(
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

pub(crate) fn collect_runtime_bindings_from_expression(
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

pub(crate) fn collect_runtime_bindings_from_static_block_stmts(
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
        assert_eq!(
            output.metadata.detected_component_functions,
            vec!["Component".to_string()]
        );
        assert!(output.metadata.detected_hook_functions.is_empty());
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
        assert_eq!(
            output.metadata.detected_component_functions,
            vec!["Component".to_string()]
        );
        assert!(output.metadata.detected_hook_functions.is_empty());
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
            output.metadata.detected_component_functions,
            vec![super::DEFAULT_EXPORT_COMPONENT_NAME.to_string()]
        );
        assert!(output.metadata.detected_hook_functions.is_empty());
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
        assert!(output.metadata.detected_component_functions.is_empty());
        assert_eq!(
            output.metadata.detected_hook_functions,
            vec!["useValue".to_string()]
        );
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
        assert!(output.metadata.detected_component_functions.is_empty());
        assert_eq!(
            output.metadata.detected_hook_functions,
            vec!["useValue".to_string()]
        );
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
        assert!(debug.contains("detected_component_functions=Component"));
        assert!(debug.contains("detected_hook_functions=useThing"));
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
        assert!(debug.contains("detected_component_functions=Component"));
        assert!(debug.contains("detected_hook_functions="));
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
