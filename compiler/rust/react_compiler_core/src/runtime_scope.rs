use std::collections::HashSet;

use swc_ecma_ast::{Decl, DefaultDecl, ImportSpecifier, ModuleDecl, ModuleItem, Stmt};

use crate::runtime_binding_utils::collect_binding_names_from_pat_into;

pub(crate) fn with_shadowed_runtime_bindings<F>(
    shadowed_bindings: &[String],
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    scan: F,
) where
    F: FnOnce(&mut HashSet<String>, &mut HashSet<String>),
{
    let mut snapshots: Vec<(String, bool, bool)> = Vec::new();
    for binding_name in shadowed_bindings {
        let was_runtime_namespace = runtime_namespace_bindings.remove(binding_name.as_str());
        let was_runtime_callee = runtime_callee_bindings.remove(binding_name.as_str());
        snapshots.push((
            binding_name.clone(),
            was_runtime_namespace,
            was_runtime_callee,
        ));
    }

    scan(runtime_namespace_bindings, runtime_callee_bindings);

    for (binding_name, was_runtime_namespace, was_runtime_callee) in snapshots {
        runtime_namespace_bindings.remove(binding_name.as_str());
        runtime_callee_bindings.remove(binding_name.as_str());
        if was_runtime_namespace {
            runtime_namespace_bindings.insert(binding_name.clone());
        }
        if was_runtime_callee {
            runtime_callee_bindings.insert(binding_name);
        }
    }
}

pub(crate) fn collect_declared_binding_names_from_stmts(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in stmts {
        collect_declared_binding_names_from_stmt(stmt, &mut names);
    }
    sort_and_dedup_names(&mut names);
    names
}

pub(crate) fn collect_declared_binding_names_from_stmt(stmt: &Stmt, names: &mut Vec<String>) {
    if let Stmt::Decl(decl) = stmt {
        collect_declared_binding_names_from_decl(decl, names);
    }
}

pub(crate) fn collect_declared_binding_names_from_decl(decl: &Decl, names: &mut Vec<String>) {
    match decl {
        Decl::Var(var_decl) => {
            for declarator in &var_decl.decls {
                collect_binding_names_from_pat_into(&declarator.name, names);
            }
        }
        Decl::Using(using_decl) => {
            for declarator in &using_decl.decls {
                collect_binding_names_from_pat_into(&declarator.name, names);
            }
        }
        Decl::Class(class_decl) => names.push(class_decl.ident.sym.to_string()),
        Decl::Fn(fn_decl) => names.push(fn_decl.ident.sym.to_string()),
        Decl::TsEnum(enum_decl) => names.push(enum_decl.id.sym.to_string()),
        Decl::TsModule(module_decl) => match &module_decl.id {
            swc_ecma_ast::TsModuleName::Ident(ident) => names.push(ident.sym.to_string()),
            swc_ecma_ast::TsModuleName::Str(_) => {}
        },
        _ => {}
    }
}

pub(crate) fn collect_declared_binding_names_from_module_items(items: &[ModuleItem]) -> Vec<String> {
    let mut names = Vec::new();
    for item in items {
        collect_declared_binding_names_from_module_item(item, &mut names);
    }
    sort_and_dedup_names(&mut names);
    names
}

pub(crate) fn collect_declared_binding_names_from_module_item(
    item: &ModuleItem,
    names: &mut Vec<String>,
) {
    match item {
        ModuleItem::Stmt(stmt) => collect_declared_binding_names_from_stmt(stmt, names),
        ModuleItem::ModuleDecl(module_decl) => match module_decl {
            ModuleDecl::Import(import_decl) => {
                for specifier in &import_decl.specifiers {
                    match specifier {
                        ImportSpecifier::Named(named) => names.push(named.local.sym.to_string()),
                        ImportSpecifier::Default(default_import) => {
                            names.push(default_import.local.sym.to_string())
                        }
                        ImportSpecifier::Namespace(namespace_import) => {
                            names.push(namespace_import.local.sym.to_string())
                        }
                    }
                }
            }
            ModuleDecl::ExportDecl(export_decl) => {
                collect_declared_binding_names_from_decl(&export_decl.decl, names)
            }
            ModuleDecl::ExportDefaultDecl(default_decl) => match &default_decl.decl {
                DefaultDecl::Class(class_expr) => {
                    if let Some(ident) = &class_expr.ident {
                        names.push(ident.sym.to_string());
                    }
                }
                DefaultDecl::Fn(fn_expr) => {
                    if let Some(ident) = &fn_expr.ident {
                        names.push(ident.sym.to_string());
                    }
                }
                _ => {}
            },
            ModuleDecl::TsImportEquals(import_equals_decl) => {
                names.push(import_equals_decl.id.sym.to_string())
            }
            _ => {}
        },
    }
}

pub(crate) fn for_init_declared_binding_names(for_stmt: &swc_ecma_ast::ForStmt) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(var_decl)) = &for_stmt.init {
        for declarator in &var_decl.decls {
            collect_binding_names_from_pat_into(&declarator.name, &mut names);
        }
    }
    sort_and_dedup_names(&mut names);
    names
}

pub(crate) fn for_head_declared_binding_names(for_head: &swc_ecma_ast::ForHead) -> Vec<String> {
    let mut names = Vec::new();
    match for_head {
        swc_ecma_ast::ForHead::VarDecl(var_decl) => {
            for declarator in &var_decl.decls {
                collect_binding_names_from_pat_into(&declarator.name, &mut names);
            }
        }
        swc_ecma_ast::ForHead::UsingDecl(using_decl) => {
            for declarator in &using_decl.decls {
                collect_binding_names_from_pat_into(&declarator.name, &mut names);
            }
        }
        swc_ecma_ast::ForHead::Pat(_) => {}
    }
    sort_and_dedup_names(&mut names);
    names
}

pub(crate) fn catch_param_declared_binding_names(
    catch_clause: &swc_ecma_ast::CatchClause,
) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(param) = &catch_clause.param {
        collect_binding_names_from_pat_into(param, &mut names);
    }
    sort_and_dedup_names(&mut names);
    names
}

fn sort_and_dedup_names(names: &mut Vec<String>) {
    names.sort();
    names.dedup();
}
