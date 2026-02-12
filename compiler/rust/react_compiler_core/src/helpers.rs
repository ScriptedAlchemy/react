use swc_common::{sync::Lrc, SourceMap, Span};
use swc_ecma_ast::{BinaryOp, Expr, Lit};

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

pub(crate) fn expression_static_string_value(expr: &Expr) -> Option<String> {
    match unwrap_expression(expr) {
        Expr::Lit(literal) => match literal {
            Lit::Str(str_lit) => Some(str_lit.value.to_string()),
            Lit::Bool(bool_lit) => Some(if bool_lit.value {
                "true".to_string()
            } else {
                "false".to_string()
            }),
            Lit::Null(_) => Some("null".to_string()),
            Lit::Num(number_lit) => Some(number_lit.value.to_string()),
            Lit::BigInt(big_int_lit) => Some(big_int_lit.value.to_string()),
            _ => None,
        },
        Expr::Tpl(template_literal) => {
            if template_literal.quasis.len() != template_literal.exprs.len() + 1 {
                return None;
            }
            let mut result = String::new();
            for (index, quasi) in template_literal.quasis.iter().enumerate() {
                let quasi_segment = quasi
                    .cooked
                    .as_ref()
                    .map(|value| value.as_ref())
                    .unwrap_or_else(|| quasi.raw.as_ref());
                result.push_str(quasi_segment);
                if let Some(template_expr) = template_literal.exprs.get(index) {
                    result.push_str(expression_static_string_value(template_expr.as_ref())?.as_str());
                }
            }
            Some(result)
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::Add => {
            let left_value = expression_static_string_value(binary_expression.left.as_ref())?;
            let right_value = expression_static_string_value(binary_expression.right.as_ref())?;
            Some(format!("{left_value}{right_value}"))
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::LogicalAnd => {
            let left_truthy = expression_static_boolean_value(binary_expression.left.as_ref())?;
            if left_truthy {
                expression_static_string_value(binary_expression.right.as_ref())
            } else {
                expression_static_string_value(binary_expression.left.as_ref())
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::LogicalOr => {
            let left_truthy = expression_static_boolean_value(binary_expression.left.as_ref())?;
            if left_truthy {
                expression_static_string_value(binary_expression.left.as_ref())
            } else {
                expression_static_string_value(binary_expression.right.as_ref())
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::NullishCoalescing => {
            if expression_is_static_nullish(binary_expression.left.as_ref()) {
                expression_static_string_value(binary_expression.right.as_ref())
            } else {
                expression_static_string_value(binary_expression.left.as_ref())
            }
        }
        Expr::Seq(sequence_expression) => {
            if sequence_expression.exprs.is_empty() {
                return None;
            }
            for sequence_item in &sequence_expression.exprs[0..sequence_expression.exprs.len() - 1] {
                expression_static_string_value(sequence_item.as_ref())?;
            }
            sequence_expression
                .exprs
                .last()
                .and_then(|expression| expression_static_string_value(expression.as_ref()))
        }
        Expr::Cond(conditional_expression) => {
            let test_value = expression_static_boolean_value(conditional_expression.test.as_ref())?;
            if test_value {
                expression_static_string_value(conditional_expression.cons.as_ref())
            } else {
                expression_static_string_value(conditional_expression.alt.as_ref())
            }
        }
        _ => None,
    }
}

fn expression_static_boolean_value(expr: &Expr) -> Option<bool> {
    expression_static_truthiness_value(expr)
}

fn expression_is_static_nullish(expr: &Expr) -> bool {
    match unwrap_expression(expr) {
        Expr::Lit(Lit::Null(_)) => true,
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Void => {
            expression_static_truthiness_value(unary_expression.arg.as_ref()).is_some()
        }
        _ => false,
    }
}

fn expression_static_truthiness_value(expr: &Expr) -> Option<bool> {
    match unwrap_expression(expr) {
        Expr::Lit(literal) => match literal {
            Lit::Bool(boolean_literal) => Some(boolean_literal.value),
            Lit::Null(_) => Some(false),
            Lit::Str(string_literal) => Some(!string_literal.value.is_empty()),
            Lit::Num(number_literal) => Some(number_literal.value != 0.0 && !number_literal.value.is_nan()),
            Lit::BigInt(big_int_literal) => Some(big_int_literal.value.to_string() != "0"),
            _ => None,
        },
        Expr::Tpl(_) => expression_static_string_value(expr).map(|value| !value.is_empty()),
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Bang => {
            expression_static_truthiness_value(unary_expression.arg.as_ref()).map(|value| !value)
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Void => {
            expression_static_truthiness_value(unary_expression.arg.as_ref())?;
            Some(false)
        }
        Expr::Seq(sequence_expression) => {
            if sequence_expression.exprs.is_empty() {
                return None;
            }
            let mut last_truthiness = None;
            for sequence_item in &sequence_expression.exprs {
                last_truthiness = Some(expression_static_truthiness_value(sequence_item.as_ref())?);
            }
            last_truthiness
        }
        Expr::Cond(conditional_expression) => {
            let test_value = expression_static_truthiness_value(conditional_expression.test.as_ref())?;
            if test_value {
                expression_static_truthiness_value(conditional_expression.cons.as_ref())
            } else {
                expression_static_truthiness_value(conditional_expression.alt.as_ref())
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::LogicalAnd => {
            let left_truthy = expression_static_truthiness_value(binary_expression.left.as_ref())?;
            if left_truthy {
                expression_static_truthiness_value(binary_expression.right.as_ref())
            } else {
                Some(false)
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::LogicalOr => {
            let left_truthy = expression_static_truthiness_value(binary_expression.left.as_ref())?;
            if left_truthy {
                Some(true)
            } else {
                expression_static_truthiness_value(binary_expression.right.as_ref())
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::NullishCoalescing => {
            if expression_is_static_nullish(binary_expression.left.as_ref()) {
                expression_static_truthiness_value(binary_expression.right.as_ref())
            } else {
                expression_static_truthiness_value(binary_expression.left.as_ref())
            }
        }
        _ => None,
    }
}
