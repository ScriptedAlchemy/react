#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDialect {
    JavaScript,
    TypeScript,
    Flow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerOptions {
    pub dialect: InputDialect,
    pub is_module: bool,
    pub filename: String,
    pub apply_placeholder_transforms: bool,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            dialect: InputDialect::JavaScript,
            is_module: true,
            filename: "unknown.js".to_string(),
            apply_placeholder_transforms: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseMetadata {
    pub statement_count: usize,
    pub statement_count_after_transform: usize,
    pub placeholder_runtime_helper_import_count_before_transform: usize,
    pub placeholder_runtime_helper_import_count_after_transform: usize,
    pub placeholder_runtime_helper_import_added: bool,
    pub placeholder_runtime_callee_reused: bool,
    pub placeholder_runtime_callee_generated: bool,
    pub placeholder_transform_status: String,
    pub placeholder_transform_candidates: Vec<String>,
    pub placeholder_transform_skipped_functions: Vec<String>,
    pub placeholder_transform_candidate_count: usize,
    pub placeholder_transform_skipped_count: usize,
    pub placeholder_transform_candidate_component_count: usize,
    pub placeholder_transform_candidate_hook_count: usize,
    pub placeholder_transform_transformed_component_count: usize,
    pub placeholder_transform_transformed_hook_count: usize,
    pub placeholder_transform_skipped_component_count: usize,
    pub placeholder_transform_skipped_hook_count: usize,
    pub detected_component_function_count: usize,
    pub detected_hook_function_count: usize,
    pub detected_component_functions: Vec<String>,
    pub detected_hook_functions: Vec<String>,
    pub detected_react_functions: usize,
    pub react_functions: Vec<ReactFunction>,
    pub placeholder_transforms_applied: usize,
    pub placeholder_transformed_functions: Vec<String>,
    pub placeholder_runtime_callee_name_before_transform: Option<String>,
    pub placeholder_runtime_callee_candidates_before_transform: Vec<String>,
    pub placeholder_runtime_callee_candidate_count_before_transform: usize,
    pub placeholder_runtime_namespace_candidates_before_transform: Vec<String>,
    pub placeholder_runtime_namespace_candidate_count_before_transform: usize,
    pub placeholder_runtime_callee_name: Option<String>,
    pub placeholder_runtime_callee_candidates: Vec<String>,
    pub placeholder_runtime_callee_candidate_count: usize,
    pub placeholder_runtime_namespace_candidates: Vec<String>,
    pub placeholder_runtime_namespace_candidate_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactFunctionKind {
    Component,
    Hook,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactFunction {
    pub name: String,
    pub kind: ReactFunctionKind,
    pub loc: Option<SourceLocation>,
}

pub const DEFAULT_EXPORT_COMPONENT_NAME: &str = "__default_export_component__";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileOutput {
    pub code: String,
    pub metadata: ParseMetadata,
}

pub fn render_react_functions_debug(metadata: &ParseMetadata) -> String {
    let mut lines = vec![
        "ReactiveFunctionsDebug v0".to_string(),
        format!("statement_count={}", metadata.statement_count),
        format!(
            "statement_count_after_transform={}",
            metadata.statement_count_after_transform
        ),
        format!(
            "placeholder_runtime_helper_import_count_before_transform={}",
            metadata.placeholder_runtime_helper_import_count_before_transform
        ),
        format!(
            "placeholder_runtime_helper_import_count_after_transform={}",
            metadata.placeholder_runtime_helper_import_count_after_transform
        ),
        format!(
            "placeholder_runtime_helper_import_added={}",
            metadata.placeholder_runtime_helper_import_added
        ),
        format!(
            "placeholder_runtime_callee_reused={}",
            metadata.placeholder_runtime_callee_reused
        ),
        format!(
            "placeholder_runtime_callee_generated={}",
            metadata.placeholder_runtime_callee_generated
        ),
        format!(
            "placeholder_transform_status={}",
            metadata.placeholder_transform_status
        ),
        format!(
            "placeholder_transform_candidates={}",
            metadata.placeholder_transform_candidates.join(",")
        ),
        format!(
            "placeholder_transform_skipped_functions={}",
            metadata.placeholder_transform_skipped_functions.join(",")
        ),
        format!(
            "placeholder_transform_candidate_count={}",
            metadata.placeholder_transform_candidate_count
        ),
        format!(
            "placeholder_transform_skipped_count={}",
            metadata.placeholder_transform_skipped_count
        ),
        format!(
            "placeholder_transform_candidate_component_count={}",
            metadata.placeholder_transform_candidate_component_count
        ),
        format!(
            "placeholder_transform_candidate_hook_count={}",
            metadata.placeholder_transform_candidate_hook_count
        ),
        format!(
            "placeholder_transform_transformed_component_count={}",
            metadata.placeholder_transform_transformed_component_count
        ),
        format!(
            "placeholder_transform_transformed_hook_count={}",
            metadata.placeholder_transform_transformed_hook_count
        ),
        format!(
            "placeholder_transform_skipped_component_count={}",
            metadata.placeholder_transform_skipped_component_count
        ),
        format!(
            "placeholder_transform_skipped_hook_count={}",
            metadata.placeholder_transform_skipped_hook_count
        ),
        format!(
            "detected_component_function_count={}",
            metadata.detected_component_function_count
        ),
        format!(
            "detected_hook_function_count={}",
            metadata.detected_hook_function_count
        ),
        format!(
            "detected_component_functions={}",
            metadata.detected_component_functions.join(",")
        ),
        format!(
            "detected_hook_functions={}",
            metadata.detected_hook_functions.join(",")
        ),
        format!(
            "detected_react_functions={}",
            metadata.detected_react_functions
        ),
        format!(
            "placeholder_transforms_applied={}",
            metadata.placeholder_transforms_applied
        ),
        format!(
            "placeholder_transformed_functions={}",
            metadata.placeholder_transformed_functions.join(",")
        ),
        format!(
            "placeholder_runtime_callee_name={}",
            metadata
                .placeholder_runtime_callee_name
                .as_deref()
                .unwrap_or("none")
        ),
        format!(
            "placeholder_runtime_callee_name_before_transform={}",
            metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref()
                .unwrap_or("none")
        ),
        format!(
            "placeholder_runtime_callee_candidates={}",
            metadata.placeholder_runtime_callee_candidates.join(",")
        ),
        format!(
            "placeholder_runtime_callee_candidate_count={}",
            metadata.placeholder_runtime_callee_candidate_count
        ),
        format!(
            "placeholder_runtime_callee_candidates_before_transform={}",
            metadata
                .placeholder_runtime_callee_candidates_before_transform
                .join(",")
        ),
        format!(
            "placeholder_runtime_callee_candidate_count_before_transform={}",
            metadata.placeholder_runtime_callee_candidate_count_before_transform
        ),
        format!(
            "placeholder_runtime_namespace_candidates={}",
            metadata.placeholder_runtime_namespace_candidates.join(",")
        ),
        format!(
            "placeholder_runtime_namespace_candidate_count={}",
            metadata.placeholder_runtime_namespace_candidate_count
        ),
        format!(
            "placeholder_runtime_namespace_candidates_before_transform={}",
            metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .join(",")
        ),
        format!(
            "placeholder_runtime_namespace_candidate_count_before_transform={}",
            metadata.placeholder_runtime_namespace_candidate_count_before_transform
        ),
    ];
    for (index, function) in metadata.react_functions.iter().enumerate() {
        let kind = match function.kind {
            ReactFunctionKind::Component => "Component",
            ReactFunctionKind::Hook => "Hook",
        };
        let loc = function
            .loc
            .as_ref()
            .map(|location| {
                format!(
                    "{}:{}-{}:{}",
                    location.start_line,
                    location.start_column,
                    location.end_line,
                    location.end_column
                )
            })
            .unwrap_or_else(|| "none".to_string());
        lines.push(format!(
            "fn[{index}] name={} kind={} loc={loc}",
            function.name, kind
        ));
    }
    lines.join("\n")
}
