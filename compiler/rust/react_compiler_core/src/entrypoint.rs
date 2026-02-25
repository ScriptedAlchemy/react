use std::collections::{HashMap, HashSet};

use swc_ecma_ast::{
    AssignTarget, DefaultDecl, Expr, MemberExpr, MemberProp, Module, ModuleDecl,
    ModuleExportName, ModuleItem, Pat, Prop, PropName, PropOrSpread, Script, SimpleAssignTarget,
    Stmt, VarDecl,
};

use crate::{
    binding::{assign_target_ident, resolve_function_binding_name, TopLevelBinding},
    helpers::{expression_static_string_value, unwrap_expression},
};

pub(crate) fn collect_fixture_entrypoint_function_names(module: &Module) -> HashSet<String> {
    module
        .body
        .iter()
        .flat_map(|item| match item {
            ModuleItem::Stmt(stmt) => collect_fixture_entrypoint_names_from_stmt(stmt),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => match &export_decl.decl {
                swc_ecma_ast::Decl::Var(var_decl) => {
                    collect_fixture_entrypoint_names_from_var_decl(var_decl)
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        })
        .collect()
}

pub(crate) fn collect_fixture_entrypoint_function_names_in_script(script: &Script) -> HashSet<String> {
    script
        .body
        .iter()
        .flat_map(collect_fixture_entrypoint_names_from_stmt)
        .collect()
}

fn collect_fixture_entrypoint_names_from_stmt(stmt: &Stmt) -> Vec<String> {
    match stmt {
        Stmt::Expr(expr_stmt) => collect_fixture_entrypoint_names_from_expr(expr_stmt.expr.as_ref()),
        Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl)) => {
            collect_fixture_entrypoint_names_from_var_decl(var_decl)
        }
        _ => Vec::new(),
    }
}

fn collect_fixture_entrypoint_names_from_expr(expr: &Expr) -> Vec<String> {
    let Expr::Assign(assign_expr) = unwrap_expression(expr) else {
        return Vec::new();
    };
    if assign_expr.op != swc_ecma_ast::AssignOp::Assign {
        return Vec::new();
    }

    if let Some(target_name) = assign_target_ident(&assign_expr.left) {
        if target_name == "FIXTURE_ENTRYPOINT" {
            if let Expr::Object(object_literal) = unwrap_expression(assign_expr.right.as_ref()) {
                if let Some(name) = fixture_entrypoint_fn_name_from_object_literal(object_literal) {
                    return vec![name];
                }
            }
        }
    }

    if assign_target_is_fixture_entrypoint_fn(&assign_expr.left) {
        if let Some(name) = function_name_from_expr(assign_expr.right.as_ref()) {
            return vec![name];
        }
    }

    Vec::new()
}

pub(crate) fn collect_default_export_function_names(
    module: &Module,
    bindings: &HashMap<String, TopLevelBinding>,
) -> HashSet<String> {
    module
        .body
        .iter()
        .filter_map(|item| {
            let ModuleItem::ModuleDecl(module_decl) = item else {
                return None;
            };
            match module_decl {
                ModuleDecl::ExportDefaultDecl(default_decl) => match &default_decl.decl {
                    DefaultDecl::Fn(fn_expr) => {
                        fn_expr.ident.as_ref().map(|ident| ident.sym.to_string())
                    }
                    _ => None,
                },
                ModuleDecl::ExportDefaultExpr(default_expr) => {
                    match unwrap_expression(default_expr.expr.as_ref()) {
                        Expr::Ident(ident) => Some(ident.sym.to_string()),
                        Expr::Fn(fn_expr) => {
                            fn_expr.ident.as_ref().map(|ident| ident.sym.to_string())
                        }
                        _ => None,
                    }
                }
                ModuleDecl::ExportNamed(named_export)
                    if named_export.src.is_none() && !named_export.type_only =>
                {
                    named_export
                        .specifiers
                        .iter()
                        .find_map(|specifier| match specifier {
                            swc_ecma_ast::ExportSpecifier::Named(named_specifier)
                                if !named_specifier.is_type_only
                                    && named_specifier
                                        .exported
                                        .as_ref()
                                        .map(|name| name.atom() == &"default")
                                        .unwrap_or(false) =>
                            {
                                match &named_specifier.orig {
                                    ModuleExportName::Ident(ident) => Some(ident.sym.to_string()),
                                    ModuleExportName::Str(_) => None,
                                }
                            }
                            _ => None,
                        })
                }
                _ => None,
            }
        })
        .filter_map(|name| resolve_function_binding_name(bindings, &name))
        .collect()
}

pub(crate) fn module_has_default_export_component_candidate(module: &Module) -> bool {
    module.body.iter().any(|item| {
        let ModuleItem::ModuleDecl(module_decl) = item else {
            return false;
        };
        match module_decl {
            ModuleDecl::ExportDefaultDecl(default_decl) => {
                matches!(&default_decl.decl, DefaultDecl::Fn(_))
            }
            ModuleDecl::ExportDefaultExpr(default_expr) => {
                matches!(
                    unwrap_expression(default_expr.expr.as_ref()),
                    Expr::Fn(_) | Expr::Arrow(_)
                )
            }
            _ => false,
        }
    })
}

fn collect_fixture_entrypoint_names_from_var_decl(var_decl: &VarDecl) -> Vec<String> {
    var_decl
        .decls
        .iter()
        .filter_map(|declarator| {
            let Pat::Ident(binding) = &declarator.name else {
                return None;
            };
            if binding.id.sym != *"FIXTURE_ENTRYPOINT" {
                return None;
            }
            let Some(init) = declarator.init.as_ref() else {
                return None;
            };
            let Expr::Object(object_literal) = unwrap_expression(init.as_ref()) else {
                return None;
            };
            fixture_entrypoint_fn_name_from_object_literal(object_literal)
        })
        .collect()
}

fn assign_target_is_fixture_entrypoint_fn(target: &AssignTarget) -> bool {
    match target {
        AssignTarget::Simple(simple) => simple_assign_target_is_fixture_entrypoint_fn(simple),
        _ => false,
    }
}

fn simple_assign_target_is_fixture_entrypoint_fn(target: &SimpleAssignTarget) -> bool {
    match target {
        SimpleAssignTarget::Member(member_expr) => {
            member_expr_is_fixture_entrypoint_fn(member_expr)
        }
        SimpleAssignTarget::Paren(paren_expr) => {
            expression_is_fixture_entrypoint_fn_target(paren_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsAs(ts_as_expr) => {
            expression_is_fixture_entrypoint_fn_target(ts_as_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsSatisfies(ts_satisfies_expr) => {
            expression_is_fixture_entrypoint_fn_target(ts_satisfies_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsNonNull(ts_non_null_expr) => {
            expression_is_fixture_entrypoint_fn_target(ts_non_null_expr.expr.as_ref())
        }
        SimpleAssignTarget::TsTypeAssertion(ts_type_assertion) => {
            expression_is_fixture_entrypoint_fn_target(ts_type_assertion.expr.as_ref())
        }
        SimpleAssignTarget::TsInstantiation(ts_instantiation) => {
            expression_is_fixture_entrypoint_fn_target(ts_instantiation.expr.as_ref())
        }
        _ => false,
    }
}

fn expression_is_fixture_entrypoint_fn_target(expr: &Expr) -> bool {
    match unwrap_expression(expr) {
        Expr::Member(member_expr) => member_expr_is_fixture_entrypoint_fn(member_expr),
        _ => false,
    }
}

fn member_expr_is_fixture_entrypoint_fn(member_expr: &MemberExpr) -> bool {
    let object_name = match unwrap_expression(member_expr.obj.as_ref()) {
        Expr::Ident(ident) => ident.sym.as_ref(),
        _ => return false,
    };
    if object_name != "FIXTURE_ENTRYPOINT" {
        return false;
    }
    is_member_prop_with(&member_expr.prop, "fn")
}

fn function_name_from_expr(expr: &Expr) -> Option<String> {
    match unwrap_expression(expr) {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        Expr::Fn(fn_expr) => fn_expr.ident.as_ref().map(|ident| ident.sym.to_string()),
        _ => None,
    }
}

fn fixture_entrypoint_fn_name_from_object_literal(
    object_literal: &swc_ecma_ast::ObjectLit,
) -> Option<String> {
    object_literal.props.iter().find_map(|prop_or_spread| {
        let PropOrSpread::Prop(prop) = prop_or_spread else {
            return None;
        };
        match prop.as_ref() {
            Prop::KeyValue(key_value) => {
                if !is_fn_property_name(&key_value.key) {
                    return None;
                }
                function_name_from_expr(key_value.value.as_ref())
            }
            Prop::Shorthand(ident) if ident.sym == *"fn" => Some(ident.sym.to_string()),
            _ => None,
        }
    })
}

fn is_fn_property_name(name: &PropName) -> bool {
    is_prop_name_with(name, "fn")
}

fn computed_prop_name_matches_expected(expr: &Expr, expected: &str) -> bool {
    expression_static_string_value(expr)
        .map(|value| value == expected)
        .unwrap_or(false)
}

pub(crate) fn is_prop_name_with(name: &PropName, expected: &str) -> bool {
    match name {
        PropName::Ident(ident) => ident.sym == *expected,
        PropName::Str(str_lit) => str_lit.value == *expected,
        PropName::Computed(computed) => {
            computed_prop_name_matches_expected(computed.expr.as_ref(), expected)
        }
        _ => false,
    }
}

pub(crate) fn is_member_prop_with(prop: &MemberProp, expected: &str) -> bool {
    match prop {
        MemberProp::Ident(ident_name) => ident_name.sym == *expected,
        MemberProp::Computed(computed_prop) => {
            computed_prop_name_matches_expected(computed_prop.expr.as_ref(), expected)
        }
        _ => false,
    }
}
