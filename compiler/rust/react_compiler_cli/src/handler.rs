use crate::protocol::{
    serialize_react_function, serialize_source_location, CompileRequest, CompileResponse,
    CLI_PROTOCOL_VERSION,
};
use react_compiler_core::{compile, render_react_functions_debug, CompilerOptions, InputDialect};

pub(crate) fn parse_dialect(dialect: Option<&str>) -> Result<InputDialect, String> {
    match dialect.unwrap_or("javascript") {
        "js" | "javascript" => Ok(InputDialect::JavaScript),
        "ts" | "typescript" => Ok(InputDialect::TypeScript),
        "flow" => Ok(InputDialect::Flow),
        unsupported => Err(format!("Unsupported dialect: {unsupported}")),
    }
}

pub(crate) fn handle_request(request: CompileRequest) -> CompileResponse {
    if let Some(requested_protocol_version) = request.protocol_version {
        if requested_protocol_version != CLI_PROTOCOL_VERSION {
            return CompileResponse::Error {
                protocol_version: CLI_PROTOCOL_VERSION,
                code: "unsupported_protocol_version".to_string(),
                category: "request".to_string(),
                reason: "invalid_option".to_string(),
                severity: "error".to_string(),
                message: format!(
                    "Unsupported protocol_version: {requested_protocol_version}. This CLI supports protocol_version {CLI_PROTOCOL_VERSION}"
                ),
                location: None,
            };
        }
    }

    let dialect = match parse_dialect(request.dialect.as_deref()) {
        Ok(dialect) => dialect,
        Err(message) => {
            return CompileResponse::Error {
                protocol_version: CLI_PROTOCOL_VERSION,
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
                protocol_version: CLI_PROTOCOL_VERSION,
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
                placeholder_runtime_callee_reused: output
                    .metadata
                    .placeholder_runtime_callee_reused,
                placeholder_runtime_callee_generated: output
                    .metadata
                    .placeholder_runtime_callee_generated,
                placeholder_transform_status: output
                    .metadata
                    .placeholder_transform_status
                    .clone(),
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
                placeholder_transform_candidate_component_count: output
                    .metadata
                    .placeholder_transform_candidate_component_count,
                placeholder_transform_candidate_hook_count: output
                    .metadata
                    .placeholder_transform_candidate_hook_count,
                placeholder_transform_transformed_component_count: output
                    .metadata
                    .placeholder_transform_transformed_component_count,
                placeholder_transform_transformed_hook_count: output
                    .metadata
                    .placeholder_transform_transformed_hook_count,
                placeholder_transform_skipped_component_count: output
                    .metadata
                    .placeholder_transform_skipped_component_count,
                placeholder_transform_skipped_hook_count: output
                    .metadata
                    .placeholder_transform_skipped_hook_count,
                detected_component_function_count: output
                    .metadata
                    .detected_component_function_count,
                detected_hook_function_count: output.metadata.detected_hook_function_count,
                detected_component_functions: output.metadata.detected_component_functions.clone(),
                detected_hook_functions: output.metadata.detected_hook_functions.clone(),
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
                placeholder_runtime_callee_candidate_count_before_transform: output
                    .metadata
                    .placeholder_runtime_callee_candidate_count_before_transform,
                placeholder_runtime_namespace_candidates_before_transform: output
                    .metadata
                    .placeholder_runtime_namespace_candidates_before_transform
                    .clone(),
                placeholder_runtime_namespace_candidate_count_before_transform: output
                    .metadata
                    .placeholder_runtime_namespace_candidate_count_before_transform,
                placeholder_runtime_callee_name: output
                    .metadata
                    .placeholder_runtime_callee_name
                    .clone(),
                placeholder_runtime_callee_candidates: output
                    .metadata
                    .placeholder_runtime_callee_candidates
                    .clone(),
                placeholder_runtime_callee_candidate_count: output
                    .metadata
                    .placeholder_runtime_callee_candidate_count,
                placeholder_runtime_namespace_candidates: output
                    .metadata
                    .placeholder_runtime_namespace_candidates
                    .clone(),
                placeholder_runtime_namespace_candidate_count: output
                    .metadata
                    .placeholder_runtime_namespace_candidate_count,
                debug_ir,
            }
        }
        Err(error) => CompileResponse::Error {
            protocol_version: CLI_PROTOCOL_VERSION,
            code: error.code().to_string(),
            category: error.category().to_string(),
            reason: error.reason().to_string(),
            severity: error.severity().to_string(),
            message: error.to_string(),
            location: error.location().map(serialize_source_location),
        },
    }
}
