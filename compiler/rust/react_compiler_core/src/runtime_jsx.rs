use std::collections::HashSet;

pub(crate) fn collect_runtime_bindings_from_jsx_element(
    jsx_element: &swc_ecma_ast::JSXElement,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    for attr_or_spread in &jsx_element.opening.attrs {
        match attr_or_spread {
            swc_ecma_ast::JSXAttrOrSpread::SpreadElement(spread) => {
                crate::collect_runtime_bindings_from_expression(
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

pub(crate) fn collect_runtime_bindings_from_jsx_fragment(
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

pub(crate) fn collect_runtime_bindings_from_jsx_child(
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
            crate::collect_runtime_bindings_from_expression(
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

pub(crate) fn collect_runtime_bindings_from_jsx_attr_value(
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

pub(crate) fn collect_runtime_bindings_from_jsx_expr_container(
    container: &swc_ecma_ast::JSXExprContainer,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    if let swc_ecma_ast::JSXExpr::Expr(expr) = &container.expr {
        crate::collect_runtime_bindings_from_expression(
            expr.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}
