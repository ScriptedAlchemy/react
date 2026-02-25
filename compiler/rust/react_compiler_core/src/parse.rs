use swc_ecma_parser::error::SyntaxError as ParserSyntaxError;

pub(crate) fn parse_syntax_error_reason(kind: &ParserSyntaxError) -> &'static str {
    match kind {
        ParserSyntaxError::Unexpected { got, .. }
            if got.contains("eof") || got.contains("EOF") || got.contains("<eof>") =>
        {
            "unexpected_eof"
        }
        ParserSyntaxError::Unexpected { .. }
        | ParserSyntaxError::UnexpectedTokenWithSuggestions { .. }
        | ParserSyntaxError::UnexpectedChar { .. }
        | ParserSyntaxError::Hash => "unexpected_token",
        ParserSyntaxError::Expected(..)
        | ParserSyntaxError::ExpectedIdent
        | ParserSyntaxError::ExpectedSemi
        | ParserSyntaxError::ExpectedSemiForExprStmt { .. }
        | ParserSyntaxError::ExpectedUnicodeEscape
        | ParserSyntaxError::ExpectedDigit { .. } => "expected_token",
        ParserSyntaxError::UnterminatedBlockComment
        | ParserSyntaxError::UnterminatedStrLit
        | ParserSyntaxError::UnterminatedRegExp
        | ParserSyntaxError::UnterminatedTpl
        | ParserSyntaxError::UnterminatedJSXContents => "unterminated_syntax",
        ParserSyntaxError::InvalidIdentChar
        | ParserSyntaxError::InvalidStrEscape
        | ParserSyntaxError::InvalidUnicodeEscape
        | ParserSyntaxError::BadCharacterEscapeSequence { .. } => "invalid_syntax",
        ParserSyntaxError::Eof => "unexpected_eof",
        _ => classify_parse_error_message(kind.msg().as_ref()),
    }
}

fn classify_parse_error_message(message: &str) -> &'static str {
    let normalized = message.to_ascii_lowercase();
    if message.contains("Unexpected token") || message.starts_with("Unexpected character") {
        return "unexpected_token";
    }
    if message.starts_with("Unexpected eof") || normalized.contains("unexpected eof") {
        return "unexpected_eof";
    }
    if message.starts_with("Expected ") || normalized.contains(" expected") {
        return "expected_token";
    }
    if message.starts_with("Unterminated ") {
        return "unterminated_syntax";
    }
    if message.starts_with("Invalid ") {
        return "invalid_syntax";
    }
    "parse_error"
}
