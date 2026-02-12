use std::collections::{HashMap, HashSet};

use swc_ecma_ast::{
    AssignTarget, Decl, Expr, Module, ModuleDecl, ModuleItem, Pat, Script, SimpleAssignTarget, Stmt,
};

use crate::helpers::unwrap_expression;

pub(crate) enum TopLevelBinding {
    FunctionLike,
    Alias(String),
}

pub(crate) fn collect_top_level_bindings(module: &Module) -> HashMap<String, TopLevelBinding> {
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

pub(crate) fn collect_top_level_bindings_in_script(
    script: &Script,
) -> HashMap<String, TopLevelBinding> {
    let mut bindings = HashMap::new();
    for stmt in &script.body {
        match stmt {
            Stmt::Decl(decl) => record_top_level_bindings_from_decl(decl, &mut bindings),
            Stmt::Expr(expr_stmt) => {
                record_top_level_bindings_from_expr(expr_stmt.expr.as_ref(), &mut bindings)
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

pub(crate) fn assign_target_ident(target: &AssignTarget) -> Option<String> {
    match target {
        AssignTarget::Simple(simple) => simple_assign_target_ident(simple),
        _ => None,
    }
}

pub(crate) fn simple_assign_target_ident(target: &SimpleAssignTarget) -> Option<String> {
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

pub(crate) fn expression_ident(expr: &Expr) -> Option<String> {
    match unwrap_expression(expr) {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        _ => None,
    }
}

pub(crate) fn resolve_function_binding_name(
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

pub(crate) fn resolve_function_binding_names(
    bindings: &HashMap<String, TopLevelBinding>,
    names: &HashSet<String>,
) -> HashSet<String> {
    names
        .iter()
        .filter_map(|name| resolve_function_binding_name(bindings, name))
        .collect()
}
