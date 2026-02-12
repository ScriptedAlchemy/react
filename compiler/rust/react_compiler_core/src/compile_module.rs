use swc_common::{comments::SingleThreadedComments, sync::Lrc, SourceMap};
use swc_ecma_ast::Module;

use crate::{
    emit::emit_module,
    placeholder::count_runtime_helper_imports,
    placeholder_apply::apply_placeholder_compilation_to_module,
    react_detect::{
        collect_placeholder_transform_candidate_names_for_module, collect_react_functions_in_module,
    },
    react_fn::{
        collect_detected_react_function_names_by_kind,
        compute_placeholder_transform_skipped_functions, count_detected_react_functions_by_kind,
        count_placeholder_transform_names_by_kind, derive_placeholder_transform_status,
        sort_and_dedup_names,
    },
    runtime_scan::{
        runtime_memo_callee_scan_for_module, select_runtime_callee_name,
        sorted_runtime_callee_candidates, sorted_runtime_namespace_candidates,
    },
    CompilerError, CompilerOptions, ParseMetadata,
};

pub(crate) fn compile_module_output(
    cm: &Lrc<SourceMap>,
    comments: &SingleThreadedComments,
    module: &mut Module,
    options: &CompilerOptions,
) -> Result<(ParseMetadata, String), CompilerError> {
    let react_functions = collect_react_functions_in_module(cm, module);
    let original_statement_count = module.body.len();
    let mut placeholder_transform_candidates =
        collect_placeholder_transform_candidate_names_for_module(module, &react_functions)
            .into_iter()
            .collect::<Vec<_>>();
    sort_and_dedup_names(&mut placeholder_transform_candidates);
    let runtime_helper_import_count_before_transform = if options.apply_placeholder_transforms {
        count_runtime_helper_imports(module)
    } else {
        0
    };
    let (
        placeholder_runtime_callee_name_before_transform,
        placeholder_runtime_callee_candidates_before_transform,
        placeholder_runtime_namespace_candidates_before_transform,
    ) = if options.apply_placeholder_transforms {
        let runtime_scan_before = runtime_memo_callee_scan_for_module(module);
        (
            select_runtime_callee_name(&runtime_scan_before.runtime_callee_bindings),
            sorted_runtime_callee_candidates(&runtime_scan_before.runtime_callee_bindings),
            sorted_runtime_namespace_candidates(&runtime_scan_before.runtime_namespace_bindings),
        )
    } else {
        (None, Vec::new(), Vec::new())
    };
    let mut placeholder_transformed_functions = if options.apply_placeholder_transforms {
        apply_placeholder_compilation_to_module(module, &react_functions)
    } else {
        Vec::new()
    };
    let transformed_count = placeholder_transformed_functions.len();
    let transformed_statement_count = module.body.len();
    let runtime_helper_import_count_after_transform = if options.apply_placeholder_transforms {
        count_runtime_helper_imports(module)
    } else {
        0
    };
    let runtime_helper_import_added =
        runtime_helper_import_count_after_transform > runtime_helper_import_count_before_transform;
    let (
        placeholder_runtime_callee_name,
        placeholder_runtime_callee_candidates,
        placeholder_runtime_namespace_candidates,
    ) = if options.apply_placeholder_transforms {
        let runtime_scan_after = runtime_memo_callee_scan_for_module(module);
        (
            select_runtime_callee_name(&runtime_scan_after.runtime_callee_bindings),
            sorted_runtime_callee_candidates(&runtime_scan_after.runtime_callee_bindings),
            sorted_runtime_namespace_candidates(&runtime_scan_after.runtime_namespace_bindings),
        )
    } else {
        (None, Vec::new(), Vec::new())
    };
    let placeholder_runtime_callee_reused = options.apply_placeholder_transforms
        && transformed_count > 0
        && placeholder_runtime_callee_name_before_transform.is_some()
        && placeholder_runtime_callee_name_before_transform == placeholder_runtime_callee_name;
    let placeholder_runtime_callee_generated = options.apply_placeholder_transforms
        && transformed_count > 0
        && placeholder_runtime_callee_name_before_transform.is_none()
        && placeholder_runtime_callee_name.is_some()
        && runtime_helper_import_added;
    sort_and_dedup_names(&mut placeholder_transformed_functions);
    let placeholder_transform_skipped_functions = compute_placeholder_transform_skipped_functions(
        &placeholder_transform_candidates,
        &placeholder_transformed_functions,
    );
    let candidate_kind_counts = count_placeholder_transform_names_by_kind(
        &placeholder_transform_candidates,
        &react_functions,
    );
    let transformed_kind_counts = count_placeholder_transform_names_by_kind(
        &placeholder_transformed_functions,
        &react_functions,
    );
    let skipped_kind_counts = count_placeholder_transform_names_by_kind(
        &placeholder_transform_skipped_functions,
        &react_functions,
    );
    let placeholder_transform_status = derive_placeholder_transform_status(
        options.apply_placeholder_transforms,
        true,
        placeholder_transform_candidates.len(),
        transformed_count,
        placeholder_runtime_callee_name_before_transform.is_some(),
    )
    .to_string();
    let placeholder_runtime_callee_candidate_count_before_transform =
        placeholder_runtime_callee_candidates_before_transform.len();
    let placeholder_runtime_namespace_candidate_count_before_transform =
        placeholder_runtime_namespace_candidates_before_transform.len();
    let placeholder_runtime_callee_candidate_count = placeholder_runtime_callee_candidates.len();
    let placeholder_runtime_namespace_candidate_count =
        placeholder_runtime_namespace_candidates.len();
    let detected_kind_counts = count_detected_react_functions_by_kind(&react_functions);
    let (detected_component_functions, detected_hook_functions) =
        collect_detected_react_function_names_by_kind(&react_functions);
    let metadata = ParseMetadata {
        statement_count: original_statement_count,
        statement_count_after_transform: transformed_statement_count,
        placeholder_runtime_helper_import_count_before_transform:
            runtime_helper_import_count_before_transform,
        placeholder_runtime_helper_import_count_after_transform:
            runtime_helper_import_count_after_transform,
        placeholder_runtime_helper_import_added: runtime_helper_import_added,
        placeholder_runtime_callee_reused,
        placeholder_runtime_callee_generated,
        placeholder_transform_status,
        placeholder_transform_candidate_count: placeholder_transform_candidates.len(),
        placeholder_transform_skipped_count: placeholder_transform_skipped_functions.len(),
        placeholder_transform_candidate_component_count: candidate_kind_counts.component_count,
        placeholder_transform_candidate_hook_count: candidate_kind_counts.hook_count,
        placeholder_transform_transformed_component_count: transformed_kind_counts.component_count,
        placeholder_transform_transformed_hook_count: transformed_kind_counts.hook_count,
        placeholder_transform_skipped_component_count: skipped_kind_counts.component_count,
        placeholder_transform_skipped_hook_count: skipped_kind_counts.hook_count,
        detected_component_function_count: detected_kind_counts.component_count,
        detected_hook_function_count: detected_kind_counts.hook_count,
        detected_component_functions,
        detected_hook_functions,
        placeholder_transform_candidates,
        placeholder_transform_skipped_functions,
        detected_react_functions: react_functions.len(),
        react_functions,
        placeholder_transforms_applied: transformed_count,
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
    };
    Ok((metadata, emit_module(cm, comments, module)?))
}
