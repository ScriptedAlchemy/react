use std::collections::HashSet;

use swc_ecma_ast::{AssignTarget, CallExpr, Callee, Expr, Lit, Pat};

use crate::{
    binding::{expression_ident, simple_assign_target_ident},
    entrypoint::{is_member_prop_with, is_prop_name_with},
    helpers::unwrap_expression,
};

pub(crate) fn collect_binding_names_from_pat(pattern: &Pat) -> Vec<String> {
    let mut names = Vec::new();
    collect_binding_names_from_pat_into(pattern, &mut names);
    names
}

pub(crate) fn collect_binding_names_from_assign_target(target: &AssignTarget) -> Vec<String> {
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

pub(crate) fn collect_binding_names_from_object_pat(
    object_pat: &swc_ecma_ast::ObjectPat,
) -> Vec<String> {
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

pub(crate) fn collect_binding_names_from_pat_into(pattern: &Pat, names: &mut Vec<String>) {
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

pub(crate) fn assign_target_object_pat(target: &AssignTarget) -> Option<&swc_ecma_ast::ObjectPat> {
    let AssignTarget::Pat(pattern) = target else {
        return None;
    };
    let swc_ecma_ast::AssignTargetPat::Object(object_pat) = pattern else {
        return None;
    };
    Some(object_pat)
}

pub(crate) fn extract_runtime_callee_from_object_pat(
    object_pat: &swc_ecma_ast::ObjectPat,
) -> Option<String> {
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

pub(crate) fn is_require_runtime_call(expr: &Expr) -> bool {
    let Expr::Call(call_expr) = unwrap_expression(expr) else {
        return false;
    };
    is_require_runtime_call_expr(call_expr)
}

fn is_require_runtime_call_expr(call_expr: &CallExpr) -> bool {
    let Callee::Expr(callee_expr) = &call_expr.callee else {
        return false;
    };
    if !is_require_callee_expr(callee_expr.as_ref()) {
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
        Expr::Tpl(template_literal)
            if template_literal.exprs.is_empty() && template_literal.quasis.len() == 1 =>
        {
            template_literal
                .quasis
                .first()
                .and_then(|quasi| {
                    quasi
                        .cooked
                        .as_ref()
                        .map(|value| value.as_ref())
                        .or_else(|| Some(quasi.raw.as_ref()))
                })
                .map(|value| value == "react/compiler-runtime")
                .unwrap_or(false)
        }
        _ => false,
    }
}

fn is_require_callee_expr(expr: &Expr) -> bool {
    match unwrap_expression(expr) {
        Expr::Ident(callee_ident) => callee_ident.sym == *"require",
        Expr::Member(member_expr) => {
            if !is_member_prop_with(&member_expr.prop, "require") {
                return false;
            }
            is_module_object_expr(member_expr.obj.as_ref())
        }
        Expr::Seq(sequence_expr) => sequence_expr
            .exprs
            .last()
            .map(|last_expr| is_require_callee_expr(last_expr.as_ref()))
            .unwrap_or(false),
        _ => false,
    }
}

fn is_module_object_expr(expr: &Expr) -> bool {
    match unwrap_expression(expr) {
        Expr::Ident(object_ident) => object_ident.sym == *"module",
        Expr::Member(member_expr) => {
            if !is_member_prop_with(&member_expr.prop, "module") {
                return false;
            }
            is_supported_global_module_root_expr(member_expr.obj.as_ref())
        }
        Expr::Seq(sequence_expr) => sequence_expr
            .exprs
            .last()
            .map(|last_expr| is_module_object_expr(last_expr.as_ref()))
            .unwrap_or(false),
        _ => false,
    }
}

fn is_supported_global_module_root_expr(expr: &Expr) -> bool {
    match unwrap_expression(expr) {
        Expr::Ident(root_ident) => {
            matches!(
                root_ident.sym.as_ref(),
                "globalThis" | "global" | "self" | "window"
            )
        }
        Expr::Member(member_expr) => {
            if !is_member_prop_with(&member_expr.prop, "globalThis")
                && !is_member_prop_with(&member_expr.prop, "global")
                && !is_member_prop_with(&member_expr.prop, "self")
                && !is_member_prop_with(&member_expr.prop, "window")
            {
                return false;
            }
            is_supported_global_module_root_expr(member_expr.obj.as_ref())
        }
        Expr::Seq(sequence_expr) => sequence_expr
            .exprs
            .last()
            .map(|last_expr| is_supported_global_module_root_expr(last_expr.as_ref()))
            .unwrap_or(false),
        _ => false,
    }
}

pub(crate) fn member_expr_is_runtime_namespace_c(
    expr: &Expr,
    runtime_namespaces: &HashSet<String>,
) -> bool {
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
