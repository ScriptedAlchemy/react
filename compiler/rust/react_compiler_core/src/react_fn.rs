use std::collections::HashSet;

use crate::{ReactFunction, ReactFunctionKind};

pub(crate) fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .map(|ch| ch.is_ascii_uppercase())
        .unwrap_or(false)
}

pub(crate) fn is_hook_name(name: &str) -> bool {
    let mut chars = name.chars();
    if chars.next() != Some('u') || chars.next() != Some('s') || chars.next() != Some('e') {
        return false;
    }

    chars
        .next()
        .map(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        .unwrap_or(false)
}

pub(crate) fn react_function_kind(name: &str) -> Option<ReactFunctionKind> {
    if is_component_name(name) {
        Some(ReactFunctionKind::Component)
    } else if is_hook_name(name) {
        Some(ReactFunctionKind::Hook)
    } else {
        None
    }
}

pub(crate) fn sort_react_functions(functions: &mut [ReactFunction]) {
    functions.sort_by(|a, b| react_function_sort_key(a).cmp(&react_function_sort_key(b)));
}

pub(crate) fn sort_and_dedup_names(names: &mut Vec<String>) {
    names.sort();
    names.dedup();
}

pub(crate) fn compute_placeholder_transform_skipped_functions(
    candidates: &[String],
    transformed: &[String],
) -> Vec<String> {
    let transformed_names: HashSet<&str> = transformed.iter().map(String::as_str).collect();
    candidates
        .iter()
        .filter(|candidate| !transformed_names.contains(candidate.as_str()))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PlaceholderTransformKindCounts {
    pub(crate) component_count: usize,
    pub(crate) hook_count: usize,
}

pub(crate) fn count_placeholder_transform_names_by_kind(
    names: &[String],
    react_functions: &[ReactFunction],
) -> PlaceholderTransformKindCounts {
    names.iter().fold(
        PlaceholderTransformKindCounts::default(),
        |mut counts, name| {
            let kind = react_functions
                .iter()
                .find(|function| function.name == *name)
                .map(|function| function.kind.clone())
                .unwrap_or_else(|| {
                    if is_hook_name(name) {
                        ReactFunctionKind::Hook
                    } else {
                        ReactFunctionKind::Component
                    }
                });
            match kind {
                ReactFunctionKind::Component => counts.component_count += 1,
                ReactFunctionKind::Hook => counts.hook_count += 1,
            }
            counts
        },
    )
}

pub(crate) fn count_detected_react_functions_by_kind(
    react_functions: &[ReactFunction],
) -> PlaceholderTransformKindCounts {
    react_functions.iter().fold(
        PlaceholderTransformKindCounts::default(),
        |mut counts, function| {
            match function.kind {
                ReactFunctionKind::Component => counts.component_count += 1,
                ReactFunctionKind::Hook => counts.hook_count += 1,
            }
            counts
        },
    )
}

pub(crate) fn collect_detected_react_function_names_by_kind(
    react_functions: &[ReactFunction],
) -> (Vec<String>, Vec<String>) {
    react_functions.iter().fold(
        (Vec::new(), Vec::new()),
        |(mut component_names, mut hook_names), function| {
            match function.kind {
                ReactFunctionKind::Component => component_names.push(function.name.clone()),
                ReactFunctionKind::Hook => hook_names.push(function.name.clone()),
            }
            (component_names, hook_names)
        },
    )
}

pub(crate) fn derive_placeholder_transform_status(
    apply_placeholder_transforms: bool,
    is_module: bool,
    candidate_count: usize,
    transformed_count: usize,
    runtime_callee_available_before_transform: bool,
) -> &'static str {
    if !apply_placeholder_transforms {
        return "disabled";
    }
    if candidate_count == 0 {
        return "no_candidates";
    }
    if transformed_count > 0 {
        return "transformed";
    }
    if !is_module && !runtime_callee_available_before_transform {
        return "blocked_missing_runtime_callee";
    }
    "no_op"
}

fn react_function_sort_key(function: &ReactFunction) -> (usize, usize, usize, usize, &str, &str) {
    let (start_line, start_column, end_line, end_column) = match &function.loc {
        Some(loc) => (
            loc.start_line,
            loc.start_column,
            loc.end_line,
            loc.end_column,
        ),
        None => (usize::MAX, usize::MAX, usize::MAX, usize::MAX),
    };
    let kind = match function.kind {
        ReactFunctionKind::Component => "component",
        ReactFunctionKind::Hook => "hook",
    };
    (
        start_line,
        start_column,
        end_line,
        end_column,
        function.name.as_str(),
        kind,
    )
}
