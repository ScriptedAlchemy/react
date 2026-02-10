mod handler;
mod protocol;

use handler::handle_request;
use protocol::{CompileRequest, CompileResponse, CLI_PROTOCOL_VERSION};
use std::io::{Read, Write};

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
            protocol_version: CLI_PROTOCOL_VERSION,
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
    use super::{
        handle_request, CompileRequest, CompileResponse, CLI_PROTOCOL_VERSION,
    };
    use react_compiler_core::InputDialect;

    #[test]
    fn parse_dialect_maps_common_variants() {
        assert_eq!(
            super::handler::parse_dialect(Some("js")),
            Ok(InputDialect::JavaScript)
        );
        assert_eq!(
            super::handler::parse_dialect(Some("typescript")),
            Ok(InputDialect::TypeScript)
        );
        assert_eq!(super::handler::parse_dialect(Some("flow")), Ok(InputDialect::Flow));
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
            protocol_version: None,
        });

        match response {
            CompileResponse::Ok {
                protocol_version,
                statement_count,
                statement_count_after_transform,
                placeholder_runtime_helper_import_count_before_transform,
                placeholder_runtime_helper_import_count_after_transform,
                placeholder_runtime_helper_import_added,
                placeholder_runtime_callee_reused,
                placeholder_runtime_callee_generated,
                placeholder_transform_status,
                placeholder_transform_candidates,
                placeholder_transform_skipped_functions,
                placeholder_transform_candidate_count,
                placeholder_transform_skipped_count,
                placeholder_transform_candidate_component_count,
                placeholder_transform_candidate_hook_count,
                placeholder_transform_transformed_component_count,
                placeholder_transform_transformed_hook_count,
                placeholder_transform_skipped_component_count,
                placeholder_transform_skipped_hook_count,
                detected_component_function_count,
                detected_hook_function_count,
                detected_component_functions,
                detected_hook_functions,
                detected_react_functions,
                react_functions,
                placeholder_transforms_applied,
                placeholder_transformed_functions,
                placeholder_runtime_callee_name_before_transform,
                placeholder_runtime_callee_candidates_before_transform,
                placeholder_runtime_callee_candidate_count_before_transform,
                placeholder_runtime_namespace_candidates_before_transform,
                placeholder_runtime_namespace_candidate_count_before_transform,
                placeholder_runtime_callee_name,
                placeholder_runtime_callee_candidates,
                placeholder_runtime_callee_candidate_count,
                placeholder_runtime_namespace_candidates,
                placeholder_runtime_namespace_candidate_count,
                ..
            } => {
                assert_eq!(protocol_version, CLI_PROTOCOL_VERSION);
                assert_eq!(statement_count, 1);
                assert_eq!(statement_count_after_transform, 1);
                assert_eq!(placeholder_runtime_helper_import_count_before_transform, 0);
                assert_eq!(placeholder_runtime_helper_import_count_after_transform, 0);
                assert!(!placeholder_runtime_helper_import_added);
                assert!(!placeholder_runtime_callee_reused);
                assert!(!placeholder_runtime_callee_generated);
                assert_eq!(placeholder_transform_status, "disabled");
                assert!(placeholder_transform_candidates.is_empty());
                assert!(placeholder_transform_skipped_functions.is_empty());
                assert_eq!(placeholder_transform_candidate_count, 0);
                assert_eq!(placeholder_transform_skipped_count, 0);
                assert_eq!(placeholder_transform_candidate_component_count, 0);
                assert_eq!(placeholder_transform_candidate_hook_count, 0);
                assert_eq!(placeholder_transform_transformed_component_count, 0);
                assert_eq!(placeholder_transform_transformed_hook_count, 0);
                assert_eq!(placeholder_transform_skipped_component_count, 0);
                assert_eq!(placeholder_transform_skipped_hook_count, 0);
                assert_eq!(detected_component_function_count, 0);
                assert_eq!(detected_hook_function_count, 0);
                assert!(detected_component_functions.is_empty());
                assert!(detected_hook_functions.is_empty());
                assert_eq!(detected_react_functions, 0);
                assert!(react_functions.is_empty());
                assert_eq!(placeholder_transforms_applied, 0);
                assert!(placeholder_transformed_functions.is_empty());
                assert!(placeholder_runtime_callee_name_before_transform.is_none());
                assert!(placeholder_runtime_callee_candidates_before_transform.is_empty());
                assert_eq!(
                    placeholder_runtime_callee_candidate_count_before_transform,
                    0
                );
                assert!(placeholder_runtime_namespace_candidates_before_transform.is_empty());
                assert_eq!(
                    placeholder_runtime_namespace_candidate_count_before_transform,
                    0
                );
                assert!(placeholder_runtime_callee_name.is_none());
                assert!(placeholder_runtime_callee_candidates.is_empty());
                assert_eq!(placeholder_runtime_callee_candidate_count, 0);
                assert!(placeholder_runtime_namespace_candidates.is_empty());
                assert_eq!(placeholder_runtime_namespace_candidate_count, 0);
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
            protocol_version: None,
        });

        match response {
            CompileResponse::Error {
                protocol_version,
                code,
                category,
                reason,
                severity,
                ..
            } => {
                assert_eq!(protocol_version, CLI_PROTOCOL_VERSION);
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
            protocol_version: None,
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
            protocol_version: None,
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
            protocol_version: None,
        });

        match response {
            CompileResponse::Ok {
                debug_ir,
                placeholder_transformed_functions,
                placeholder_runtime_callee_name_before_transform,
                placeholder_runtime_callee_candidates_before_transform,
                placeholder_runtime_callee_candidate_count_before_transform,
                placeholder_runtime_namespace_candidates_before_transform,
                placeholder_runtime_namespace_candidate_count_before_transform,
                placeholder_runtime_callee_name,
                placeholder_runtime_callee_candidates,
                placeholder_runtime_callee_candidate_count,
                placeholder_runtime_namespace_candidates,
                placeholder_runtime_namespace_candidate_count,
                ..
            } => {
                let debug_ir = debug_ir.expect("expected debug_ir payload when requested");
                assert!(debug_ir.contains("ReactiveFunctionsDebug v0"));
                assert!(debug_ir.contains("name=Component kind=Component"));
                assert!(debug_ir.contains("placeholder_transform_candidates=Component"));
                assert!(debug_ir.contains("placeholder_transform_skipped_functions=Component"));
                assert!(debug_ir.contains("placeholder_transform_candidate_count=1"));
                assert!(debug_ir.contains("placeholder_transform_skipped_count=1"));
                assert!(debug_ir.contains("placeholder_transform_candidate_component_count=1"));
                assert!(debug_ir.contains("placeholder_transform_candidate_hook_count=0"));
                assert!(debug_ir.contains("placeholder_transform_transformed_component_count=0"));
                assert!(debug_ir.contains("placeholder_transform_transformed_hook_count=0"));
                assert!(debug_ir.contains("placeholder_transform_skipped_component_count=1"));
                assert!(debug_ir.contains("placeholder_transform_skipped_hook_count=0"));
                assert!(debug_ir.contains("placeholder_transform_status=disabled"));
                assert!(debug_ir.contains("detected_component_function_count=1"));
                assert!(debug_ir.contains("detected_hook_function_count=0"));
                assert!(debug_ir.contains("detected_component_functions=Component"));
                assert!(debug_ir.contains("detected_hook_functions="));
                assert!(debug_ir.contains("placeholder_runtime_callee_reused=false"));
                assert!(debug_ir.contains("placeholder_runtime_callee_generated=false"));
                assert!(debug_ir.contains("placeholder_runtime_callee_candidate_count=0"));
                assert!(debug_ir.contains(
                    "placeholder_runtime_callee_candidate_count_before_transform=0"
                ));
                assert!(debug_ir.contains("placeholder_runtime_namespace_candidate_count=0"));
                assert!(debug_ir.contains(
                    "placeholder_runtime_namespace_candidate_count_before_transform=0"
                ));
                assert!(placeholder_transformed_functions.is_empty());
                assert!(placeholder_runtime_callee_name_before_transform.is_none());
                assert!(placeholder_runtime_callee_candidates_before_transform.is_empty());
                assert_eq!(
                    placeholder_runtime_callee_candidate_count_before_transform,
                    0
                );
                assert!(placeholder_runtime_namespace_candidates_before_transform.is_empty());
                assert_eq!(
                    placeholder_runtime_namespace_candidate_count_before_transform,
                    0
                );
                assert!(placeholder_runtime_callee_name.is_none());
                assert!(placeholder_runtime_callee_candidates.is_empty());
                assert_eq!(placeholder_runtime_callee_candidate_count, 0);
                assert!(placeholder_runtime_namespace_candidates.is_empty());
                assert_eq!(placeholder_runtime_namespace_candidate_count, 0);
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
            protocol_version: None,
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
                assert!(debug_ir.contains("placeholder_transform_candidate_component_count=1"));
                assert!(debug_ir.contains("placeholder_transform_candidate_hook_count=0"));
                assert!(debug_ir.contains("placeholder_transform_transformed_component_count=1"));
                assert!(debug_ir.contains("placeholder_transform_transformed_hook_count=0"));
                assert!(debug_ir.contains("placeholder_transform_skipped_component_count=0"));
                assert!(debug_ir.contains("placeholder_transform_skipped_hook_count=0"));
                assert!(debug_ir.contains("placeholder_transform_status=transformed"));
                assert!(debug_ir.contains("detected_component_function_count=1"));
                assert!(debug_ir.contains("detected_hook_function_count=0"));
                assert!(debug_ir.contains("detected_component_functions=Component"));
                assert!(debug_ir.contains("detected_hook_functions="));
                assert!(debug_ir.contains("placeholder_runtime_callee_reused=false"));
                assert!(debug_ir.contains("placeholder_runtime_callee_generated=true"));
                assert!(debug_ir.contains("placeholder_runtime_callee_candidate_count=1"));
                assert!(debug_ir.contains(
                    "placeholder_runtime_callee_candidate_count_before_transform=0"
                ));
                assert!(debug_ir.contains("placeholder_runtime_namespace_candidate_count=0"));
                assert!(debug_ir.contains(
                    "placeholder_runtime_namespace_candidate_count_before_transform=0"
                ));
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
            protocol_version: None,
        });

        match response {
            CompileResponse::Ok {
                placeholder_transforms_applied,
                placeholder_transformed_functions,
                placeholder_runtime_helper_import_count_before_transform,
                placeholder_runtime_helper_import_count_after_transform,
                placeholder_runtime_helper_import_added,
                placeholder_runtime_callee_reused,
                placeholder_runtime_callee_generated,
                placeholder_transform_status,
                placeholder_transform_candidates,
                placeholder_transform_skipped_functions,
                placeholder_transform_candidate_count,
                placeholder_transform_skipped_count,
                placeholder_transform_candidate_component_count,
                placeholder_transform_candidate_hook_count,
                placeholder_transform_transformed_component_count,
                placeholder_transform_transformed_hook_count,
                placeholder_transform_skipped_component_count,
                placeholder_transform_skipped_hook_count,
                detected_component_function_count,
                detected_hook_function_count,
                detected_component_functions,
                detected_hook_functions,
                placeholder_runtime_callee_name_before_transform,
                placeholder_runtime_callee_candidates_before_transform,
                placeholder_runtime_callee_candidate_count_before_transform,
                placeholder_runtime_namespace_candidates_before_transform,
                placeholder_runtime_namespace_candidate_count_before_transform,
                placeholder_runtime_callee_name,
                placeholder_runtime_callee_candidates,
                placeholder_runtime_callee_candidate_count,
                placeholder_runtime_namespace_candidates,
                placeholder_runtime_namespace_candidate_count,
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
                assert!(!placeholder_runtime_callee_reused);
                assert!(placeholder_runtime_callee_generated);
                assert_eq!(
                    placeholder_transform_candidates,
                    vec!["__default_export_component__"]
                );
                assert!(placeholder_transform_skipped_functions.is_empty());
                assert_eq!(placeholder_transform_candidate_count, 1);
                assert_eq!(placeholder_transform_skipped_count, 0);
                assert_eq!(placeholder_transform_candidate_component_count, 1);
                assert_eq!(placeholder_transform_candidate_hook_count, 0);
                assert_eq!(placeholder_transform_transformed_component_count, 1);
                assert_eq!(placeholder_transform_transformed_hook_count, 0);
                assert_eq!(placeholder_transform_skipped_component_count, 0);
                assert_eq!(placeholder_transform_skipped_hook_count, 0);
                assert_eq!(detected_component_function_count, 1);
                assert_eq!(detected_hook_function_count, 0);
                assert_eq!(
                    detected_component_functions,
                    vec!["__default_export_component__"]
                );
                assert!(detected_hook_functions.is_empty());
                assert_eq!(placeholder_transform_status, "transformed");
                assert!(placeholder_runtime_callee_name_before_transform.is_none());
                assert!(placeholder_runtime_callee_candidates_before_transform.is_empty());
                assert_eq!(
                    placeholder_runtime_callee_candidate_count_before_transform,
                    0
                );
                assert!(placeholder_runtime_namespace_candidates_before_transform.is_empty());
                assert_eq!(
                    placeholder_runtime_namespace_candidate_count_before_transform,
                    0
                );
                assert_eq!(placeholder_runtime_callee_name.as_deref(), Some("_c"));
                assert_eq!(placeholder_runtime_callee_candidates, vec!["_c"]);
                assert_eq!(placeholder_runtime_callee_candidate_count, 1);
                assert!(placeholder_runtime_namespace_candidates.is_empty());
                assert_eq!(placeholder_runtime_namespace_candidate_count, 0);
            }
            CompileResponse::Error { message, .. } => {
                panic!("expected successful compile response, got error: {message}")
            }
        }
    }

    #[test]
    fn compile_request_reports_hook_transform_counts() {
        let response = handle_request(CompileRequest {
            source: "export function useValue() { return 1; }".to_string(),
            filename: Some("fixture.js".to_string()),
            dialect: Some("javascript".to_string()),
            is_module: Some(true),
            apply_placeholder_transforms: Some(true),
            emit_debug_ir: Some(false),
            protocol_version: None,
        });

        match response {
            CompileResponse::Ok {
                placeholder_transforms_applied,
                placeholder_transformed_functions,
                placeholder_transform_status,
                placeholder_transform_candidates,
                placeholder_transform_skipped_functions,
                placeholder_transform_candidate_count,
                placeholder_transform_skipped_count,
                placeholder_transform_candidate_component_count,
                placeholder_transform_candidate_hook_count,
                placeholder_transform_transformed_component_count,
                placeholder_transform_transformed_hook_count,
                placeholder_transform_skipped_component_count,
                placeholder_transform_skipped_hook_count,
                detected_component_function_count,
                detected_hook_function_count,
                detected_component_functions,
                detected_hook_functions,
                placeholder_runtime_helper_import_count_before_transform,
                placeholder_runtime_helper_import_count_after_transform,
                placeholder_runtime_helper_import_added,
                placeholder_runtime_callee_reused,
                placeholder_runtime_callee_generated,
                placeholder_runtime_callee_name,
                placeholder_runtime_callee_candidate_count,
                placeholder_runtime_callee_candidate_count_before_transform,
                ..
            } => {
                assert_eq!(placeholder_transforms_applied, 1);
                assert_eq!(placeholder_transformed_functions, vec!["useValue"]);
                assert_eq!(placeholder_transform_status, "transformed");
                assert_eq!(placeholder_transform_candidates, vec!["useValue"]);
                assert!(placeholder_transform_skipped_functions.is_empty());
                assert_eq!(placeholder_transform_candidate_count, 1);
                assert_eq!(placeholder_transform_skipped_count, 0);
                assert_eq!(placeholder_transform_candidate_component_count, 0);
                assert_eq!(placeholder_transform_candidate_hook_count, 1);
                assert_eq!(placeholder_transform_transformed_component_count, 0);
                assert_eq!(placeholder_transform_transformed_hook_count, 1);
                assert_eq!(placeholder_transform_skipped_component_count, 0);
                assert_eq!(placeholder_transform_skipped_hook_count, 0);
                assert_eq!(detected_component_function_count, 0);
                assert_eq!(detected_hook_function_count, 1);
                assert!(detected_component_functions.is_empty());
                assert_eq!(detected_hook_functions, vec!["useValue"]);
                assert_eq!(placeholder_runtime_helper_import_count_before_transform, 0);
                assert_eq!(placeholder_runtime_helper_import_count_after_transform, 1);
                assert!(placeholder_runtime_helper_import_added);
                assert!(!placeholder_runtime_callee_reused);
                assert!(placeholder_runtime_callee_generated);
                assert_eq!(placeholder_runtime_callee_name.as_deref(), Some("_c"));
                assert_eq!(placeholder_runtime_callee_candidate_count, 1);
                assert_eq!(
                    placeholder_runtime_callee_candidate_count_before_transform,
                    0
                );
            }
            CompileResponse::Error { message, .. } => {
                panic!("expected successful compile response, got error: {message}")
            }
        }
    }

    #[test]
    fn compile_request_accepts_supported_protocol_version() {
        let response = handle_request(CompileRequest {
            source: "export const value = 1;".to_string(),
            filename: None,
            dialect: Some("javascript".to_string()),
            is_module: Some(true),
            apply_placeholder_transforms: None,
            emit_debug_ir: None,
            protocol_version: Some(CLI_PROTOCOL_VERSION),
        });

        match response {
            CompileResponse::Ok {
                protocol_version,
                statement_count,
                ..
            } => {
                assert_eq!(protocol_version, CLI_PROTOCOL_VERSION);
                assert_eq!(statement_count, 1);
            }
            CompileResponse::Error { message, .. } => {
                panic!("expected successful compile response, got error: {message}")
            }
        }
    }

    #[test]
    fn compile_request_rejects_unsupported_protocol_version() {
        let response = handle_request(CompileRequest {
            source: "export const value = 1;".to_string(),
            filename: None,
            dialect: Some("javascript".to_string()),
            is_module: Some(true),
            apply_placeholder_transforms: None,
            emit_debug_ir: None,
            protocol_version: Some(CLI_PROTOCOL_VERSION + 1),
        });

        match response {
            CompileResponse::Error {
                protocol_version,
                code,
                category,
                reason,
                severity,
                message,
                ..
            } => {
                assert_eq!(protocol_version, CLI_PROTOCOL_VERSION);
                assert_eq!(code, "unsupported_protocol_version");
                assert_eq!(category, "request");
                assert_eq!(reason, "invalid_option");
                assert_eq!(severity, "error");
                assert!(message.contains("Unsupported protocol_version"));
            }
            CompileResponse::Ok { .. } => {
                panic!("expected unsupported protocol version error")
            }
        }
    }
}
