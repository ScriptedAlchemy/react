use std::collections::HashSet;

use swc_ecma_ast::{Decl, DefaultDecl, ImportDecl, ImportSpecifier, Module, ModuleDecl, ModuleItem, Script, Stmt};

#[derive(Debug, Default)]
pub(crate) struct RuntimeMemoCalleeScan {
    pub(crate) runtime_namespace_bindings: HashSet<String>,
    pub(crate) runtime_callee_bindings: HashSet<String>,
}

pub(crate) fn select_runtime_callee_name(
    runtime_callee_bindings: &HashSet<String>,
) -> Option<String> {
    sorted_runtime_callee_candidates(runtime_callee_bindings)
        .into_iter()
        .next()
}

pub(crate) fn sorted_runtime_callee_candidates(
    runtime_callee_bindings: &HashSet<String>,
) -> Vec<String> {
    let mut candidates: Vec<String> = runtime_callee_bindings.iter().cloned().collect();
    candidates.sort();
    candidates
}

pub(crate) fn sorted_runtime_namespace_candidates(
    runtime_namespace_bindings: &HashSet<String>,
) -> Vec<String> {
    let mut candidates: Vec<String> = runtime_namespace_bindings.iter().cloned().collect();
    candidates.sort();
    candidates
}

pub(crate) fn collect_runtime_bindings_from_import_decl(
    import_decl: &ImportDecl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    for specifier in &import_decl.specifiers {
        match specifier {
            ImportSpecifier::Named(named) => {
                if named.is_type_only {
                    continue;
                }
                let is_memo_runtime_import = named
                    .imported
                    .as_ref()
                    .map(|imported| imported.atom() == &"c")
                    .unwrap_or(named.local.sym == *"c");
                if is_memo_runtime_import {
                    let binding_name = named.local.sym.to_string();
                    runtime_callee_bindings.insert(binding_name.clone());
                    runtime_namespace_bindings.remove(binding_name.as_str());
                }
            }
            ImportSpecifier::Namespace(namespace) => {
                let binding_name = namespace.local.sym.to_string();
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
            ImportSpecifier::Default(default_import) => {
                let binding_name = default_import.local.sym.to_string();
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
        }
    }
}

pub(crate) fn runtime_memo_callee_scan_for_module(module: &Module) -> RuntimeMemoCalleeScan {
    let mut runtime_namespace_bindings: HashSet<String> = HashSet::new();
    let mut runtime_callee_bindings: HashSet<String> = HashSet::new();
    for item in &module.body {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) => {
                if import_decl.src.value != *"react/compiler-runtime" {
                    continue;
                }
                collect_runtime_bindings_from_import_decl(
                    import_decl,
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
                if let Decl::Var(var_decl) = &export_decl.decl {
                    for declarator in &var_decl.decls {
                        crate::runtime_traversal::collect_runtime_bindings_from_script_declarator(
                            declarator,
                            &mut runtime_namespace_bindings,
                            &mut runtime_callee_bindings,
                        );
                    }
                } else if let Decl::Class(class_decl) = &export_decl.decl {
                    crate::runtime_class::collect_runtime_bindings_from_class(
                        &class_decl.class,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                        false,
                    );
                } else if let Decl::Using(using_decl) = &export_decl.decl {
                    for declarator in &using_decl.decls {
                        crate::runtime_traversal::collect_runtime_bindings_from_script_declarator(
                            declarator,
                            &mut runtime_namespace_bindings,
                            &mut runtime_callee_bindings,
                        );
                    }
                } else if let Decl::TsEnum(ts_enum_decl) = &export_decl.decl {
                    crate::runtime_traversal::collect_runtime_bindings_from_ts_enum_decl(
                        ts_enum_decl.as_ref(),
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                        false,
                    );
                } else if let Decl::TsModule(ts_module_decl) = &export_decl.decl {
                    crate::runtime_traversal::collect_runtime_bindings_from_ts_module_decl(
                        ts_module_decl.as_ref(),
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                        false,
                    );
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(default_decl)) => {
                if let DefaultDecl::Class(class_expr) = &default_decl.decl {
                    crate::runtime_class::collect_runtime_bindings_from_class(
                        &class_expr.class,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                        false,
                    );
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(default_expr)) => {
                crate::runtime_traversal::collect_runtime_bindings_from_script_assignment_expr(
                    default_expr.expr.as_ref(),
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::ModuleDecl(ModuleDecl::TsExportAssignment(export_assignment)) => {
                crate::runtime_traversal::collect_runtime_bindings_from_script_assignment_expr(
                    export_assignment.expr.as_ref(),
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::ModuleDecl(ModuleDecl::TsImportEquals(import_equals_decl)) => {
                crate::runtime_traversal::collect_runtime_bindings_from_ts_import_equals_decl(
                    import_equals_decl.as_ref(),
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) => {
                for declarator in &var_decl.decls {
                    crate::runtime_traversal::collect_runtime_bindings_from_script_declarator(
                        declarator,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                    );
                }
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Class(class_decl))) => {
                crate::runtime_class::collect_runtime_bindings_from_class(
                    &class_decl.class,
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                    false,
                );
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Using(using_decl))) => {
                for declarator in &using_decl.decls {
                    crate::runtime_traversal::collect_runtime_bindings_from_script_declarator(
                        declarator,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                    );
                }
            }
            ModuleItem::Stmt(Stmt::Expr(expr_stmt)) => {
                crate::runtime_traversal::collect_runtime_bindings_from_script_assignment_expr(
                    expr_stmt.expr.as_ref(),
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                );
            }
            ModuleItem::Stmt(stmt) => crate::runtime_stmt::collect_runtime_bindings_from_static_block_stmt(
                stmt,
                &mut runtime_namespace_bindings,
                &mut runtime_callee_bindings,
                false,
            ),
            _ => {}
        }
    }
    RuntimeMemoCalleeScan {
        runtime_namespace_bindings,
        runtime_callee_bindings,
    }
}

pub(crate) fn runtime_memo_callee_name(module: &Module) -> Option<String> {
    let runtime_scan = runtime_memo_callee_scan_for_module(module);
    select_runtime_callee_name(&runtime_scan.runtime_callee_bindings)
}

pub(crate) fn runtime_memo_callee_scan_for_script(script: &Script) -> RuntimeMemoCalleeScan {
    let mut runtime_namespace_bindings: HashSet<String> = HashSet::new();
    let mut runtime_callee_bindings: HashSet<String> = HashSet::new();
    for stmt in &script.body {
        match stmt {
            Stmt::Decl(Decl::Var(var_decl)) => {
                for declarator in &var_decl.decls {
                    crate::runtime_traversal::collect_runtime_bindings_from_script_declarator(
                        declarator,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                    );
                }
            }
            Stmt::Decl(Decl::Class(class_decl)) => {
                crate::runtime_class::collect_runtime_bindings_from_class(
                    &class_decl.class,
                    &mut runtime_namespace_bindings,
                    &mut runtime_callee_bindings,
                    false,
                );
            }
            Stmt::Decl(Decl::Using(using_decl)) => {
                for declarator in &using_decl.decls {
                    crate::runtime_traversal::collect_runtime_bindings_from_script_declarator(
                        declarator,
                        &mut runtime_namespace_bindings,
                        &mut runtime_callee_bindings,
                    );
                }
            }
            Stmt::Expr(expr_stmt) => crate::runtime_traversal::collect_runtime_bindings_from_script_assignment_expr(
                expr_stmt.expr.as_ref(),
                &mut runtime_namespace_bindings,
                &mut runtime_callee_bindings,
            ),
            stmt => crate::runtime_stmt::collect_runtime_bindings_from_static_block_stmt(
                stmt,
                &mut runtime_namespace_bindings,
                &mut runtime_callee_bindings,
                false,
            ),
        }
    }
    RuntimeMemoCalleeScan {
        runtime_namespace_bindings,
        runtime_callee_bindings,
    }
}

pub(crate) fn runtime_memo_callee_name_in_script(script: &Script) -> Option<String> {
    let runtime_scan = runtime_memo_callee_scan_for_script(script);
    select_runtime_callee_name(&runtime_scan.runtime_callee_bindings)
}
