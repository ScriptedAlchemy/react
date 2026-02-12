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
            Lit::Regex(_) => Some(true),
            _ => None,
        },
        Expr::Array(_) => Some(true),
        Expr::Object(_) => Some(true),
        Expr::Fn(_) => Some(true),
        Expr::Arrow(_) => Some(true),
        Expr::Class(_) => Some(true),
        Expr::Tpl(_) => expression_static_string_value(expr).map(|value| !value.is_empty()),
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Bang => {
            expression_static_truthiness_value(unary_expression.arg.as_ref()).map(|value| !value)
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Void => {
            expression_static_truthiness_value(unary_expression.arg.as_ref())?;
            Some(false)
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Plus => {
            let number_value = expression_static_number_value(unary_expression.arg.as_ref())?;
            Some(number_value != 0.0 && !number_value.is_nan())
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Minus => {
            let number_value = expression_static_number_value(unary_expression.arg.as_ref())?;
            let negated_value = -number_value;
            Some(negated_value != 0.0 && !negated_value.is_nan())
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
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::EqEqEq => {
            Some(static_strict_equality(
                expression_static_primitive_value(binary_expression.left.as_ref())?,
                expression_static_primitive_value(binary_expression.right.as_ref())?,
            ))
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::EqEq => Some(
            static_abstract_equality(
                expression_static_primitive_value(binary_expression.left.as_ref())?,
                expression_static_primitive_value(binary_expression.right.as_ref())?,
            )?,
        ),
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::NotEqEq => {
            Some(!static_strict_equality(
                expression_static_primitive_value(binary_expression.left.as_ref())?,
                expression_static_primitive_value(binary_expression.right.as_ref())?,
            ))
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::NotEq => Some(
            !static_abstract_equality(
                expression_static_primitive_value(binary_expression.left.as_ref())?,
                expression_static_primitive_value(binary_expression.right.as_ref())?,
            )?,
        ),
        Expr::Bin(binary_expression)
            if matches!(
                binary_expression.op,
                BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq
            ) =>
        {
            static_relational_comparison(
                expression_static_primitive_value(binary_expression.left.as_ref())?,
                expression_static_primitive_value(binary_expression.right.as_ref())?,
                binary_expression.op,
            )
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

fn expression_static_number_value(expr: &Expr) -> Option<f64> {
    match unwrap_expression(expr) {
        Expr::Lit(Lit::Num(number_literal)) => Some(number_literal.value),
        Expr::Lit(Lit::Bool(boolean_literal)) => Some(if boolean_literal.value { 1.0 } else { 0.0 }),
        Expr::Lit(Lit::Null(_)) => Some(0.0),
        Expr::Lit(Lit::Str(string_literal)) => parse_js_numeric_string(string_literal.value.as_ref()),
        Expr::Tpl(_) => parse_js_numeric_string(expression_static_string_value(expr)?.as_str()),
        Expr::Seq(sequence_expression) => {
            if sequence_expression.exprs.is_empty() {
                return None;
            }
            for sequence_item in &sequence_expression.exprs[0..sequence_expression.exprs.len() - 1] {
                expression_static_number_value(sequence_item.as_ref())?;
            }
            sequence_expression
                .exprs
                .last()
                .and_then(|expression| expression_static_number_value(expression.as_ref()))
        }
        Expr::Cond(conditional_expression) => {
            let test_truthy = expression_static_truthiness_value(conditional_expression.test.as_ref())?;
            if test_truthy {
                expression_static_number_value(conditional_expression.cons.as_ref())
            } else {
                expression_static_number_value(conditional_expression.alt.as_ref())
            }
        }
        Expr::Bin(binary_expression) => {
            let left = expression_static_number_value(binary_expression.left.as_ref())?;
            let right = expression_static_number_value(binary_expression.right.as_ref())?;
            match binary_expression.op {
                BinaryOp::Add => Some(left + right),
                BinaryOp::Sub => Some(left - right),
                BinaryOp::Mul => Some(left * right),
                BinaryOp::Div => Some(left / right),
                BinaryOp::Mod => Some(left % right),
                BinaryOp::Exp => Some(left.powf(right)),
                BinaryOp::LogicalAnd => {
                    if expression_static_truthiness_value(binary_expression.left.as_ref())? {
                        Some(right)
                    } else {
                        Some(left)
                    }
                }
                BinaryOp::LogicalOr => {
                    if expression_static_truthiness_value(binary_expression.left.as_ref())? {
                        Some(left)
                    } else {
                        Some(right)
                    }
                }
                BinaryOp::NullishCoalescing => {
                    if expression_is_static_nullish(binary_expression.left.as_ref()) {
                        Some(right)
                    } else {
                        Some(left)
                    }
                }
                _ => None,
            }
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Plus => {
            expression_static_number_value(unary_expression.arg.as_ref())
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Minus => {
            expression_static_number_value(unary_expression.arg.as_ref()).map(|value| -value)
        }
        _ => None,
    }
}

fn parse_js_numeric_string(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    match trimmed {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return Some(
            u64::from_str_radix(hex, 16)
                .ok()
                .map(|parsed| parsed as f64)
                .unwrap_or(f64::NAN),
        );
    }
    if let Some(octal) = trimmed
        .strip_prefix("0o")
        .or_else(|| trimmed.strip_prefix("0O"))
    {
        return Some(
            u64::from_str_radix(octal, 8)
                .ok()
                .map(|parsed| parsed as f64)
                .unwrap_or(f64::NAN),
        );
    }
    if let Some(binary) = trimmed
        .strip_prefix("0b")
        .or_else(|| trimmed.strip_prefix("0B"))
    {
        return Some(
            u64::from_str_radix(binary, 2)
                .ok()
                .map(|parsed| parsed as f64)
                .unwrap_or(f64::NAN),
        );
    }
    Some(trimmed.parse::<f64>().unwrap_or(f64::NAN))
}

enum StaticPrimitive {
    Bool(bool),
    Number(f64),
    String(String),
    Null,
    Undefined,
    BigInt(String),
}

fn expression_static_primitive_value(expr: &Expr) -> Option<StaticPrimitive> {
    match unwrap_expression(expr) {
        Expr::Lit(literal) => match literal {
            Lit::Bool(boolean_literal) => Some(StaticPrimitive::Bool(boolean_literal.value)),
            Lit::Num(number_literal) => Some(StaticPrimitive::Number(number_literal.value)),
            Lit::Str(string_literal) => Some(StaticPrimitive::String(string_literal.value.to_string())),
            Lit::Null(_) => Some(StaticPrimitive::Null),
            Lit::BigInt(big_int_literal) => Some(StaticPrimitive::BigInt(big_int_literal.value.to_string())),
            _ => None,
        },
        Expr::Tpl(_) => Some(StaticPrimitive::String(expression_static_string_value(expr)?)),
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::TypeOf => {
            Some(StaticPrimitive::String(expression_static_typeof_value(
                unary_expression.arg.as_ref(),
            )?))
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Bang => {
            Some(StaticPrimitive::Bool(
                !expression_static_truthiness_value(unary_expression.arg.as_ref())?,
            ))
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Void => {
            expression_static_truthiness_value(unary_expression.arg.as_ref())?;
            Some(StaticPrimitive::Undefined)
        }
        Expr::Unary(unary_expression)
            if unary_expression.op == swc_ecma_ast::UnaryOp::Plus
                || unary_expression.op == swc_ecma_ast::UnaryOp::Minus =>
        {
            Some(StaticPrimitive::Number(expression_static_number_value(expr)?))
        }
        Expr::Seq(sequence_expression) => {
            if sequence_expression.exprs.is_empty() {
                return None;
            }
            for sequence_item in &sequence_expression.exprs[0..sequence_expression.exprs.len() - 1] {
                expression_static_primitive_value(sequence_item.as_ref())?;
            }
            sequence_expression
                .exprs
                .last()
                .and_then(|last_expr| expression_static_primitive_value(last_expr.as_ref()))
        }
        Expr::Cond(conditional_expression) => {
            let test_truthy = expression_static_truthiness_value(conditional_expression.test.as_ref())?;
            if test_truthy {
                expression_static_primitive_value(conditional_expression.cons.as_ref())
            } else {
                expression_static_primitive_value(conditional_expression.alt.as_ref())
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::LogicalAnd => {
            let left_truthy = expression_static_truthiness_value(binary_expression.left.as_ref())?;
            if left_truthy {
                expression_static_primitive_value(binary_expression.right.as_ref())
            } else {
                expression_static_primitive_value(binary_expression.left.as_ref())
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::LogicalOr => {
            let left_truthy = expression_static_truthiness_value(binary_expression.left.as_ref())?;
            if left_truthy {
                expression_static_primitive_value(binary_expression.left.as_ref())
            } else {
                expression_static_primitive_value(binary_expression.right.as_ref())
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::NullishCoalescing => {
            if expression_is_static_nullish(binary_expression.left.as_ref()) {
                expression_static_primitive_value(binary_expression.right.as_ref())
            } else {
                expression_static_primitive_value(binary_expression.left.as_ref())
            }
        }
        _ => None,
    }
}

fn expression_static_typeof_value(expr: &Expr) -> Option<String> {
    match unwrap_expression(expr) {
        Expr::Lit(literal) => match literal {
            Lit::Bool(_) => Some("boolean".to_string()),
            Lit::Null(_) => Some("object".to_string()),
            Lit::Str(_) => Some("string".to_string()),
            Lit::Num(_) => Some("number".to_string()),
            Lit::BigInt(_) => Some("bigint".to_string()),
            Lit::Regex(_) => Some("object".to_string()),
            _ => None,
        },
        Expr::Array(_) => Some("object".to_string()),
        Expr::Object(_) => Some("object".to_string()),
        Expr::Fn(_) => Some("function".to_string()),
        Expr::Arrow(_) => Some("function".to_string()),
        Expr::Class(_) => Some("function".to_string()),
        Expr::Tpl(_) => {
            expression_static_string_value(expr)?;
            Some("string".to_string())
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Void => {
            expression_static_truthiness_value(unary_expression.arg.as_ref())?;
            Some("undefined".to_string())
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::Bang => {
            expression_static_truthiness_value(unary_expression.arg.as_ref())?;
            Some("boolean".to_string())
        }
        Expr::Unary(unary_expression)
            if unary_expression.op == swc_ecma_ast::UnaryOp::Plus
                || unary_expression.op == swc_ecma_ast::UnaryOp::Minus =>
        {
            expression_static_number_value(unary_expression.arg.as_ref())?;
            Some("number".to_string())
        }
        Expr::Unary(unary_expression) if unary_expression.op == swc_ecma_ast::UnaryOp::TypeOf => {
            expression_static_typeof_value(unary_expression.arg.as_ref())?;
            Some("string".to_string())
        }
        Expr::Seq(sequence_expression) => {
            if sequence_expression.exprs.is_empty() {
                return None;
            }
            for sequence_item in &sequence_expression.exprs[0..sequence_expression.exprs.len() - 1] {
                expression_static_typeof_value(sequence_item.as_ref())?;
            }
            sequence_expression
                .exprs
                .last()
                .and_then(|last_expr| expression_static_typeof_value(last_expr.as_ref()))
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::LogicalAnd => {
            let left_truthy = expression_static_truthiness_value(binary_expression.left.as_ref())?;
            if left_truthy {
                expression_static_typeof_value(binary_expression.right.as_ref())
            } else {
                expression_static_typeof_value(binary_expression.left.as_ref())
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::LogicalOr => {
            let left_truthy = expression_static_truthiness_value(binary_expression.left.as_ref())?;
            if left_truthy {
                expression_static_typeof_value(binary_expression.left.as_ref())
            } else {
                expression_static_typeof_value(binary_expression.right.as_ref())
            }
        }
        Expr::Bin(binary_expression) if binary_expression.op == BinaryOp::NullishCoalescing => {
            if expression_is_static_nullish(binary_expression.left.as_ref()) {
                expression_static_typeof_value(binary_expression.right.as_ref())
            } else {
                expression_static_typeof_value(binary_expression.left.as_ref())
            }
        }
        Expr::Bin(binary_expression) => static_binary_expression_typeof(binary_expression),
        Expr::Cond(conditional_expression) => {
            let test_truthy = expression_static_truthiness_value(conditional_expression.test.as_ref())?;
            if test_truthy {
                expression_static_typeof_value(conditional_expression.cons.as_ref())
            } else {
                expression_static_typeof_value(conditional_expression.alt.as_ref())
            }
        }
        _ => None,
    }
}

fn static_binary_expression_typeof(binary_expression: &swc_ecma_ast::BinExpr) -> Option<String> {
    let left = expression_static_primitive_value(binary_expression.left.as_ref())?;
    let right = expression_static_primitive_value(binary_expression.right.as_ref())?;
    match binary_expression.op {
        BinaryOp::Add => {
            if matches!(left, StaticPrimitive::String(_)) || matches!(right, StaticPrimitive::String(_)) {
                return Some("string".to_string());
            }
            if matches!(left, StaticPrimitive::BigInt(_)) || matches!(right, StaticPrimitive::BigInt(_)) {
                if matches!((&left, &right), (StaticPrimitive::BigInt(_), StaticPrimitive::BigInt(_))) {
                    return Some("bigint".to_string());
                }
                return None;
            }
            static_primitive_to_number(left)?;
            static_primitive_to_number(right)?;
            Some("number".to_string())
        }
        BinaryOp::Sub
        | BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::Mod
        | BinaryOp::Exp
        | BinaryOp::BitAnd
        | BinaryOp::BitOr
        | BinaryOp::BitXor
        | BinaryOp::LShift
        | BinaryOp::RShift => {
            if matches!(left, StaticPrimitive::BigInt(_)) || matches!(right, StaticPrimitive::BigInt(_)) {
                if matches!((&left, &right), (StaticPrimitive::BigInt(_), StaticPrimitive::BigInt(_))) {
                    return Some("bigint".to_string());
                }
                return None;
            }
            static_primitive_to_number(left)?;
            static_primitive_to_number(right)?;
            Some("number".to_string())
        }
        BinaryOp::ZeroFillRShift => {
            if matches!(left, StaticPrimitive::BigInt(_)) || matches!(right, StaticPrimitive::BigInt(_)) {
                return None;
            }
            static_primitive_to_number(left)?;
            static_primitive_to_number(right)?;
            Some("number".to_string())
        }
        BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
            static_relational_comparison(left, right, binary_expression.op)?;
            Some("boolean".to_string())
        }
        BinaryOp::EqEq => {
            static_abstract_equality(left, right)?;
            Some("boolean".to_string())
        }
        BinaryOp::NotEq => {
            static_abstract_equality(left, right)?;
            Some("boolean".to_string())
        }
        BinaryOp::EqEqEq | BinaryOp::NotEqEq => Some("boolean".to_string()),
        _ => None,
    }
}

fn static_strict_equality(left: StaticPrimitive, right: StaticPrimitive) -> bool {
    match (left, right) {
        (StaticPrimitive::Bool(left), StaticPrimitive::Bool(right)) => left == right,
        (StaticPrimitive::String(left), StaticPrimitive::String(right)) => left == right,
        (StaticPrimitive::Number(left), StaticPrimitive::Number(right)) => {
            !left.is_nan() && !right.is_nan() && left == right
        }
        (StaticPrimitive::Null, StaticPrimitive::Null) => true,
        (StaticPrimitive::Undefined, StaticPrimitive::Undefined) => true,
        (StaticPrimitive::BigInt(left), StaticPrimitive::BigInt(right)) => left == right,
        _ => false,
    }
}

fn static_number_equality(left: f64, right: f64) -> bool {
    !left.is_nan() && !right.is_nan() && left == right
}

fn static_abstract_equality(left: StaticPrimitive, right: StaticPrimitive) -> Option<bool> {
    match (left, right) {
        (StaticPrimitive::Bool(left), StaticPrimitive::Bool(right)) => Some(left == right),
        (StaticPrimitive::String(left), StaticPrimitive::String(right)) => Some(left == right),
        (StaticPrimitive::Number(left), StaticPrimitive::Number(right)) => {
            Some(static_number_equality(left, right))
        }
        (StaticPrimitive::Null, StaticPrimitive::Null) => Some(true),
        (StaticPrimitive::Undefined, StaticPrimitive::Undefined) => Some(true),
        (StaticPrimitive::BigInt(left), StaticPrimitive::BigInt(right)) => Some(left == right),
        (StaticPrimitive::Null, StaticPrimitive::Undefined)
        | (StaticPrimitive::Undefined, StaticPrimitive::Null) => Some(true),
        (StaticPrimitive::Number(left), StaticPrimitive::String(right))
        | (StaticPrimitive::String(right), StaticPrimitive::Number(left)) => {
            Some(static_number_equality(left, parse_js_numeric_string(&right)?))
        }
        (StaticPrimitive::Bool(value), right) => static_abstract_equality(
            StaticPrimitive::Number(if value { 1.0 } else { 0.0 }),
            right,
        ),
        (left, StaticPrimitive::Bool(value)) => static_abstract_equality(
            left,
            StaticPrimitive::Number(if value { 1.0 } else { 0.0 }),
        ),
        (StaticPrimitive::BigInt(left), StaticPrimitive::String(right))
        | (StaticPrimitive::String(right), StaticPrimitive::BigInt(left)) => Some(
            static_canonical_bigint_equality(&left, parse_js_bigint_string(&right)?),
        ),
        (StaticPrimitive::BigInt(left), StaticPrimitive::Number(right))
        | (StaticPrimitive::Number(right), StaticPrimitive::BigInt(left)) => {
            static_number_bigint_equality(right, &left)
        }
        (StaticPrimitive::BigInt(_), _)
        | (_, StaticPrimitive::BigInt(_))
        | (StaticPrimitive::Null, _)
        | (_, StaticPrimitive::Null)
        | (StaticPrimitive::Undefined, _)
        | (_, StaticPrimitive::Undefined) => Some(false),
    }
}

fn static_primitive_to_number(value: StaticPrimitive) -> Option<f64> {
    match value {
        StaticPrimitive::Bool(value) => Some(if value { 1.0 } else { 0.0 }),
        StaticPrimitive::Number(value) => Some(value),
        StaticPrimitive::String(value) => parse_js_numeric_string(&value),
        StaticPrimitive::Null => Some(0.0),
        StaticPrimitive::Undefined => Some(f64::NAN),
        StaticPrimitive::BigInt(_) => None,
    }
}

fn static_relational_comparison(
    left: StaticPrimitive,
    right: StaticPrimitive,
    operator: BinaryOp,
) -> Option<bool> {
    if let (StaticPrimitive::BigInt(left_bigint), StaticPrimitive::BigInt(right_bigint)) =
        (&left, &right)
    {
        let ordering = static_bigint_decimal_compare(left_bigint, right_bigint)?;
        return Some(match operator {
            BinaryOp::Lt => ordering == std::cmp::Ordering::Less,
            BinaryOp::LtEq => {
                ordering == std::cmp::Ordering::Less || ordering == std::cmp::Ordering::Equal
            }
            BinaryOp::Gt => ordering == std::cmp::Ordering::Greater,
            BinaryOp::GtEq => {
                ordering == std::cmp::Ordering::Greater || ordering == std::cmp::Ordering::Equal
            }
            _ => return None,
        });
    }

    if let (StaticPrimitive::String(left_string), StaticPrimitive::String(right_string)) =
        (&left, &right)
    {
        return Some(match operator {
            BinaryOp::Lt => left_string < right_string,
            BinaryOp::LtEq => left_string <= right_string,
            BinaryOp::Gt => left_string > right_string,
            BinaryOp::GtEq => left_string >= right_string,
            _ => return None,
        });
    }

    let left_number = static_primitive_to_number(left)?;
    let right_number = static_primitive_to_number(right)?;

    if left_number.is_nan() || right_number.is_nan() {
        return Some(false);
    }

    Some(match operator {
        BinaryOp::Lt => left_number < right_number,
        BinaryOp::LtEq => left_number <= right_number,
        BinaryOp::Gt => left_number > right_number,
        BinaryOp::GtEq => left_number >= right_number,
        _ => return None,
    })
}

fn static_bigint_decimal_compare(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let (left_sign, left_digits) = normalize_decimal_bigint_string(left)?;
    let (right_sign, right_digits) = normalize_decimal_bigint_string(right)?;

    if left_sign != right_sign {
        return Some(if left_sign < right_sign {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        });
    }

    let mut ordering = left_digits.len().cmp(&right_digits.len());
    if ordering == std::cmp::Ordering::Equal {
        ordering = left_digits.cmp(right_digits);
    }

    if left_sign < 0 {
        ordering = ordering.reverse();
    }

    Some(ordering)
}

fn normalize_decimal_bigint_string(value: &str) -> Option<(i8, &str)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (mut sign, mut digits) = if let Some(rest) = trimmed.strip_prefix('-') {
        (-1, rest)
    } else if let Some(rest) = trimmed.strip_prefix('+') {
        (1, rest)
    } else {
        (1, trimmed)
    };

    digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        digits = "0";
        sign = 1;
    }

    if !digits.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }

    Some((sign, digits))
}

fn parse_js_bigint_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let (sign, digits) = normalize_decimal_bigint_string(trimmed)?;
    let mut normalized = String::new();
    if sign < 0 {
        normalized.push('-');
    }
    normalized.push_str(digits);
    Some(normalized)
}

fn static_canonical_bigint_equality(left: &str, right: String) -> bool {
    parse_js_bigint_string(left).as_ref() == Some(&right)
}

fn static_number_bigint_equality(number: f64, bigint: &str) -> Option<bool> {
    if number.is_nan() || !number.is_finite() {
        return Some(false);
    }
    if number.fract() != 0.0 {
        return Some(false);
    }

    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    if number.abs() > MAX_SAFE_INTEGER {
        return None;
    }

    let integer = number as i64;
    let bigint_from_number = integer.to_string();
    Some(static_canonical_bigint_equality(bigint, bigint_from_number))
}
