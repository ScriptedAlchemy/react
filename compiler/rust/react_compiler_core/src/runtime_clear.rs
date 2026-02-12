use std::collections::HashSet;

use swc_ecma_ast::{AssignTarget, Expr, Lit, MemberProp, SimpleAssignTarget};

use crate::{
    binding::expression_ident,
    helpers::unwrap_expression,
    runtime_binding_utils::{
        collect_binding_names_from_assign_target, collect_binding_names_from_object_pat,
        collect_binding_names_from_pat,
    },
};

pub(crate) fn clear_runtime_bindings_for_pat(
    pattern: &swc_ecma_ast::Pat,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    for binding_name in collect_binding_names_from_pat(pattern) {
        runtime_namespace_bindings.remove(binding_name.as_str());
        runtime_callee_bindings.remove(binding_name.as_str());
    }
}

pub(crate) fn clear_runtime_bindings_for_object_pat_bindings(
    object_pat: &swc_ecma_ast::ObjectPat,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    for binding_name in collect_binding_names_from_object_pat(object_pat) {
        runtime_namespace_bindings.remove(binding_name.as_str());
        runtime_callee_bindings.remove(binding_name.as_str());
    }
}

pub(crate) fn clear_runtime_bindings_for_assign_target(
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

pub(crate) fn clear_runtime_bindings_for_name(
    binding_name: &str,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    runtime_namespace_bindings.remove(binding_name);
    runtime_callee_bindings.remove(binding_name);
}

pub(crate) fn clear_runtime_bindings_for_side_effect_expression(
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
