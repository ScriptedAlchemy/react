use std::collections::HashSet;

use swc_ecma_ast::{Decl, DefaultDecl, Expr, ModuleDecl, ModuleItem, Pat, VarDeclarator};

pub(crate) fn collect_runtime_bindings_from_ts_import_equals_decl(
    import_equals_decl: &swc_ecma_ast::TsImportEqualsDecl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    let binding_name = import_equals_decl.id.sym.to_string();
    crate::runtime_clear::clear_runtime_bindings_for_name(
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
            let source_root_name = crate::runtime_helpers::ts_entity_name_root_name(entity_name);
            if runtime_namespace_bindings.contains(source_root_name) {
                let source_leaf_name =
                    crate::runtime_helpers::ts_entity_name_leaf_name(entity_name);
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
            crate::collect_runtime_bindings_from_expression(
                crate::runtime_helpers::runtime_initializer_expr(init.as_ref()),
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
    let shadowed_bindings =
        crate::runtime_scope::collect_declared_binding_names_from_module_items(&module_block.body);
    crate::runtime_scope::with_shadowed_runtime_bindings(
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
            crate::runtime_scan::collect_runtime_bindings_from_import_decl(
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
                crate::runtime_class::collect_runtime_bindings_from_class(
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
        ModuleItem::Stmt(stmt) => {
            crate::runtime_stmt::collect_runtime_bindings_from_static_block_stmt(
                stmt,
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            )
        }
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
                crate::runtime_stmt::collect_runtime_bindings_from_var_decl_in_static_block(
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
                        crate::collect_runtime_bindings_from_expression(
                            crate::runtime_helpers::runtime_initializer_expr(init),
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
        Decl::Class(class_decl) => crate::runtime_class::collect_runtime_bindings_from_class(
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
    let Some(init) = declarator
        .init
        .as_deref()
        .map(crate::runtime_helpers::runtime_initializer_expr)
    else {
        return;
    };
    if crate::runtime_binding_utils::is_require_runtime_call(init) {
        match &declarator.name {
            Pat::Ident(binding) => {
                let binding_name = binding.id.sym.to_string();
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
            Pat::Object(object_pat) => {
                crate::runtime_clear::clear_runtime_bindings_for_object_pat_bindings(
                    object_pat,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                );
                if let Some(callee_name) =
                    crate::runtime_binding_utils::extract_runtime_callee_from_object_pat(object_pat)
                {
                    runtime_callee_bindings.insert(callee_name.clone());
                    runtime_namespace_bindings.remove(callee_name.as_str());
                }
            }
            _ => {}
        }
        return;
    }

    if let Some(namespace_name) = crate::binding::expression_ident(init) {
        if runtime_namespace_bindings.contains(namespace_name.as_str()) {
            if let Pat::Object(object_pat) = &declarator.name {
                crate::runtime_clear::clear_runtime_bindings_for_object_pat_bindings(
                    object_pat,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                );
                if let Some(callee_name) =
                    crate::runtime_binding_utils::extract_runtime_callee_from_object_pat(object_pat)
                {
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

    if crate::runtime_binding_utils::member_expr_is_runtime_namespace_c(
        init,
        runtime_namespace_bindings,
    ) {
        if let Pat::Ident(binding) = &declarator.name {
            let binding_name = binding.id.sym.to_string();
            runtime_callee_bindings.insert(binding_name.clone());
            runtime_namespace_bindings.remove(binding_name.as_str());
        }
        return;
    }

    crate::collect_runtime_bindings_from_expression(
        init,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        false,
    );
    crate::runtime_clear::clear_runtime_bindings_for_pat(
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
    crate::collect_runtime_bindings_from_expression(
        expr,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        false,
    );
}
