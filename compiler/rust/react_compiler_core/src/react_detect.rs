use std::collections::HashSet;

use swc_common::{sync::Lrc, SourceMap};
use swc_ecma_ast::{
    Decl, DefaultDecl, Expr, Module, ModuleDecl, ModuleItem, Pat, Script, Stmt,
};

use crate::{
    binding::{
        assign_target_ident, collect_top_level_bindings, collect_top_level_bindings_in_script,
        resolve_function_binding_names,
    },
    entrypoint::{
        collect_default_export_function_names, collect_fixture_entrypoint_function_names,
        collect_fixture_entrypoint_function_names_in_script,
    },
    helpers::{span_to_location, unwrap_expression},
    react_fn::{react_function_kind, sort_react_functions},
    ReactFunction, ReactFunctionKind, DEFAULT_EXPORT_COMPONENT_NAME,
};

pub(crate) fn collect_react_functions_in_module(
    cm: &Lrc<SourceMap>,
    module: &Module,
) -> Vec<ReactFunction> {
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

pub(crate) fn collect_react_functions_in_script(
    cm: &Lrc<SourceMap>,
    script: &Script,
) -> Vec<ReactFunction> {
    let bindings = collect_top_level_bindings_in_script(script);
    let mut functions: Vec<ReactFunction> = script
        .body
        .iter()
        .flat_map(|stmt| collect_react_functions_in_stmt(cm, stmt))
        .collect();
    let fixture_entrypoint_names = collect_fixture_entrypoint_function_names_in_script(script);
    if !fixture_entrypoint_names.is_empty() {
        let resolved_fixture_entrypoint_names =
            resolve_function_binding_names(&bindings, &fixture_entrypoint_names);
        let fixture_entrypoint_functions =
            collect_named_functions_in_script(cm, script, &resolved_fixture_entrypoint_names);
        functions = merge_react_functions(functions, fixture_entrypoint_functions);
    }
    sort_react_functions(&mut functions);
    functions
}

pub(crate) fn collect_placeholder_transform_candidate_names_for_script(
    react_functions: &[ReactFunction],
) -> HashSet<String> {
    react_functions
        .iter()
        .map(|function| function.name.clone())
        .collect()
}

pub(crate) fn collect_placeholder_transform_candidate_names_for_module(
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

fn collect_named_functions_in_script(
    cm: &Lrc<SourceMap>,
    script: &Script,
    target_names: &HashSet<String>,
) -> Vec<ReactFunction> {
    script
        .body
        .iter()
        .flat_map(|stmt| collect_named_functions_in_stmt(cm, stmt, target_names))
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
