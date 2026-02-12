use std::collections::HashSet;

use swc_ecma_ast::{Decl, Stmt, VarDecl};

pub(crate) fn collect_runtime_bindings_from_static_block_stmts(
    stmts: &[Stmt],
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let shadowed_bindings = crate::runtime_scope::collect_declared_binding_names_from_stmts(stmts);
    crate::runtime_scope::with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            for stmt in stmts {
                collect_runtime_bindings_from_static_block_stmt(
                    stmt,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
        },
    );
}

pub(crate) fn collect_runtime_bindings_from_static_block_stmt(
    stmt: &Stmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    match stmt {
        Stmt::Expr(expr_stmt) => crate::collect_runtime_bindings_from_expression(
            expr_stmt.expr.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Decl(Decl::Var(var_decl)) => collect_runtime_bindings_from_var_decl_in_static_block(
            var_decl,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Decl(Decl::Using(using_decl)) => {
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
        }
        Stmt::Decl(Decl::Class(class_decl)) => {
            crate::runtime_class::collect_runtime_bindings_from_class(
                &class_decl.class,
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            )
        }
        Stmt::Decl(Decl::TsEnum(ts_enum_decl)) => {
            crate::runtime_traversal::collect_runtime_bindings_from_ts_enum_decl(
                ts_enum_decl.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            )
        }
        Stmt::Decl(Decl::TsModule(ts_module_decl)) => {
            crate::runtime_traversal::collect_runtime_bindings_from_ts_module_decl(
                ts_module_decl.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            )
        }
        Stmt::If(if_stmt) => {
            crate::collect_runtime_bindings_from_expression(
                if_stmt.test.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            collect_runtime_bindings_from_static_block_stmt(
                if_stmt.cons.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
            if let Some(alt) = &if_stmt.alt {
                collect_runtime_bindings_from_static_block_stmt(
                    alt.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    true,
                );
            }
        }
        Stmt::Block(block_stmt) => collect_runtime_bindings_from_static_block_stmts(
            &block_stmt.stmts,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::For(for_stmt) => collect_runtime_bindings_from_for_stmt(
            for_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::ForIn(for_in_stmt) => collect_runtime_bindings_from_for_in_stmt(
            for_in_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::ForOf(for_of_stmt) => collect_runtime_bindings_from_for_of_stmt(
            for_of_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::While(while_stmt) => {
            crate::collect_runtime_bindings_from_expression(
                while_stmt.test.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            collect_runtime_bindings_from_static_block_stmt(
                while_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
        }
        Stmt::DoWhile(do_while_stmt) => {
            collect_runtime_bindings_from_static_block_stmt(
                do_while_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            crate::collect_runtime_bindings_from_expression(
                do_while_stmt.test.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        Stmt::Switch(switch_stmt) => collect_runtime_bindings_from_switch_stmt(
            switch_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Try(try_stmt) => collect_runtime_bindings_from_try_stmt(
            try_stmt,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Throw(throw_stmt) => crate::collect_runtime_bindings_from_expression(
            throw_stmt.arg.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::Labeled(labeled_stmt) => collect_runtime_bindings_from_static_block_stmt(
            labeled_stmt.body.as_ref(),
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        ),
        Stmt::With(with_stmt) => {
            crate::collect_runtime_bindings_from_expression(
                with_stmt.obj.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            collect_runtime_bindings_from_static_block_stmt(
                with_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
        _ => {}
    }
}

pub(crate) fn collect_runtime_bindings_from_var_decl_in_static_block(
    var_decl: &VarDecl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    for declarator in &var_decl.decls {
        if let Some(init) = declarator.init.as_deref() {
            crate::collect_runtime_bindings_from_expression(
                crate::runtime_helpers::runtime_initializer_expr(init),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
        }
    }
}

fn collect_runtime_bindings_from_for_stmt(
    for_stmt: &swc_ecma_ast::ForStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let shadowed_bindings = crate::runtime_scope::for_init_declared_binding_names(for_stmt);
    crate::runtime_scope::with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            if let Some(init) = &for_stmt.init {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(var_decl) => {
                        collect_runtime_bindings_from_var_decl_in_static_block(
                            var_decl,
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                    swc_ecma_ast::VarDeclOrExpr::Expr(expr) => {
                        crate::collect_runtime_bindings_from_expression(
                            expr.as_ref(),
                            runtime_namespace_bindings,
                            runtime_callee_bindings,
                            may_be_conditional,
                        );
                    }
                }
            }
            if let Some(test) = &for_stmt.test {
                crate::collect_runtime_bindings_from_expression(
                    test.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    may_be_conditional,
                );
            }
            if let Some(update) = &for_stmt.update {
                crate::collect_runtime_bindings_from_expression(
                    update.as_ref(),
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    true,
                );
            }
            collect_runtime_bindings_from_static_block_stmt(
                for_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
        },
    );
}

fn collect_runtime_bindings_from_for_in_stmt(
    for_in_stmt: &swc_ecma_ast::ForInStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let shadowed_bindings =
        crate::runtime_scope::for_head_declared_binding_names(&for_in_stmt.left);
    crate::runtime_scope::with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            crate::collect_runtime_bindings_from_expression(
                for_in_stmt.right.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            clear_runtime_bindings_for_for_head(
                &for_in_stmt.left,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
            collect_runtime_bindings_from_static_block_stmt(
                for_in_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
        },
    );
}

fn collect_runtime_bindings_from_for_of_stmt(
    for_of_stmt: &swc_ecma_ast::ForOfStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    let shadowed_bindings =
        crate::runtime_scope::for_head_declared_binding_names(&for_of_stmt.left);
    crate::runtime_scope::with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            crate::collect_runtime_bindings_from_expression(
                for_of_stmt.right.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                may_be_conditional,
            );
            clear_runtime_bindings_for_for_head(
                &for_of_stmt.left,
                runtime_namespace_bindings,
                runtime_callee_bindings,
            );
            collect_runtime_bindings_from_static_block_stmt(
                for_of_stmt.body.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
                true,
            );
        },
    );
}

fn collect_runtime_bindings_from_try_stmt(
    try_stmt: &swc_ecma_ast::TryStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    collect_runtime_bindings_from_static_block_stmts(
        &try_stmt.block.stmts,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );
    if let Some(handler) = &try_stmt.handler {
        let shadowed_bindings = crate::runtime_scope::catch_param_declared_binding_names(handler);
        crate::runtime_scope::with_shadowed_runtime_bindings(
            &shadowed_bindings,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            |runtime_namespace_bindings, runtime_callee_bindings| {
                collect_runtime_bindings_from_static_block_stmts(
                    &handler.body.stmts,
                    runtime_namespace_bindings,
                    runtime_callee_bindings,
                    true,
                );
            },
        );
    }
    if let Some(finalizer) = &try_stmt.finalizer {
        collect_runtime_bindings_from_static_block_stmts(
            &finalizer.stmts,
            runtime_namespace_bindings,
            runtime_callee_bindings,
            may_be_conditional,
        );
    }
}

fn collect_runtime_bindings_from_switch_stmt(
    switch_stmt: &swc_ecma_ast::SwitchStmt,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
    may_be_conditional: bool,
) {
    crate::collect_runtime_bindings_from_expression(
        switch_stmt.discriminant.as_ref(),
        runtime_namespace_bindings,
        runtime_callee_bindings,
        may_be_conditional,
    );

    let mut shadowed_bindings = Vec::new();
    for case in &switch_stmt.cases {
        for stmt in &case.cons {
            crate::runtime_scope::collect_declared_binding_names_from_stmt(
                stmt,
                &mut shadowed_bindings,
            );
        }
    }
    shadowed_bindings.sort();
    shadowed_bindings.dedup();

    crate::runtime_scope::with_shadowed_runtime_bindings(
        &shadowed_bindings,
        runtime_namespace_bindings,
        runtime_callee_bindings,
        |runtime_namespace_bindings, runtime_callee_bindings| {
            for case in &switch_stmt.cases {
                if let Some(test) = &case.test {
                    crate::collect_runtime_bindings_from_expression(
                        test.as_ref(),
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        true,
                    );
                }
                for stmt in &case.cons {
                    collect_runtime_bindings_from_static_block_stmt(
                        stmt,
                        runtime_namespace_bindings,
                        runtime_callee_bindings,
                        true,
                    );
                }
            }
        },
    );
}

fn clear_runtime_bindings_for_for_head(
    for_head: &swc_ecma_ast::ForHead,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    match for_head {
        swc_ecma_ast::ForHead::Pat(pattern) => {
            crate::runtime_clear::clear_runtime_bindings_for_pat(
                pattern.as_ref(),
                runtime_namespace_bindings,
                runtime_callee_bindings,
            )
        }
        swc_ecma_ast::ForHead::VarDecl(_) | swc_ecma_ast::ForHead::UsingDecl(_) => {}
    }
}
