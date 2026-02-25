use swc_ecma_ast::Expr;

use crate::helpers::unwrap_expression;

pub(crate) fn ts_entity_name_root_name(entity_name: &swc_ecma_ast::TsEntityName) -> &str {
    match entity_name {
        swc_ecma_ast::TsEntityName::Ident(ident) => ident.sym.as_ref(),
        swc_ecma_ast::TsEntityName::TsQualifiedName(qualified_name) => {
            ts_entity_name_root_name(&qualified_name.left)
        }
    }
}

pub(crate) fn ts_entity_name_leaf_name(entity_name: &swc_ecma_ast::TsEntityName) -> &str {
    match entity_name {
        swc_ecma_ast::TsEntityName::Ident(ident) => ident.sym.as_ref(),
        swc_ecma_ast::TsEntityName::TsQualifiedName(qualified_name) => {
            qualified_name.right.sym.as_ref()
        }
    }
}

pub(crate) fn runtime_initializer_expr(expr: &Expr) -> &Expr {
    let expression = unwrap_expression(expr);
    if let Expr::Seq(sequence_expr) = expression {
        if let Some(last_expression) = sequence_expr.exprs.last() {
            return runtime_initializer_expr(last_expression.as_ref());
        }
    }
    expression
}
