use std::collections::HashSet;

use swc_ecma_ast::PropName;

pub(crate) fn collect_runtime_bindings_from_class(
    class: &swc_ecma_ast::Class,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    collect_runtime_bindings_from_decorators(
        &class.decorators,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );
    if let Some(super_class) = &class.super_class {
        crate::collect_runtime_bindings_from_expression(
            super_class.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
    for class_member in &class.body {
        match class_member {
            swc_ecma_ast::ClassMember::Method(class_method) => {
                collect_runtime_bindings_from_class_key(
                    &class_method.key,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
            swc_ecma_ast::ClassMember::ClassProp(class_prop) => {
                collect_runtime_bindings_from_class_key(
                    &class_prop.key,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                collect_runtime_bindings_from_decorators(
                    &class_prop.decorators,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                if class_prop.is_static {
                    if let Some(value) = &class_prop.value {
                        crate::collect_runtime_bindings_from_expression(
                            value.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                }
            }
            swc_ecma_ast::ClassMember::PrivateProp(private_prop) => {
                collect_runtime_bindings_from_decorators(
                    &private_prop.decorators,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                if private_prop.is_static {
                    if let Some(value) = &private_prop.value {
                        crate::collect_runtime_bindings_from_expression(
                            value.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                }
            }
            swc_ecma_ast::ClassMember::StaticBlock(static_block) => {
                collect_runtime_bindings_from_static_block(
                    static_block,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
            swc_ecma_ast::ClassMember::AutoAccessor(auto_accessor) => {
                if let swc_ecma_ast::Key::Public(prop_name) = &auto_accessor.key {
                    collect_runtime_bindings_from_class_key(
                        prop_name,
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        may_be_conditional,
                    );
                }
                collect_runtime_bindings_from_decorators(
                    &auto_accessor.decorators,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
                if auto_accessor.is_static {
                    if let Some(value) = &auto_accessor.value {
                        crate::collect_runtime_bindings_from_expression(
                            value.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_runtime_bindings_from_decorators(
    decorators: &[swc_ecma_ast::Decorator],
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    for decorator in decorators {
        crate::collect_runtime_bindings_from_expression(
            decorator.expr.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}

fn collect_runtime_bindings_from_class_key(
    key: &PropName,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    collect_runtime_bindings_from_prop_name(
        key,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );
}

pub(crate) fn collect_runtime_bindings_from_prop_name(
    key: &PropName,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    if let PropName::Computed(computed_key) = key {
        crate::collect_runtime_bindings_from_expression(
            computed_key.expr.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}

fn collect_runtime_bindings_from_static_block(
    static_block: &swc_ecma_ast::StaticBlock,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    crate::runtime_stmt::collect_runtime_bindings_from_static_block_stmts(
        &static_block.body.stmts,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );
}
