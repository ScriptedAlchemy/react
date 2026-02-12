use std::collections::HashSet;

use swc_ecma_ast::{Decl, DefaultDecl, Expr, Module, ModuleDecl, ModuleItem, Pat, Script, Stmt};

use crate::{
    binding::assign_target_ident,
    entrypoint::module_has_default_export_component_candidate,
    helpers::unwrap_expression_mut,
    model::DEFAULT_EXPORT_COMPONENT_NAME,
    placeholder::{
        inject_placeholder_memo_init_into_arrow_function,
        inject_placeholder_memo_init_into_function, make_runtime_import_decl,
    },
    react_detect::{
        collect_placeholder_transform_candidate_names_for_module,
        collect_placeholder_transform_candidate_names_for_script,
    },
    runtime_scan::{runtime_memo_callee_name, runtime_memo_callee_name_in_script},
    ReactFunction,
};

pub(crate) fn apply_placeholder_compilation_to_module(
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

pub(crate) fn apply_placeholder_compilation_to_script(
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
