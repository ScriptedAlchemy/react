use std::collections::HashSet;

use swc_ecma_ast::{ImportDecl, ImportSpecifier};

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

pub(crate) fn collect_runtime_bindings_from_import_decl(
    import_decl: &ImportDecl,
    runtime_namespace_bindings: &mut HashSet<String>,
    runtime_callee_bindings: &mut HashSet<String>,
) {
    for specifier in &import_decl.specifiers {
        match specifier {
            ImportSpecifier::Named(named) => {
                if named.is_type_only {
                    continue;
                }
                let is_memo_runtime_import = named
                    .imported
                    .as_ref()
                    .map(|imported| imported.atom() == &"c")
                    .unwrap_or(named.local.sym == *"c");
                if is_memo_runtime_import {
                    let binding_name = named.local.sym.to_string();
                    runtime_callee_bindings.insert(binding_name.clone());
                    runtime_namespace_bindings.remove(binding_name.as_str());
                }
            }
            ImportSpecifier::Namespace(namespace) => {
                let binding_name = namespace.local.sym.to_string();
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
            ImportSpecifier::Default(default_import) => {
                let binding_name = default_import.local.sym.to_string();
                runtime_namespace_bindings.insert(binding_name.clone());
                runtime_callee_bindings.remove(binding_name.as_str());
            }
        }
    }
}
