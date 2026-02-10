use std::collections::HashSet;

use swc_ecma_ast::{Callee, Expr, MemberProp, Prop, PropOrSpread};

pub(crate) fn collect_runtime_bindings_from_expression(
    expr: &Expr,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let expression = crate::helpers::unwrap_expression(expr);
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
                        crate::runtime_class::collect_runtime_bindings_from_prop_name(
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
                        crate::runtime_class::collect_runtime_bindings_from_prop_name(
                            &getter_prop.key,
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                    Prop::Setter(setter_prop) => {
                        crate::runtime_class::collect_runtime_bindings_from_prop_name(
                            &setter_prop.key,
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                    Prop::Method(method_prop) => {
                        crate::runtime_class::collect_runtime_bindings_from_prop_name(
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
        crate::runtime_jsx::collect_runtime_bindings_from_jsx_element(
            jsx_element.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
        return;
    }
    if let Expr::JSXFragment(jsx_fragment) = expression {
        crate::runtime_jsx::collect_runtime_bindings_from_jsx_fragment(
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
            crate::runtime_clear::clear_runtime_bindings_for_side_effect_expression(
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
        crate::runtime_class::collect_runtime_bindings_from_class(
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
        crate::runtime_clear::clear_runtime_bindings_for_side_effect_expression(
            expression,
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
        return;
    };
    if may_be_conditional {
        crate::runtime_clear::clear_runtime_bindings_for_assign_target(
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
        crate::runtime_clear::clear_runtime_bindings_for_assign_target(
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
    let target_ident = crate::binding::assign_target_ident(&assign_expr.left);
    let target_object_pat = crate::runtime_binding_utils::assign_target_object_pat(&assign_expr.left);
    let right = crate::runtime_helpers::runtime_initializer_expr(assign_expr.right.as_ref());
    if crate::runtime_binding_utils::is_require_runtime_call(right) {
        crate::runtime_clear::clear_runtime_bindings_for_assign_target(
            &assign_expr.left,
            runtime_namespace_bindings,
            runtime_callee_bindings,
        );
        if let Some(target_name) = target_ident.as_ref() {
            runtime_namespace_bindings.insert(target_name.clone());
            runtime_callee_bindings.remove(target_name.as_str());
        } else if let Some(object_pat) = target_object_pat {
            if let Some(callee_name) =
                crate::runtime_binding_utils::extract_runtime_callee_from_object_pat(object_pat)
            {
                runtime_callee_bindings.insert(callee_name.clone());
                runtime_namespace_bindings.remove(callee_name.as_str());
            }
        }
        return;
    }
    if let Some(namespace_name) = crate::binding::expression_ident(right) {
        if runtime_namespace_bindings.contains(namespace_name.as_str()) {
            crate::runtime_clear::clear_runtime_bindings_for_assign_target(
                &assign_expr.left,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
            if let Some(target_name) = target_ident.as_ref() {
                runtime_namespace_bindings.insert(target_name.clone());
                runtime_callee_bindings.remove(target_name.as_str());
            } else if let Some(object_pat) = target_object_pat {
                if let Some(callee_name) =
                    crate::runtime_binding_utils::extract_runtime_callee_from_object_pat(object_pat)
                {
                    runtime_callee_bindings.insert(callee_name.clone());
                    runtime_namespace_bindings.remove(callee_name.as_str());
                }
            }
            return;
        }
        if runtime_callee_bindings.contains(namespace_name.as_str()) {
            crate::runtime_clear::clear_runtime_bindings_for_assign_target(
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
    if crate::runtime_binding_utils::member_expr_is_runtime_namespace_c(
        right,
        runtime_namespace_bindings,
    ) {
        crate::runtime_clear::clear_runtime_bindings_for_assign_target(
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

    crate::runtime_clear::clear_runtime_bindings_for_assign_target(
        &assign_expr.left,
        runtime_namespace_bindings,
        runtime_callee_bindings,
    );
}
