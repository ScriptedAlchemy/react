use react_compiler_core::{
    compile, CompilerOptions, InputDialect, ReactFunction, ReactFunctionKind, SourceLocation,
};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

#[derive(Debug, Deserialize)]
struct CompileRequest {
    source: String,
    filename: Option<String>,
    dialect: Option<String>,
    is_module: Option<bool>,
    apply_placeholder_transforms: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
enum CompileResponse {
    #[serde(rename = "ok")]
    Ok {
        code: String,
        statement_count: usize,
        detected_react_functions: usize,
        react_functions: Vec<SerializedReactFunction>,
    },
    #[serde(rename = "error")]
    Error { code: String, message: String },
}

#[derive(Debug, Serialize)]
struct SerializedReactFunction {
    name: String,
    kind: String,
    loc: Option<SerializedSourceLocation>,
}

#[derive(Debug, Serialize)]
struct SerializedSourceLocation {
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
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
        Err(message) => {
            return CompileResponse::Error {
                code: "unsupported_dialect".to_string(),
                message,
            }
        }
    };

    let options = CompilerOptions {
        dialect,
        is_module: request.is_module.unwrap_or(true),
        filename: request
            .filename
            .unwrap_or_else(|| "react-compiler-input.js".to_string()),
        apply_placeholder_transforms: request.apply_placeholder_transforms.unwrap_or(false),
    };

    match compile(&request.source, &options) {
        Ok(output) => CompileResponse::Ok {
            code: output.code,
            statement_count: output.metadata.statement_count,
            detected_react_functions: output.metadata.detected_react_functions,
            react_functions: output
                .metadata
                .react_functions
                .iter()
                .map(serialize_react_function)
                .collect(),
        },
        Err(error) => CompileResponse::Error {
            code: error.code().to_string(),
            message: error.to_string(),
        },
    }
}

fn serialize_react_function(function: &ReactFunction) -> SerializedReactFunction {
    SerializedReactFunction {
        name: function.name.clone(),
        kind: serialize_react_function_kind(function.kind.clone()).to_string(),
        loc: function.loc.as_ref().map(serialize_source_location),
    }
}

fn serialize_react_function_kind(kind: ReactFunctionKind) -> &'static str {
    match kind {
        ReactFunctionKind::Component => "Component",
        ReactFunctionKind::Hook => "Hook",
    }
}

fn serialize_source_location(location: &SourceLocation) -> SerializedSourceLocation {
    SerializedSourceLocation {
        start_line: location.start_line,
        start_column: location.start_column,
        end_line: location.end_line,
        end_column: location.end_column,
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
            code: "invalid_request".to_string(),
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
            apply_placeholder_transforms: None,
        });

        match response {
            CompileResponse::Ok {
                statement_count,
                detected_react_functions,
                react_functions,
                ..
            } => {
                assert_eq!(statement_count, 1);
                assert_eq!(detected_react_functions, 0);
                assert!(react_functions.is_empty());
            }
            CompileResponse::Error { message, .. } => {
                panic!("expected successful compile response, got error: {message}")
            }
        }
    }

    #[test]
    fn compile_request_returns_error_code_for_unsupported_dialect() {
        let response = handle_request(CompileRequest {
            source: "const value = 1;".to_string(),
            filename: None,
            dialect: Some("unknown".to_string()),
            is_module: Some(true),
            apply_placeholder_transforms: None,
        });

        match response {
            CompileResponse::Error { code, .. } => {
                assert_eq!(code, "unsupported_dialect");
            }
            CompileResponse::Ok { .. } => {
                panic!("expected error response for unsupported dialect")
            }
        }
    }
}
