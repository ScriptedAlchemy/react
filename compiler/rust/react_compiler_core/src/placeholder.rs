use swc_common::DUMMY_SP;
use swc_ecma_ast::{
    BindingIdent, BlockStmt, BlockStmtOrExpr, Callee, Decl, Expr, ExprOrSpread, Ident, ImportDecl,
    ImportNamedSpecifier, ImportSpecifier, Lit, Module, ModuleDecl, ModuleExportName, ModuleItem,
    Number, Pat, Stmt, VarDecl, VarDeclKind, VarDeclarator,
};

pub(crate) fn inject_placeholder_memo_init_into_function(
    function: &mut swc_ecma_ast::Function,
    runtime_callee_name: &str,
) -> bool {
    if function_has_placeholder_memo_init(function, runtime_callee_name) {
        return false;
    }
    let memo_stmt = make_placeholder_memo_stmt(runtime_callee_name);
    match function.body.as_mut() {
        Some(body) => {
            let insertion_index = first_non_directive_stmt_index(&body.stmts);
            body.stmts.insert(insertion_index, memo_stmt);
        }
        None => {
            function.body = Some(BlockStmt {
                span: DUMMY_SP,
                ctxt: Default::default(),
                stmts: vec![memo_stmt],
            });
        }
    }
    true
}

pub(crate) fn inject_placeholder_memo_init_into_arrow_function(
    arrow: &mut swc_ecma_ast::ArrowExpr,
    runtime_callee_name: &str,
) -> bool {
    if arrow_has_placeholder_memo_init(arrow, runtime_callee_name) {
        return false;
    }
    let memo_stmt = make_placeholder_memo_stmt(runtime_callee_name);
    match arrow.body.as_mut() {
        BlockStmtOrExpr::BlockStmt(block) => {
            let insertion_index = first_non_directive_stmt_index(&block.stmts);
            block.stmts.insert(insertion_index, memo_stmt);
        }
        BlockStmtOrExpr::Expr(expr) => {
            let return_stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
                span: DUMMY_SP,
                arg: Some(expr.clone()),
            });
            arrow.body = Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
                span: DUMMY_SP,
                ctxt: Default::default(),
                stmts: vec![memo_stmt, return_stmt],
            }));
        }
    }
    true
}

fn make_placeholder_memo_stmt(runtime_callee_name: &str) -> Stmt {
    Stmt::Decl(Decl::Var(Box::new(VarDecl {
        span: DUMMY_SP,
        ctxt: Default::default(),
        kind: VarDeclKind::Const,
        declare: false,
        decls: vec![VarDeclarator {
            span: DUMMY_SP,
            name: Pat::Ident(BindingIdent::from(Ident::new_no_ctxt("$".into(), DUMMY_SP))),
            init: Some(Box::new(Expr::Call(swc_ecma_ast::CallExpr {
                span: DUMMY_SP,
                ctxt: Default::default(),
                callee: Callee::Expr(Box::new(Expr::Ident(Ident::new_no_ctxt(
                    runtime_callee_name.into(),
                    DUMMY_SP,
                )))),
                args: vec![ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Lit(Lit::Num(Number {
                        span: DUMMY_SP,
                        value: 0.0,
                        raw: None,
                    }))),
                }],
                type_args: None,
            }))),
            definite: false,
        }],
    })))
}

fn function_has_placeholder_memo_init(
    function: &swc_ecma_ast::Function,
    runtime_callee_name: &str,
) -> bool {
    function
        .body
        .as_ref()
        .and_then(|body| {
            body.stmts
                .get(first_non_directive_stmt_index(&body.stmts))
        })
        .map(|stmt| stmt_is_placeholder_memo_init(stmt, runtime_callee_name))
        .unwrap_or(false)
}

fn arrow_has_placeholder_memo_init(
    arrow: &swc_ecma_ast::ArrowExpr,
    runtime_callee_name: &str,
) -> bool {
    let BlockStmtOrExpr::BlockStmt(block) = arrow.body.as_ref() else {
        return false;
    };
    block
        .stmts
        .get(first_non_directive_stmt_index(&block.stmts))
        .map(|stmt| stmt_is_placeholder_memo_init(stmt, runtime_callee_name))
        .unwrap_or(false)
}

fn first_non_directive_stmt_index(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .position(|stmt| !stmt_is_directive_prologue(stmt))
        .unwrap_or(stmts.len())
}

fn stmt_is_directive_prologue(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::Expr(expr_stmt)
            if matches!(
                expr_stmt.expr.as_ref(),
                Expr::Lit(Lit::Str(_))
            )
    )
}

fn stmt_is_placeholder_memo_init(stmt: &Stmt, runtime_callee_name: &str) -> bool {
    let Stmt::Decl(Decl::Var(var_decl)) = stmt else {
        return false;
    };
    if var_decl.decls.len() != 1 {
        return false;
    }
    let Some(declarator) = var_decl.decls.first() else {
        return false;
    };
    let Pat::Ident(binding) = &declarator.name else {
        return false;
    };
    if binding.id.sym != *"$" {
        return false;
    }

    let Some(init) = declarator.init.as_ref() else {
        return false;
    };
    let Expr::Call(call_expr) = init.as_ref() else {
        return false;
    };

    let Callee::Expr(callee_expr) = &call_expr.callee else {
        return false;
    };
    let Expr::Ident(callee_ident) = callee_expr.as_ref() else {
        return false;
    };
    if callee_ident.sym != *runtime_callee_name {
        return false;
    }

    if call_expr.args.len() != 1 {
        return false;
    }
    let Some(first_arg) = call_expr.args.first() else {
        return false;
    };
    let Expr::Lit(Lit::Num(number_literal)) = first_arg.expr.as_ref() else {
        return false;
    };
    number_literal.value.is_finite() && number_literal.value >= 0.0
}

pub(crate) fn make_runtime_import_decl() -> ImportDecl {
    ImportDecl {
        span: DUMMY_SP,
        specifiers: vec![ImportSpecifier::Named(ImportNamedSpecifier {
            span: DUMMY_SP,
            local: Ident::new_no_ctxt("_c".into(), DUMMY_SP),
            imported: Some(ModuleExportName::Ident(Ident::new_no_ctxt(
                "c".into(),
                DUMMY_SP,
            ))),
            is_type_only: false,
        })],
        src: Box::new("react/compiler-runtime".into()),
        type_only: false,
        with: None,
        phase: Default::default(),
    }
}

pub(crate) fn count_runtime_helper_imports(module: &Module) -> usize {
    module
        .body
        .iter()
        .filter_map(|item| match item {
            ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl))
                if import_decl.src.value == *"react/compiler-runtime" =>
            {
                Some(import_decl)
            }
            _ => None,
        })
        .map(|import_decl| {
            import_decl
                .specifiers
                .iter()
                .filter(|specifier| match specifier {
                    ImportSpecifier::Named(named) if !named.is_type_only => named
                        .imported
                        .as_ref()
                        .map(|imported| imported.atom() == &"c")
                        .unwrap_or(named.local.sym == *"c"),
                    _ => false,
                })
                .count()
        })
        .sum()
}
