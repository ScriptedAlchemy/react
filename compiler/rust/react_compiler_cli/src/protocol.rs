use react_compiler_core::{ReactFunction, ReactFunctionKind, SourceLocation};
use serde::{Deserialize, Serialize};

pub(crate) const CLI_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
pub(crate) struct CompileRequest {
    pub(crate) source: String,
    pub(crate) filename: Option<String>,
    pub(crate) dialect: Option<String>,
    pub(crate) is_module: Option<bool>,
    pub(crate) apply_placeholder_transforms: Option<bool>,
    pub(crate) emit_debug_ir: Option<bool>,
    pub(crate) protocol_version: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
pub(crate) enum CompileResponse {
    #[serde(rename = "ok")]
    Ok {
        protocol_version: u32,
        code: String,
        statement_count: usize,
        statement_count_after_transform: usize,
        placeholder_runtime_helper_import_count_before_transform: usize,
        placeholder_runtime_helper_import_count_after_transform: usize,
        placeholder_runtime_helper_import_added: bool,
        placeholder_runtime_callee_reused: bool,
        placeholder_runtime_callee_generated: bool,
        placeholder_transform_status: String,
        placeholder_transform_candidates: Vec<String>,
        placeholder_transform_skipped_functions: Vec<String>,
        placeholder_transform_candidate_count: usize,
        placeholder_transform_skipped_count: usize,
        placeholder_transform_candidate_component_count: usize,
        placeholder_transform_candidate_hook_count: usize,
        placeholder_transform_transformed_component_count: usize,
        placeholder_transform_transformed_hook_count: usize,
        placeholder_transform_skipped_component_count: usize,
        placeholder_transform_skipped_hook_count: usize,
        detected_component_function_count: usize,
        detected_hook_function_count: usize,
        detected_component_functions: Vec<String>,
        detected_hook_functions: Vec<String>,
        detected_react_functions: usize,
        react_functions: Vec<SerializedReactFunction>,
        placeholder_transforms_applied: usize,
        placeholder_transformed_functions: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder_runtime_callee_name_before_transform: Option<String>,
        placeholder_runtime_callee_candidates_before_transform: Vec<String>,
        placeholder_runtime_callee_candidate_count_before_transform: usize,
        placeholder_runtime_namespace_candidates_before_transform: Vec<String>,
        placeholder_runtime_namespace_candidate_count_before_transform: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder_runtime_callee_name: Option<String>,
        placeholder_runtime_callee_candidates: Vec<String>,
        placeholder_runtime_callee_candidate_count: usize,
        placeholder_runtime_namespace_candidates: Vec<String>,
        placeholder_runtime_namespace_candidate_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        debug_ir: Option<String>,
    },
    #[serde(rename = "error")]
    Error {
        protocol_version: u32,
        code: String,
        category: String,
        reason: String,
        severity: String,
        message: String,
        location: Option<SerializedSourceLocation>,
    },
}

#[derive(Debug, Serialize)]
pub(crate) struct SerializedReactFunction {
    name: String,
    kind: String,
    loc: Option<SerializedSourceLocation>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SerializedSourceLocation {
    pub(crate) start_line: usize,
    pub(crate) start_column: usize,
    pub(crate) end_line: usize,
    pub(crate) end_column: usize,
}

pub(crate) fn serialize_react_function(function: &ReactFunction) -> SerializedReactFunction {
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

pub(crate) fn serialize_source_location(location: &SourceLocation) -> SerializedSourceLocation {
    SerializedSourceLocation {
        start_line: location.start_line,
        start_column: location.start_column,
        end_line: location.end_line,
        end_column: location.end_column,
    }
}
