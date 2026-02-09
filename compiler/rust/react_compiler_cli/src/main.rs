use react_compiler_core::{compile, CompilerOptions, InputDialect};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

#[derive(Debug, Deserialize)]
struct CompileRequest {
    source: String,
    filename: Option<String>,
    dialect: Option<String>,
    is_module: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
enum CompileResponse {
    #[serde(rename = "ok")]
    Ok {
        code: String,
        statement_count: usize,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

fn parse_dialect(dialect: Option<&str>) -> Result<InputDialect, String> {
    match dialect.unwrap_or("javascript") {
        "js" | "javascript" => Ok(InputDialect::JavaScript),
        "ts" | "typescript" => Ok(InputDialect::TypeScript),
        "flow" => Ok(InputDialect::Flow),
        unsupported => Err(format!("Unsupported dialect: {unsupported}")),
    }
}

fn handle_request(request: CompileRequest) -> CompileResponse {
    let dialect = match parse_dialect(request.dialect.as_deref()) {
        Ok(dialect) => dialect,
        Err(message) => return CompileResponse::Error { message },
    };

    let options = CompilerOptions {
        dialect,
        is_module: request.is_module.unwrap_or(true),
        filename: request
            .filename
            .unwrap_or_else(|| "react-compiler-input.js".to_string()),
    };

    match compile(&request.source, &options) {
        Ok(output) => CompileResponse::Ok {
            code: output.code,
            statement_count: output.metadata.statement_count,
        },
        Err(error) => CompileResponse::Error {
            message: error.to_string(),
        },
    }
}

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        let _ = writeln!(
            std::io::stderr(),
            "Failed reading request from stdin for react_compiler_cli"
        );
        std::process::exit(1);
    }

    let response = match serde_json::from_str::<CompileRequest>(&input) {
        Ok(request) => handle_request(request),
        Err(error) => CompileResponse::Error {
            message: format!("Invalid request JSON: {error}"),
        },
    };

    match serde_json::to_string(&response) {
        Ok(payload) => {
            if writeln!(std::io::stdout(), "{payload}").is_err() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            let _ = writeln!(
                std::io::stderr(),
                "Failed serializing response in react_compiler_cli: {error}"
            );
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_request, parse_dialect, CompileRequest, CompileResponse};
    use react_compiler_core::InputDialect;

    #[test]
    fn parse_dialect_maps_common_variants() {
        assert_eq!(parse_dialect(Some("js")), Ok(InputDialect::JavaScript));
        assert_eq!(
            parse_dialect(Some("typescript")),
            Ok(InputDialect::TypeScript)
        );
        assert_eq!(parse_dialect(Some("flow")), Ok(InputDialect::Flow));
    }

    #[test]
    fn compile_request_returns_statement_count() {
        let response = handle_request(CompileRequest {
            source: "export const value = 1;".to_string(),
            filename: None,
            dialect: Some("javascript".to_string()),
            is_module: Some(true),
        });

        match response {
            CompileResponse::Ok {
                statement_count, ..
            } => assert_eq!(statement_count, 1),
            CompileResponse::Error { message } => {
                panic!("expected successful compile response, got error: {message}")
            }
        }
    }
}
