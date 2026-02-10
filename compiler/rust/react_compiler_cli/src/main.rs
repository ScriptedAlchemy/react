use react_compiler_core::{
    compile, render_react_functions_debug, CompilerOptions, InputDialect, ReactFunction,
    ReactFunctionKind, SourceLocation,
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
    emit_debug_ir: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
enum CompileResponse {
    #[serde(rename = "ok")]
    Ok {
        code: String,
        statement_count: usize,
        statement_count_after_transform: usize,
        placeholder_runtime_helper_import_count_before_transform: usize,
        placeholder_runtime_helper_import_count_after_transform: usize,
        placeholder_runtime_helper_import_added: bool,
        placeholder_transform_candidates: Vec<String>,
        placeholder_transform_skipped_functions: Vec<String>,
        placeholder_transform_candidate_count: usize,
        placeholder_transform_skipped_count: usize,
        detected_react_functions: usize,
        react_functions: Vec<SerializedReactFunction>,
        placeholder_transforms_applied: usize,
        placeholder_transformed_functions: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder_runtime_callee_name_before_transform: Option<String>,
        placeholder_runtime_callee_candidates_before_transform: Vec<String>,
        placeholder_runtime_namespace_candidates_before_transform: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder_runtime_callee_name: Option<String>,
        placeholder_runtime_callee_candidates: Vec<String>,
        placeholder_runtime_namespace_candidates: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        debug_ir: Option<String>,
    },
    #[serde(rename = "error")]
    Error {
        code: String,
        category: String,
        reason: String,
        severity: String,
        message: String,
        location: Option<SerializedSourceLocation>,
    },
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
                category: "request".to_string(),
                reason: "invalid_option".to_string(),
                severity: "error".to_string(),
                message,
                location: None,
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
        Ok(output) => {
            let debug_ir = if request.emit_debug_ir.unwrap_or(false) {
                Some(render_react_functions_debug(&output.metadata))
            } else {
                None
            };
            CompileResponse::Ok {
                code: output.code,
                statement_count: output.metadata.statement_count,
                statement_count_after_transform: output.metadata.statement_count_after_transform,
                placeholder_runtime_helper_import_count_before_transform: output
                    .metadata
                    .placeholder_runtime_helper_import_count_before_transform,
                placeholder_runtime_helper_import_count_after_transform: output
                    .metadata
                    .placeholder_runtime_helper_import_count_after_transform,
                placeholder_runtime_helper_import_added: output
                    .metadata
                    .placeholder_runtime_helper_import_added,
                placeholder_transform_candidates: output
                    .metadata
                    .placeholder_transform_candidates
                    .clone(),
                placeholder_transform_skipped_functions: output
                    .metadata
                    .placeholder_transform_skipped_functions
                    .clone(),
                placeholder_transform_candidate_count: output
                    .metadata
                    .placeholder_transform_candidate_count,
                placeholder_transform_skipped_count: output
                    .metadata
                    .placeholder_transform_skipped_count,
                detected_react_functions: output.metadata.detected_react_functions,
                react_functions: output
                    .metadata
                    .react_functions
                    .iter()
                    .map(serialize_react_function)
                    .collect(),
                placeholder_transforms_applied: output.metadata.placeholder_transforms_applied,
                placeholder_transformed_functions: output
                    .metadata
                    .placeholder_transformed_functions
                    .clone(),
                placeholder_runtime_callee_name_before_transform: output
                    .metadata
                    .placeholder_runtime_callee_name_before_transform
                    .clone(),
                placeholder_runtime_callee_candidates_before_transform: output
                    .metadata
                    .placeholder_runtime_callee_candidates_before_transform
                    .clone(),
                placeholder_runtime_namespace_candidates_before_transform: output
                    .metadata
                    .placeholder_runtime_namespace_candidates_before_transform
                    .clone(),
                placeholder_runtime_callee_name: output
                    .metadata
                    .placeholder_runtime_callee_name
                    .clone(),
                placeholder_runtime_callee_candidates: output
                    .metadata
                    .placeholder_runtime_callee_candidates
                    .clone(),
                placeholder_runtime_namespace_candidates: output
                    .metadata
                    .placeholder_runtime_namespace_candidates
                    .clone(),
                debug_ir,
            }
        }
        Err(error) => CompileResponse::Error {
            code: error.code().to_string(),
            category: error.category().to_string(),
            reason: error.reason().to_string(),
            severity: error.severity().to_string(),
            message: error.to_string(),
            location: error.location().map(serialize_source_location),
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
            category: "request".to_string(),
            reason: "invalid_request".to_string(),
            severity: "error".to_string(),
            message: format!("Invalid request JSON: {error}"),
            location: None,
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
            emit_debug_ir: None,
        });

        match response {
            CompileResponse::Ok {
                statement_count,
                statement_count_after_transform,
                placeholder_runtime_helper_import_count_before_transform,
                placeholder_runtime_helper_import_count_after_transform,
                placeholder_runtime_helper_import_added,
                placeholder_transform_candidates,
                placeholder_transform_skipped_functions,
                placeholder_transform_candidate_count,
                placeholder_transform_skipped_count,
                detected_react_functions,
                react_functions,
                placeholder_transforms_applied,
                placeholder_transformed_functions,
                placeholder_runtime_callee_name_before_transform,
                placeholder_runtime_callee_candidates_before_transform,
                placeholder_runtime_namespace_candidates_before_transform,
                placeholder_runtime_callee_name,
                placeholder_runtime_callee_candidates,
                placeholder_runtime_namespace_candidates,
                ..
            } => {
                assert_eq!(statement_count, 1);
                assert_eq!(statement_count_after_transform, 1);
                assert_eq!(placeholder_runtime_helper_import_count_before_transform, 0);
                assert_eq!(placeholder_runtime_helper_import_count_after_transform, 0);
                assert!(!placeholder_runtime_helper_import_added);
                assert!(placeholder_transform_candidates.is_empty());
                assert!(placeholder_transform_skipped_functions.is_empty());
                assert_eq!(placeholder_transform_candidate_count, 0);
                assert_eq!(placeholder_transform_skipped_count, 0);
                assert_eq!(detected_react_functions, 0);
                assert!(react_functions.is_empty());
                assert_eq!(placeholder_transforms_applied, 0);
                assert!(placeholder_transformed_functions.is_empty());
                assert!(placeholder_runtime_callee_name_before_transform.is_none());
                assert!(placeholder_runtime_callee_candidates_before_transform.is_empty());
                assert!(placeholder_runtime_namespace_candidates_before_transform.is_empty());
                assert!(placeholder_runtime_callee_name.is_none());
                assert!(placeholder_runtime_callee_candidates.is_empty());
                assert!(placeholder_runtime_namespace_candidates.is_empty());
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
            emit_debug_ir: None,
        });

        match response {
            CompileResponse::Error {
                code,
                category,
                reason,
                severity,
                ..
            } => {
                assert_eq!(code, "unsupported_dialect");
                assert_eq!(category, "request");
                assert_eq!(reason, "invalid_option");
                assert_eq!(severity, "error");
            }
            CompileResponse::Ok { .. } => {
                panic!("expected error response for unsupported dialect")
            }
        }
    }

    #[test]
    fn compile_request_returns_parse_failure_location() {
        let response = handle_request(CompileRequest {
            source: "const = 1;".to_string(),
            filename: Some("broken.js".to_string()),
            dialect: Some("javascript".to_string()),
            is_module: Some(false),
            apply_placeholder_transforms: None,
            emit_debug_ir: None,
        });

        match response {
            CompileResponse::Error {
                code,
                category,
                reason,
                severity,
                location,
                ..
            } => {
                assert_eq!(code, "parse_failure");
                assert_eq!(category, "syntax");
                assert_eq!(reason, "unexpected_token");
                assert_eq!(severity, "error");
                assert!(location.is_some());
            }
            CompileResponse::Ok { .. } => {
                panic!("expected parse failure for invalid javascript source")
            }
        }
    }

    #[test]
    fn compile_request_returns_invalid_syntax_reason() {
        let response = handle_request(CompileRequest {
            source: "const \\u00ZZ = 1;".to_string(),
            filename: Some("broken.js".to_string()),
            dialect: Some("javascript".to_string()),
            is_module: Some(false),
            apply_placeholder_transforms: None,
            emit_debug_ir: None,
        });

        match response {
            CompileResponse::Error {
                code,
                category,
                reason,
                severity,
                location,
                ..
            } => {
                assert_eq!(code, "parse_failure");
                assert_eq!(category, "syntax");
                assert_eq!(reason, "invalid_syntax");
                assert_eq!(severity, "error");
                assert!(location.is_some());
            }
            CompileResponse::Ok { .. } => {
                panic!("expected parse failure for invalid javascript source")
            }
        }
    }

    #[test]
    fn compile_request_can_emit_debug_ir() {
        let response = handle_request(CompileRequest {
            source: "function Component() { return <div />; }".to_string(),
            filename: Some("fixture.jsx".to_string()),
            dialect: Some("javascript".to_string()),
            is_module: Some(false),
            apply_placeholder_transforms: Some(false),
            emit_debug_ir: Some(true),
        });

        match response {
            CompileResponse::Ok {
                debug_ir,
                placeholder_transformed_functions,
                placeholder_runtime_callee_name_before_transform,
                placeholder_runtime_callee_candidates_before_transform,
                placeholder_runtime_namespace_candidates_before_transform,
                placeholder_runtime_callee_name,
                placeholder_runtime_callee_candidates,
                placeholder_runtime_namespace_candidates,
                ..
            } => {
                let debug_ir = debug_ir.expect("expected debug_ir payload when requested");
                assert!(debug_ir.contains("ReactiveFunctionsDebug v0"));
                assert!(debug_ir.contains("name=Component kind=Component"));
                assert!(debug_ir.contains("placeholder_transform_candidates=Component"));
                assert!(debug_ir.contains("placeholder_transform_skipped_functions=Component"));
                assert!(debug_ir.contains("placeholder_transform_candidate_count=1"));
                assert!(debug_ir.contains("placeholder_transform_skipped_count=1"));
                assert!(placeholder_transformed_functions.is_empty());
                assert!(placeholder_runtime_callee_name_before_transform.is_none());
                assert!(placeholder_runtime_callee_candidates_before_transform.is_empty());
                assert!(placeholder_runtime_namespace_candidates_before_transform.is_empty());
                assert!(placeholder_runtime_callee_name.is_none());
                assert!(placeholder_runtime_callee_candidates.is_empty());
                assert!(placeholder_runtime_namespace_candidates.is_empty());
            }
            CompileResponse::Error { message, .. } => {
                panic!("expected successful compile response, got error: {message}")
            }
        }
    }

    #[test]
    fn compile_request_debug_ir_reports_transform_pass_state() {
        let response = handle_request(CompileRequest {
            source: "export function Component() { return <div />; }".to_string(),
            filename: Some("fixture.jsx".to_string()),
            dialect: Some("javascript".to_string()),
            is_module: Some(true),
            apply_placeholder_transforms: Some(true),
            emit_debug_ir: Some(true),
        });

        match response {
            CompileResponse::Ok { debug_ir, .. } => {
                let debug_ir = debug_ir.expect("debug_ir should be populated");
                assert!(debug_ir.contains("statement_count=1"));
                assert!(debug_ir.contains("statement_count_after_transform=2"));
                assert!(debug_ir.contains(
                    "placeholder_runtime_helper_import_count_before_transform=0"
                ));
                assert!(debug_ir.contains(
                    "placeholder_runtime_helper_import_count_after_transform=1"
                ));
                assert!(debug_ir.contains("placeholder_runtime_helper_import_added=true"));
                assert!(debug_ir.contains("placeholder_transform_candidates=Component"));
                assert!(debug_ir.contains("placeholder_transform_skipped_functions="));
                assert!(debug_ir.contains("placeholder_transform_candidate_count=1"));
                assert!(debug_ir.contains("placeholder_transform_skipped_count=0"));
                assert!(debug_ir.contains("placeholder_transforms_applied=1"));
                assert!(debug_ir.contains("placeholder_transformed_functions=Component"));
                assert!(debug_ir.contains("placeholder_runtime_callee_name=_c"));
                assert!(debug_ir.contains("name=Component kind=Component"));
            }
            CompileResponse::Error { message, .. } => {
                panic!("expected successful compile response, got error: {message}")
            }
        }
    }

    #[test]
    fn compile_request_reports_placeholder_transform_count() {
        let response = handle_request(CompileRequest {
            source: "export default (() => <div />);".to_string(),
            filename: Some("fixture.jsx".to_string()),
            dialect: Some("javascript".to_string()),
            is_module: Some(true),
            apply_placeholder_transforms: Some(true),
            emit_debug_ir: Some(false),
        });

        match response {
            CompileResponse::Ok {
                placeholder_transforms_applied,
                placeholder_transformed_functions,
                placeholder_runtime_helper_import_count_before_transform,
                placeholder_runtime_helper_import_count_after_transform,
                placeholder_runtime_helper_import_added,
                placeholder_transform_candidates,
                placeholder_transform_skipped_functions,
                placeholder_transform_candidate_count,
                placeholder_transform_skipped_count,
                placeholder_runtime_callee_name_before_transform,
                placeholder_runtime_callee_candidates_before_transform,
                placeholder_runtime_namespace_candidates_before_transform,
                placeholder_runtime_callee_name,
                placeholder_runtime_callee_candidates,
                placeholder_runtime_namespace_candidates,
                ..
            } => {
                assert_eq!(placeholder_transforms_applied, 1);
                assert_eq!(
                    placeholder_transformed_functions,
                    vec!["__default_export_component__"]
                );
                assert_eq!(placeholder_runtime_helper_import_count_before_transform, 0);
                assert_eq!(placeholder_runtime_helper_import_count_after_transform, 1);
                assert!(placeholder_runtime_helper_import_added);
                assert_eq!(
                    placeholder_transform_candidates,
                    vec!["__default_export_component__"]
                );
                assert!(placeholder_transform_skipped_functions.is_empty());
                assert_eq!(placeholder_transform_candidate_count, 1);
                assert_eq!(placeholder_transform_skipped_count, 0);
                assert!(placeholder_runtime_callee_name_before_transform.is_none());
                assert!(placeholder_runtime_callee_candidates_before_transform.is_empty());
                assert!(placeholder_runtime_namespace_candidates_before_transform.is_empty());
                assert_eq!(placeholder_runtime_callee_name.as_deref(), Some("_c"));
                assert_eq!(placeholder_runtime_callee_candidates, vec!["_c"]);
                assert!(placeholder_runtime_namespace_candidates.is_empty());
            }
            CompileResponse::Error { message, .. } => {
                panic!("expected successful compile response, got error: {message}")
            }
        }
    }
}
