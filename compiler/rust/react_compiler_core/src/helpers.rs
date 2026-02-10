use swc_common::{sync::Lrc, SourceMap, Span};
use swc_ecma_ast::Expr;

use crate::SourceLocation;

pub(crate) fn span_to_location(cm: &Lrc<SourceMap>, span: Span) -> Option<SourceLocation> {
    if span.is_dummy() {
        return None;
    }
    let start = cm.lookup_char_pos(span.lo());
    let end = cm.lookup_char_pos(span.hi());
    Some(SourceLocation {
        start_line: start.line,
        start_column: start.col_display,
        end_line: end.line,
        end_column: end.col_display,
    })
}

pub(crate) fn unwrap_expression(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(paren_expr) => unwrap_expression(paren_expr.expr.as_ref()),
        Expr::TsAs(ts_as_expr) => unwrap_expression(ts_as_expr.expr.as_ref()),
        Expr::TsTypeAssertion(ts_type_assertion) => {
            unwrap_expression(ts_type_assertion.expr.as_ref())
        }
        Expr::TsConstAssertion(ts_const_assertion) => {
            unwrap_expression(ts_const_assertion.expr.as_ref())
        }
        Expr::TsNonNull(ts_non_null_expr) => unwrap_expression(ts_non_null_expr.expr.as_ref()),
        Expr::TsSatisfies(ts_satisfies_expr) => unwrap_expression(ts_satisfies_expr.expr.as_ref()),
        Expr::TsInstantiation(ts_instantiation_expr) => {
            unwrap_expression(ts_instantiation_expr.expr.as_ref())
        }
        _ => expr,
    }
}

pub(crate) fn unwrap_expression_mut(expr: &mut Expr) -> &mut Expr {
    match expr {
        Expr::Paren(paren_expr) => unwrap_expression_mut(paren_expr.expr.as_mut()),
        Expr::TsAs(ts_as_expr) => unwrap_expression_mut(ts_as_expr.expr.as_mut()),
        Expr::TsTypeAssertion(ts_type_assertion) => {
            unwrap_expression_mut(ts_type_assertion.expr.as_mut())
        }
        Expr::TsConstAssertion(ts_const_assertion) => {
            unwrap_expression_mut(ts_const_assertion.expr.as_mut())
        }
        Expr::TsNonNull(ts_non_null_expr) => unwrap_expression_mut(ts_non_null_expr.expr.as_mut()),
        Expr::TsSatisfies(ts_satisfies_expr) => {
            unwrap_expression_mut(ts_satisfies_expr.expr.as_mut())
        }
        Expr::TsInstantiation(ts_instantiation_expr) => {
            unwrap_expression_mut(ts_instantiation_expr.expr.as_mut())
        }
        _ => expr,
    }
}
