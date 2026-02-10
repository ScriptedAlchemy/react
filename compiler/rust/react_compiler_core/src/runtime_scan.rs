use std::collections::HashSet;

#[derive(Debug, Default)]
pub(crate) struct RuntimeMemoCalleeScan {
    pub(crate) runtime_namespace_bindings: HashSet<String>,
    pub(crate) runtime_callee_bindings: HashSet<String>,
}

pub(crate) fn select_runtime_callee_name(
    runtime_callee_bindings: &HashSet<String>,
) -> Option<String> {
    sorted_runtime_callee_candidates(runtime_callee_bindings)
        .into_iter()
        .next()
}

pub(crate) fn sorted_runtime_callee_candidates(
    runtime_callee_bindings: &HashSet<String>,
) -> Vec<String> {
    let mut candidates: Vec<String> = runtime_callee_bindings.iter().cloned().collect();
    candidates.sort();
    candidates
}

pub(crate) fn sorted_runtime_namespace_candidates(
    runtime_namespace_bindings: &HashSet<String>,
) -> Vec<String> {
    let mut candidates: Vec<String> = runtime_namespace_bindings.iter().cloned().collect();
    candidates.sort();
    candidates
}
