mod binding;
mod compile_driver;
mod compile_module;
mod compile_script;
mod emit;
mod entrypoint;
mod error;
mod helpers;
mod model;
mod parse;
mod placeholder;
mod placeholder_apply;
mod react_detect;
mod react_fn;
mod runtime_binding_utils;
mod runtime_class;
mod runtime_clear;
mod runtime_expr;
mod runtime_helpers;
mod runtime_jsx;
mod runtime_scan;
mod runtime_scope;
mod runtime_stmt;
mod runtime_traversal;

pub use error::CompilerError;
pub use model::{
    render_react_functions_debug, CompileOutput, CompilerOptions, InputDialect, ParseMetadata,
    ReactFunction, ReactFunctionKind, SourceLocation, DEFAULT_EXPORT_COMPONENT_NAME,
};
pub use compile_driver::compile;
pub(crate) use runtime_expr::collect_runtime_bindings_from_expression;

#[cfg(test)]
mod tests {
    use super::{
        compile, render_react_functions_debug, CompilerError, CompilerOptions, InputDialect,
    };

    #[test]
    fn parses_javascript_source() {
        let output = compile(
            "export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.statement_count_after_transform, 2);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_before_transform,
            0
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_after_transform,
            1
        );
        assert!(output.metadata.placeholder_runtime_helper_import_added);
        assert!(!output.metadata.placeholder_runtime_callee_reused);
        assert!(output.metadata.placeholder_runtime_callee_generated);
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec!["Component".to_string()]
        );
        assert!(output
            .metadata
            .placeholder_transform_skipped_functions
            .is_empty());
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_status, "transformed");
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 1);
        assert_eq!(output.metadata.detected_hook_function_count, 0);
        assert_eq!(
            output.metadata.detected_component_functions,
            vec!["Component".to_string()]
        );
        assert!(output.metadata.detected_hook_functions.is_empty());
        assert_eq!(output.metadata.react_functions.len(), 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert_eq!(
            output.metadata.placeholder_transformed_functions,
            vec!["Component".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("_c")
        );
        assert!(output
            .metadata
            .placeholder_runtime_callee_name_before_transform
            .is_none());
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["_c".to_string()]
        );
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 1);
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            0
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates_before_transform
            .is_empty());
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidate_count_before_transform,
            0
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert_eq!(output.metadata.placeholder_runtime_namespace_candidate_count, 0);
        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn preserves_leading_pragma_comments() {
        let output = compile(
            "// @enableFlowSuppressions\nexport function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("@enableFlowSuppressions"));
    }

    #[test]
    fn detects_fixture_entrypoint_function_when_name_is_not_react_like() {
        let output = compile(
            "function component(){ return 1; } export const FIXTURE_ENTRYPOINT = { fn: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn detects_fixture_entrypoint_function_referenced_via_alias() {
        let output = compile(
            "function component(){ return 1; } const alias = component; export const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_computed_object_key() {
        let output = compile(
            "function component(){ return 1; } export const FIXTURE_ENTRYPOINT = { ['fn']: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_escaped_string_object_key() {
        let output = compile(
            "function component(){ return 1; } export const FIXTURE_ENTRYPOINT = { ['\\x66n']: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_unicode_escaped_string_object_key() {
        let output = compile(
            "function component(){ return 1; } export const FIXTURE_ENTRYPOINT = { ['\\u0066n']: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_template_literal_object_key() {
        let output = compile(
            "function component(){ return 1; } export const FIXTURE_ENTRYPOINT = { [`fn`]: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_escaped_template_literal_object_key() {
        let output = compile(
            "function component(){ return 1; } export const FIXTURE_ENTRYPOINT = { [`\\x66n`]: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_unicode_escaped_template_literal_object_key() {
        let output = compile(
            "function component(){ return 1; } export const FIXTURE_ENTRYPOINT = { [`\\u0066n`]: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_shorthand_object_key() {
        let output = compile(
            "function component(){ return 1; } const fn = component; export const FIXTURE_ENTRYPOINT = { fn, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_referenced_via_assigned_alias() {
        let output = compile(
            "function component(){ return 1; } let alias; alias = component; export const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_computed_member_assignment() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT['fn'] = component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_escaped_string_member_assignment() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT['\\x66n'] = component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_unicode_escaped_string_member_assignment() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT['\\u0066n'] = component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_template_literal_member_assignment() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT[`fn`] = component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_escaped_template_literal_member_assignment() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT[`\\x66n`] = component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_unicode_escaped_template_literal_member_assignment() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT[`\\u0066n`] = component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_assigned_object_literal() {
        let output = compile(
            "function component(){ return 1; } let FIXTURE_ENTRYPOINT; FIXTURE_ENTRYPOINT = { fn: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_parenthesized_object_literal_var_initializer() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = ({ fn: component, params: [] });",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                apply_placeholder_transforms: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_member_assignment() {
        let output = compile(
            "function component(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT.fn = component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_assigned_function_expression() {
        let output = compile(
            "let alias; alias = () => 1; export const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "alias");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn detects_fixture_entrypoint_function_from_assigned_function_expression_with_matching_name() {
        let output = compile(
            "let component; component = () => 1; export const FIXTURE_ENTRYPOINT = { fn: component, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_when_name_is_not_react_like() {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = { fn: render, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_referenced_via_alias() {
        let output = compile(
            "function render(){ return 1; } const alias = render; const FIXTURE_ENTRYPOINT = { fn: alias, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_from_template_literal_object_key() {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = { [`fn`]: render, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_from_unicode_escaped_template_literal_object_key(
    ) {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = { [`\\u0066n`]: render, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_from_escaped_string_object_key() {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = { ['\\x66n']: render, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_from_unicode_escaped_string_object_key() {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = { ['\\u0066n']: render, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_from_template_literal_member_assignment() {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT[`fn`] = render;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_from_unicode_escaped_template_literal_member_assignment(
    ) {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT[`\\u0066n`] = render;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_from_escaped_string_member_assignment() {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT['\\x66n'] = render;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_function_from_unicode_escaped_string_member_assignment() {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = { params: [] }; FIXTURE_ENTRYPOINT['\\u0066n'] = render;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_script_fixture_entrypoint_from_parenthesized_object_literal_var_initializer() {
        let output = compile(
            "function render(){ return 1; } const FIXTURE_ENTRYPOINT = ({ fn: render, params: [] });",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                apply_placeholder_transforms: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn transforms_script_fixture_entrypoint_function_when_name_is_not_react_like() {
        let output = compile(
            "const { c: _c } = require('react/compiler-runtime'); function render(){ return <div />; } const FIXTURE_ENTRYPOINT = { fn: render, params: [] };",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                is_module: false,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "render");
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn reuses_existing_runtime_cache_import_alias() {
        let output = compile(
            "import { c as cache } from 'react/compiler-runtime'; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("c as cache"));
        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_before_transform,
            1
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_after_transform,
            1
        );
        assert!(!output.metadata.placeholder_runtime_helper_import_added);
        assert!(output.metadata.placeholder_runtime_callee_reused);
        assert!(!output.metadata.placeholder_runtime_callee_generated);
    }

    #[test]
    fn reuses_existing_runtime_cache_require_member_alias_in_module() {
        let output = compile(
            "const cache = require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_require_member_alias_in_module() {
        let output = compile(
            "const cache = (0, require)('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = module.require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = (0, module.require)('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_module_computed_require_member_alias_in_module() {
        let output = compile(
            "const cache = module['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_module_escaped_computed_require_member_alias_in_module() {
        let output = compile(
            "const cache = module['\\x72equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_module_unicode_computed_require_member_alias_in_module() {
        let output = compile(
            "const cache = module['\\u0072equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = globalThis.module.require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_module_computed_require_member_alias_in_module() {
        let output = compile(
            "const cache = global.module['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_module_escaped_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = global.module['\\x72equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_module_unicode_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = global.module['\\u0072equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_window_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = window.module.require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_window_module_computed_require_member_alias_in_module() {
        let output = compile(
            "const cache = window.module['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_window_module_escaped_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = window.module['\\x72equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_window_module_unicode_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = window.module['\\u0072equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_window_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = globalThis.window.module.require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_self_module_computed_require_member_alias_in_module() {
        let output = compile(
            "const cache = global.self.module['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_computed_window_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis['window'].module.require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_window_computed_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis.window['module']['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_window_computed_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis.window['module'].require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_computed_window_computed_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis['window']['module']['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_computed_self_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = global['self'].module['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_global_this_window_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis.globalThis.window.module.require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_window_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis.window.module['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_global_self_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = global.global.self.module['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_computed_self_computed_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = global['self']['module']['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_computed_self_computed_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = global['self']['module'].require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_self_module_computed_require_member_alias_in_module() {
        let output = compile(
            "const cache = self.module['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_self_module_escaped_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = self.module['\\x72equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_self_module_unicode_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = self.module['\\u0072equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_window_computed_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = window['module'].require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_window_computed_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = window['module']['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_self_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = self.module.require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_self_computed_module_computed_require_member_alias_in_module() {
        let output = compile(
            "const cache = self['module']['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_window_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = (0, window.module.require)('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_self_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, self.module['require'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_computed_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = globalThis['module'].require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_module_computed_require_member_alias_in_module() {
        let output = compile(
            "const cache = globalThis.module['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_module_escaped_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis.module['\\x72equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_module_unicode_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis.module['\\u0072equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_self_computed_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = self['module'].require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_escaped_computed_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis['\\x6dodule'].require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_unicode_computed_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis['\\u006dodule'].require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_computed_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis['module']['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_computed_module_escaped_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis['module']['\\x72equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_global_this_computed_module_unicode_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = globalThis['module']['\\u0072equire']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_module_require_member_alias_in_module() {
        let output = compile(
            "const cache = (0, globalThis.module.require)('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_window_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, globalThis.window.module.require)('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_global_this_window_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, globalThis.globalThis.window.module.require)('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_computed_window_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, globalThis['window'].module.require)('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_self_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, global.self.module['require'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_global_self_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, global.global.self.module['require'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_computed_self_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, global['self'].module['require'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_computed_module_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, globalThis['module'].require)('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_computed_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, globalThis['module']['require'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_escaped_computed_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, globalThis['\\x6dodule']['require'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_unicode_computed_module_unicode_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, globalThis['\\u006dodule']['\\u0072equire'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_window_module_then_require_member_alias_in_module() {
        let output = compile(
            "const cache = (0, window.module).require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_self_computed_module_then_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, self['module'])['require']('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_module_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, global.module['require'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_module_escaped_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, global.module['\\x72equire'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_module_unicode_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, global.module['\\u0072equire'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_module_escaped_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, globalThis.module['\\x72equire'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_global_this_module_unicode_computed_require_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = (0, globalThis.module['\\u0072equire'])('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_require_template_literal_member_alias_in_module() {
        let output = compile(
            "const cache = require(`react/compiler-runtime`).c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_require_escaped_template_literal_member_alias_in_module() {
        let output = compile(
            "const cache = require(`react/compiler-\\x72untime`).c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_require_unicode_escaped_template_literal_member_alias_in_module(
    ) {
        let output = compile(
            "const cache = require(`react/compiler-\\u0072untime`).c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_require_namespace_alias_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            1
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidate_count_before_transform,
            1
        );
        assert_eq!(output.metadata.placeholder_runtime_namespace_candidate_count, 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_sequence_require_namespace_alias_in_module() {
        let output = compile(
            "const runtime = (0, require)('react/compiler-runtime'); const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_mutated_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_escaped_string_namespace_alias_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_unicode_escaped_string_namespace_alias_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_escaped_template_literal_namespace_alias_in_module()
    {
        let output = compile(
            "const runtime = require(`react/compiler-\\x72untime`); const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_unicode_escaped_template_literal_namespace_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require(`react/compiler-\\u0072untime`); const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_namespace_escaped_string_member_alias_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime['\\x63']; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_escaped_string_namespace_escaped_string_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime['\\x63']; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_escaped_template_literal_namespace_escaped_string_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require(`react/compiler-\\x72untime`); const cache = runtime['\\x63']; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_unicode_escaped_template_literal_namespace_escaped_string_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require(`react/compiler-\\u0072untime`); const cache = runtime['\\x63']; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_unicode_escaped_string_namespace_escaped_string_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime['\\x63']; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_escaped_string_namespace_unicode_escaped_string_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime['\\u0063']; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_unicode_escaped_string_namespace_unicode_escaped_string_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime['\\u0063']; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_escaped_string_namespace_escaped_template_literal_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime[`\\x63`]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_unicode_escaped_string_namespace_escaped_template_literal_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime[`\\x63`]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_escaped_string_namespace_unicode_escaped_template_literal_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime[`\\u0063`]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_unicode_escaped_string_namespace_unicode_escaped_template_literal_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime[`\\u0063`]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_namespace_unicode_escaped_string_member_alias_in_module()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime['\\u0063']; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_namespace_escaped_template_literal_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime[`\\x63`]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_require_namespace_unicode_escaped_template_literal_member_alias_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime[`\\u0063`]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_update_expression_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_compound_assignment_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x += 1; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_logical_assignment_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x &&= unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_conditionally_reassigned_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x = unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_conditionally_deleted_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime.x); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.x); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_update_expression_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime.x++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_compound_assignment_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_uses_logical_assignment_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_conditionally_deleted_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime['x']); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.['x']); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_conditionally_reassigned_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] = unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_update_expression_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime['x']++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_compound_assignment_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_logical_assignment_in_nested_assignment_rhs_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_mutated_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_deleted_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime['x']; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_update_expression_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x']++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_compound_assignment_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] += 1; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_uses_logical_assignment_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] &&= unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_namespace_import_alias_in_module() {
        let output = compile(
            "import * as runtime from 'react/compiler-runtime'; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_require_escaped_string_member_alias_in_module() {
        let output = compile(
            "const cache = require('react/compiler-\\x72untime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_require_unicode_escaped_string_member_alias_in_module() {
        let output = compile(
            "const cache = require('react/compiler-\\u0072untime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_default_import_alias_in_module() {
        let output = compile(
            "import runtime from 'react/compiler-runtime'; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_namespace_destructure_assignment_alias_in_module() {
        let output = compile(
            "import * as runtime from 'react/compiler-runtime'; let cache; ({ c: cache } = runtime); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_require_destructure_assignment_alias_in_module() {
        let output = compile(
            "let runtime; runtime = require('react/compiler-runtime'); let cache; ({ c: cache } = runtime); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_require_shorthand_destructure_alias_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c } = runtime; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = c(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_module_require_destructure_alias_in_module() {
        let output = compile(
            "const { c: cache } = module.require('react/compiler-runtime'); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_module_computed_require_destructure_alias_in_module() {
        let output = compile(
            "const { c: cache } = module['require']('react/compiler-runtime'); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_module_require_destructure_alias_with_default_in_module() {
        let output = compile(
            "const { c: cache = fallback } = module.require('react/compiler-runtime'); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_runtime_namespace_destructure_alias_with_default_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c: cache = fallback } = runtime; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_module_require_computed_destructure_alias_with_default_in_module(
    ) {
        let output = compile(
            "const { ['c']: cache = fallback } = module.require('react/compiler-runtime'); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_runtime_namespace_computed_destructure_alias_with_default_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { ['c']: cache = fallback } = runtime; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_module_require_escaped_computed_destructure_alias_with_default_in_module(
    ) {
        let output = compile(
            "const { ['\\x63']: cache = fallback } = module.require('react/compiler-runtime'); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_module_require_unicode_computed_destructure_alias_with_default_in_module(
    ) {
        let output = compile(
            "const { ['\\u0063']: cache = fallback } = module.require('react/compiler-runtime'); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_runtime_namespace_escaped_computed_destructure_alias_with_default_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { ['\\x63']: cache = fallback } = runtime; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_runtime_namespace_unicode_computed_destructure_alias_with_default_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { ['\\u0063']: cache = fallback } = runtime; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_exported_require_member_alias_in_module() {
        let output = compile(
            "export const cache = require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_exported_require_namespace_alias_in_module() {
        let output = compile(
            "export const runtime = require('react/compiler-runtime'); export const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_import_alias_via_identifier_alias_in_module() {
        let output = compile(
            "import { c as cache0 } from 'react/compiler-runtime'; const cache = cache0; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_shadowed_runtime_callee_without_treating_it_as_namespace_in_module() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c: runtime } = require('react/compiler-runtime'); const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = runtime(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_require_alias_via_assignment_in_module() {
        let output = compile(
            "const { c: cache0 } = require('react/compiler-runtime'); let cache; cache = cache0; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_sequence_assignment_in_module() {
        let output = compile(
            "let cache; (cache = require('react/compiler-runtime').c, sideEffect()); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_sequence_assignment_rhs_in_module() {
        let output = compile(
            "let cache; cache = (sideEffect(), require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_call_argument_assignment_in_module() {
        let output = compile(
            "let cache; sideEffect(cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_new_expression_argument_assignment_in_module() {
        let output = compile(
            "let cache; new SideEffect(cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_tagged_template_assignment_in_module() {
        let output = compile(
            "let cache; tag`${cache = require('react/compiler-runtime').c}`; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_unary_assignment_in_module() {
        let output = compile(
            "let cache; void (cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_await_assignment_in_module() {
        let output = compile(
            "let cache; await (cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_for_init_assignment_in_module() {
        let output = compile(
            "let cache; for (cache = require('react/compiler-runtime').c; false;) {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_using_declaration_in_module() {
        let output = compile(
            "using cache = require('react/compiler-runtime').c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_jsx_child_assignment_in_module() {
        let output = compile(
            "let cache; <div>{cache = require('react/compiler-runtime').c}</div>; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_top_level_jsx_attr_assignment_in_module() {
        let output = compile(
            "let cache; <div data-cache={cache = require('react/compiler-runtime').c} />; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_block_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; { let cache; cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_class_static_block_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { let cache; cache = unknown; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_catch_param_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; try { throw 0; } catch (cache) { cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_switch_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; switch (value) { case 0: let cache; cache = unknown; break; default: break; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_computed_key_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { [cache = require('react/compiler-runtime').c](){} } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_static_block_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { cache = require('react/compiler-runtime').c; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_static_super_computed_assignment_in_module() {
        let output = compile(
            "let cache; class Base {} class RuntimeCarrier extends Base { static { super[cache = require('react/compiler-runtime').c]; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_super_class_assignment_in_module() {
        let output = compile(
            "let cache; class Base {} class RuntimeCarrier extends (cache = require('react/compiler-runtime').c, Base) {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_decorator_assignment_in_module() {
        let output = compile(
            "let cache; @((cache = require('react/compiler-runtime').c)) class RuntimeCarrier {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_static_for_init_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { for (cache = require('react/compiler-runtime').c; false;) {} } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_class_static_do_while_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { do { cache = require('react/compiler-runtime').c; } while (false); } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_when_class_static_for_in_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; class RuntimeCarrier { static { for (let cache in source) { break; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_class_static_for_of_decl_shadows_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = []; class RuntimeCarrier { static { for (const cache of source) { break; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_nested_class_static_block_assignment_in_module() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { class Nested { static { cache = require('react/compiler-runtime').c; } } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_export_default_class_static_block_assignment_in_module() {
        let output = compile(
            "let cache; export default class RuntimeCarrier { static { cache = require('react/compiler-runtime').c; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_export_default_expression_assignment_in_module() {
        let output = compile(
            "let cache; export default (cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_export_assignment_in_module() {
        let output = compile(
            "let cache; export = (cache = require('react/compiler-runtime').c); function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_import_equals_runtime_namespace() {
        let output = compile(
            "import Runtime = require('react/compiler-runtime'); const cache = Runtime.c; function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_import_equals_qualified_callee_alias() {
        let output = compile(
            "import Runtime = require('react/compiler-runtime'); import cache = Runtime.c; function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_enum_member_assignment_in_module() {
        let output = compile(
            "let cache; enum RuntimeCarrier { Value = (cache = require('react/compiler-runtime').c) } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_ts_module_assignment_in_module() {
        let output = compile(
            "let cache; namespace RuntimeCarrier { export const value = (cache = require('react/compiler-runtime').c); } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_ts_module_declares_shadowed_alias_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; namespace RuntimeCarrier { export let cache; cache = unknown; } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_assigned_in_class_static_block(
    ) {
        let output = compile(
            "let cache; class RuntimeCarrier { static { if (cond) { cache = require('react/compiler-runtime').c; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_ts_import_equals_runtime_alias_is_reassigned() {
        let output = compile(
            "import Runtime = require('react/compiler-runtime'); let cache = Runtime.c; cache = unknown; function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_assigned_via_optional_call()
    {
        let output = compile(
            "let cache; maybe?.(cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_assigned_via_optional_computed_member(
    ) {
        let output = compile(
            "let cache; maybe?.[cache = require('react/compiler-runtime').c]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn reuses_existing_runtime_cache_from_object_literal_assignment_in_module() {
        let output = compile(
            "let cache; const payload = {value: (cache = require('react/compiler-runtime').c)}; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_object_literal_computed_key_assignment_in_module() {
        let output = compile(
            "let cache; const payload = {[cache = require('react/compiler-runtime').c]: 1}; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_non_short_circuit_binary_assignment_in_module() {
        let output = compile(
            "let cache; const value = 1 + (cache = require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_template_literal_assignment_in_module() {
        let output = compile(
            "let cache; const value = `${cache = require('react/compiler-runtime').c}`; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_computed_member_assignment_in_module() {
        let output = compile(
            "let cache; const value = source[cache = require('react/compiler-runtime').c]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn reuses_existing_runtime_cache_from_sequence_declarator_in_module() {
        let output = compile(
            "const cache = (sideEffect(), require('react/compiler-runtime').c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache = unknown; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_uses_compound_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache += 1; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_uses_logical_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache &&= unknown; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_uses_update_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache++; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_overwritten_by_object_pattern_assignment()
    {
        let output = compile(
            "let cache; ({ c: cache } = require('react/compiler-runtime')); ({ x: cache } = source); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_sequence() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; (cache = unknown, sideEffect()); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cond && (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c = unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime.c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.c); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.['c']); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime.c++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime[prop]); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.[prop]); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime[prop]++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime[prop] += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime[prop] &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime['c']); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] = unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime['c']++); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] += 1)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] &&= unknown)); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_call_argument() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; sideEffect(cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_new_expression_argument() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; new SideEffect(cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_tagged_template() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; tag`${cache = unknown}`; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_unary_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; void (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_top_level_await() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; await (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_in_top_level_while(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; while (cond) { cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_in_top_level_switch(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; switch (value) { case 0: cache = unknown; break; default: break; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_mutated_via_top_level_for_in_pattern() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; for (cache in source) { break; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_top_level_jsx_child() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; <div>{cache = unknown}</div>; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_after_top_level_using_declaration(
    ) {
        let output = compile(
            "using cache = require('react/compiler-runtime').c; cache = unknown; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_block_decl_shadows_alias_mutation_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; { let cache; cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_class_static_block_decl_shadows_alias_mutation_in_module()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { let cache; cache = unknown; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn reuses_existing_runtime_cache_when_top_level_catch_param_shadows_alias_mutation_in_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; try { throw 0; } catch (cache) { cache = unknown; } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_via_optional_call(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; maybe?.(cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_via_optional_computed_member(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; maybe?.[cache = unknown]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_computed_key() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { [cache = unknown](){} } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_static_block() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { cache = unknown; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_static_super_computed()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class Base {} class RuntimeCarrier extends Base { static { super[cache = unknown]; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_super_class_expression()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class Base {} class RuntimeCarrier extends (cache = unknown, Base) {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_decorator_expression()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; @((cache = unknown)) class RuntimeCarrier {} export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_class_static_for_init() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { for (cache = unknown; false;) {} } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_mutated_via_class_static_for_in_pattern() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; class RuntimeCarrier { static { for (cache in source) { break; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_export_default_class_static_block(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; export default class RuntimeCarrier { static { cache = unknown; } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_in_class_static_while(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { while (cond) { cache = unknown; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_nested_class_static_block() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { class Nested { static { cache = unknown; } } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_export_default_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; export default (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_ts_export_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; export = (cache = unknown); function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_ts_enum_member() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; enum RuntimeCarrier { Value = (cache = unknown) } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_ts_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; namespace RuntimeCarrier { export const value = (cache = unknown); } export function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_conditionally_reassigned_in_class_static_block(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { if (cond) { cache = unknown; } } } export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_object_literal() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const payload = {value: (cache = unknown)}; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_object_literal_computed_key()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const payload = {[cache = unknown]: 1}; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_non_short_circuit_binary() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = 1 + (cache = unknown); export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_template_literal() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = `${cache = unknown}`; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_alias_is_reassigned_in_computed_member() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = source[cache = unknown]; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_reassigned() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_reassigned() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime.c; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_is_deleted_via_optional_chain()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.c; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_deleted_via_optional_chain(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.['c']; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_optional_chain_computed_member_may_target_c(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; delete runtime?.[prop]; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime['c']; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_update_expression() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_update_expression()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c']++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_compound_assignment() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c += 1; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_compound_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] += 1; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_c_member_uses_logical_assignment() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c &&= unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_c_member_uses_logical_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] &&= unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; runtime[prop] = unknown; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; delete runtime[prop]; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn falls_back_to_import_when_module_runtime_namespace_computed_member_may_target_c_uses_update_expression(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; runtime[prop]++; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output
            .code
            .contains("import { c as _c } from \"react/compiler-runtime\";"));
        assert!(output.code.contains("const $ = _c(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn transforms_script_component_with_runtime_require_destructure_alias() {
        let output = compile(
            "const { c: cache } = require('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert_eq!(
            output.metadata.placeholder_transformed_functions,
            vec!["Component".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_shorthand_destructure_alias() {
        let output = compile(
            "const { c } = require('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = c(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_module_require_destructure_alias() {
        let output = compile(
            "const { c: cache } = module.require('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_module_computed_require_destructure_alias() {
        let output = compile(
            "const { c: cache } = module['require']('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_module_require_destructure_alias_with_default() {
        let output = compile(
            "const { c: cache = fallback } = module.require('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_module_computed_destructure_alias_with_default() {
        let output = compile(
            "const { ['c']: cache = fallback } = module.require('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_module_escaped_computed_destructure_alias_with_default(
    ) {
        let output = compile(
            "const { ['\\x63']: cache = fallback } = module.require('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_module_unicode_computed_destructure_alias_with_default(
    ) {
        let output = compile(
            "const { ['\\u0063']: cache = fallback } = module.require('react/compiler-runtime'); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_chain() {
        let output = compile(
            "const { c: cache0 } = require('react/compiler-runtime'); const cache = cache0; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_using_shadowed_runtime_callee_alias() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c: runtime } = require('react/compiler-runtime'); const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = runtime(0);"));
        assert!(!output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_assignment_alias_chain() {
        let output = compile(
            "const { c: cache0 } = require('react/compiler-runtime'); let cache; cache = cache0; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_sequence_assignment() {
        let output = compile(
            "let cache; (cache = require('react/compiler-runtime').c, sideEffect()); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_sequence_assignment_rhs() {
        let output = compile(
            "let cache; cache = (sideEffect(), require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_call_argument_assignment() {
        let output = compile(
            "let cache; sideEffect(cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_new_expression_argument_assignment() {
        let output = compile(
            "let cache; new SideEffect(cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_tagged_template_assignment() {
        let output = compile(
            "let cache; tag`${cache = require('react/compiler-runtime').c}`; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_unary_assignment() {
        let output = compile(
            "let cache; void (cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_computed_key_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { [cache = require('react/compiler-runtime').c](){} } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_static_block_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { cache = require('react/compiler-runtime').c; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_static_super_computed_assignment()
    {
        let output = compile(
            "let cache; class Base {} class RuntimeCarrier extends Base { static { super[cache = require('react/compiler-runtime').c]; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_static_for_init_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { for (cache = require('react/compiler-runtime').c; false;) {} } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_top_level_for_init_assignment() {
        let output = compile(
            "let cache; for (cache = require('react/compiler-runtime').c; false;) {} function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_top_level_using_declaration() {
        let output = compile(
            "using cache = require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_ts_enum_member_assignment() {
        let output = compile(
            "let cache; enum RuntimeCarrier { Value = (cache = require('react/compiler-runtime').c) } function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_ts_module_assignment() {
        let output = compile(
            "let cache; namespace RuntimeCarrier { export const value = (cache = require('react/compiler-runtime').c); } function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_top_level_jsx_child_assignment() {
        let output = compile(
            "let cache; <div>{cache = require('react/compiler-runtime').c}</div>; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_top_level_block_decl_shadows_runtime_alias_mutation() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; { let cache; cache = unknown; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_class_static_block_decl_shadows_runtime_alias_mutation() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { let cache; cache = unknown; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_top_level_catch_param_shadows_runtime_alias_mutation() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; try { throw 0; } catch (cache) { cache = unknown; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_top_level_switch_decl_shadows_runtime_alias_mutation() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; switch (value) { case 0: let cache; cache = unknown; break; default: break; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_class_static_do_while_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { do { cache = require('react/compiler-runtime').c; } while (false); } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_when_class_static_for_in_decl_shadows_alias() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; class RuntimeCarrier { static { for (let cache in source) { break; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_nested_class_static_block_assignment() {
        let output = compile(
            "let cache; class RuntimeCarrier { static { class Nested { static { cache = require('react/compiler-runtime').c; } } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_assigned_in_class_static_block(
    ) {
        let output = compile(
            "let cache; class RuntimeCarrier { static { if (cond) { cache = require('react/compiler-runtime').c; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 1);
        assert_eq!(output.metadata.detected_hook_function_count, 0);
        assert_eq!(
            output.metadata.detected_component_functions,
            vec!["Component".to_string()]
        );
        assert!(output.metadata.detected_hook_functions.is_empty());
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_assigned_via_optional_call(
    ) {
        let output = compile(
            "let cache; maybe?.(cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_assigned_via_optional_computed_member(
    ) {
        let output = compile(
            "let cache; maybe?.[cache = require('react/compiler-runtime').c]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_object_literal_assignment() {
        let output = compile(
            "let cache; const payload = {value: (cache = require('react/compiler-runtime').c)}; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_object_literal_computed_key_assignment()
    {
        let output = compile(
            "let cache; const payload = {[cache = require('react/compiler-runtime').c]: 1}; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_non_short_circuit_binary_assignment() {
        let output = compile(
            "let cache; const value = 1 + (cache = require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_template_literal_assignment() {
        let output = compile(
            "let cache; const value = `${cache = require('react/compiler-runtime').c}`; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_computed_member_assignment() {
        let output = compile(
            "let cache; const value = source[cache = require('react/compiler-runtime').c]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_alias_from_sequence_declarator() {
        let output = compile(
            "const cache = (sideEffect(), require('react/compiler-runtime').c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache = unknown; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_sequence() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; (cache = unknown, sideEffect()); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cond && (cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c = unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime.c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.c); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.['c']); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime.c++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.c &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime[prop]); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.[prop]); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime[prop]++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime[prop] += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime[prop] &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime['c']); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] = unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime['c']++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['c'] &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_call_argument() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; sideEffect(cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_new_expression_argument(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; new SideEffect(cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_tagged_template() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; tag`${cache = unknown}`; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_unary_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; void (cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_via_optional_call(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; maybe?.(cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_via_optional_computed_member(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; maybe?.[cache = unknown]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_class_computed_key()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { [cache = unknown](){} } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_class_static_block()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { cache = unknown; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_class_static_super_computed(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class Base {} class RuntimeCarrier extends Base { static { super[cache = unknown]; } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_class_static_for_init(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { for (cache = unknown; false;) {} } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_in_top_level_while(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; while (cond) { cache = unknown; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_in_top_level_switch(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; switch (value) { case 0: cache = unknown; break; default: break; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_mutated_via_top_level_for_in_pattern(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; for (cache in source) { break; } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_top_level_jsx_child()
    {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; <div>{cache = unknown}</div>; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_after_top_level_using_declaration(
    ) {
        let output = compile(
            "using cache = require('react/compiler-runtime').c; cache = unknown; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_ts_enum_member() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; enum RuntimeCarrier { Value = (cache = unknown) } function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_ts_module() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; namespace RuntimeCarrier { export const value = (cache = unknown); } function Component(){ return null; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.ts".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_mutated_via_class_static_for_in_pattern(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const source = {}; class RuntimeCarrier { static { for (cache in source) { break; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_in_class_static_block(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { if (cond) { cache = unknown; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_conditionally_reassigned_in_class_static_while(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { while (cond) { cache = unknown; } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_nested_class_static_block(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; class RuntimeCarrier { static { class Nested { static { cache = unknown; } } } } function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_object_literal() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const payload = {value: (cache = unknown)}; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_object_literal_computed_key(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const payload = {[cache = unknown]: 1}; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_non_short_circuit_binary(
    ) {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = 1 + (cache = unknown); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_template_literal() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = `${cache = unknown}`; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_reassigned_in_computed_member() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; const value = source[cache = unknown]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_reassigned() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_reassigned()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; runtime[prop] = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_is_deleted(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; delete runtime[prop]; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_member_may_target_c_uses_update_expression(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; runtime[prop]++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_uses_compound_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache += 1; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_uses_logical_assignment() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache &&= unknown; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_deleted() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; delete cache; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime.c; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_is_deleted_via_optional_chain(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.c; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_deleted_via_optional_chain(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.['c']; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_optional_chain_computed_member_may_target_c(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const prop = maybe ? 'c' : 'x'; delete runtime?.[prop]; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
        assert!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform
                .is_empty()
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name_before_transform.is_none());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert!(output.metadata.placeholder_runtime_callee_candidates.is_empty());
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime['c']; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_update_expression() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_update_expression(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c']++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_compound_assignment()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c += 1; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_compound_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] += 1; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_c_member_uses_logical_assignment()
    {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.c &&= unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_namespace_computed_c_member_uses_logical_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['c'] &&= unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_uses_update_expression() {
        let output = compile(
            "let cache = require('react/compiler-runtime').c; cache++; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_transform_script_component_when_runtime_alias_is_overwritten_by_object_pattern_assignment(
    ) {
        let output = compile(
            "let cache; ({ c: cache } = require('react/compiler-runtime')); ({ x: cache } = source); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert!(!output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_member_alias() {
        let output = compile(
            "const cache = require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_require_member_alias() {
        let output = compile(
            "const cache = (0, require)('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_module_require_member_alias() {
        let output = compile(
            "const cache = module.require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_module_require_member_alias() {
        let output = compile(
            "const cache = (0, module.require)('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_module_computed_require_member_alias() {
        let output = compile(
            "const cache = module['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_module_escaped_computed_require_member_alias() {
        let output = compile(
            "const cache = module['\\x72equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_module_unicode_computed_require_member_alias() {
        let output = compile(
            "const cache = module['\\u0072equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_module_require_member_alias() {
        let output = compile(
            "const cache = globalThis.module.require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_module_computed_require_member_alias() {
        let output = compile(
            "const cache = global.module['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_module_escaped_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = global.module['\\x72equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_module_unicode_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = global.module['\\u0072equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_window_module_require_member_alias() {
        let output = compile(
            "const cache = window.module.require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_window_module_computed_require_member_alias() {
        let output = compile(
            "const cache = window.module['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_window_module_escaped_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = window.module['\\x72equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_window_module_unicode_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = window.module['\\u0072equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_window_module_require_member_alias() {
        let output = compile(
            "const cache = globalThis.window.module.require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_self_module_computed_require_member_alias() {
        let output = compile(
            "const cache = global.self.module['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_computed_window_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis['window'].module.require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_window_computed_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis.window['module']['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_window_computed_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis.window['module'].require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_computed_window_computed_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis['window']['module']['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_computed_self_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = global['self'].module['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_global_this_window_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis.globalThis.window.module.require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_window_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis.window.module['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_global_self_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = global.global.self.module['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_computed_self_computed_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = global['self']['module']['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_computed_self_computed_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = global['self']['module'].require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_self_module_computed_require_member_alias() {
        let output = compile(
            "const cache = self.module['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_self_module_escaped_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = self.module['\\x72equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_self_module_unicode_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = self.module['\\u0072equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_window_computed_module_require_member_alias() {
        let output = compile(
            "const cache = window['module'].require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_window_computed_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = window['module']['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_self_module_require_member_alias() {
        let output = compile(
            "const cache = self.module.require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_self_computed_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = self['module']['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_window_module_require_member_alias() {
        let output = compile(
            "const cache = (0, window.module.require)('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_self_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, self.module['require'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_computed_module_require_member_alias() {
        let output = compile(
            "const cache = globalThis['module'].require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_module_computed_require_member_alias()
    {
        let output = compile(
            "const cache = globalThis.module['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_module_escaped_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis.module['\\x72equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_module_unicode_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis.module['\\u0072equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_self_computed_module_require_member_alias() {
        let output = compile(
            "const cache = self['module'].require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_escaped_computed_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis['\\x6dodule'].require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_unicode_computed_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis['\\u006dodule'].require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_computed_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis['module']['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_computed_module_escaped_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis['module']['\\x72equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_global_this_computed_module_unicode_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = globalThis['module']['\\u0072equire']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_module_require_member_alias() {
        let output = compile(
            "const cache = (0, globalThis.module.require)('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_window_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, globalThis.window.module.require)('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_global_this_window_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, globalThis.globalThis.window.module.require)('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_computed_window_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, globalThis['window'].module.require)('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_self_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, global.self.module['require'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_global_self_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, global.global.self.module['require'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_computed_self_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, global['self'].module['require'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_computed_module_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, globalThis['module'].require)('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_computed_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, globalThis['module']['require'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_escaped_computed_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, globalThis['\\x6dodule']['require'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_unicode_computed_module_unicode_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, globalThis['\\u006dodule']['\\u0072equire'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_window_module_then_require_member_alias() {
        let output = compile(
            "const cache = (0, window.module).require('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_self_computed_module_then_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, self['module'])['require']('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_module_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, global.module['require'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_module_escaped_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, global.module['\\x72equire'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_module_unicode_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, global.module['\\u0072equire'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_module_escaped_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, globalThis.module['\\x72equire'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_global_this_module_unicode_computed_require_member_alias(
    ) {
        let output = compile(
            "const cache = (0, globalThis.module['\\u0072equire'])('react/compiler-runtime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_escaped_string_member_alias() {
        let output = compile(
            "const cache = require('react/compiler-\\x72untime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_unicode_escaped_string_member_alias() {
        let output = compile(
            "const cache = require('react/compiler-\\u0072untime').c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_template_literal_member_alias() {
        let output = compile(
            "const cache = require(`react/compiler-runtime`).c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_escaped_template_literal_member_alias() {
        let output = compile(
            "const cache = require(`react/compiler-\\x72untime`).c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_unicode_escaped_template_literal_member_alias(
    ) {
        let output = compile(
            "const cache = require(`react/compiler-\\u0072untime`).c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_member_alias() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_with_runtime_sequence_require_namespace_member_alias() {
        let output = compile(
            "const runtime = (0, require)('react/compiler-runtime'); const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_escaped_string_namespace_alias() {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
    }

    #[test]
    fn transforms_script_component_with_runtime_require_unicode_escaped_string_namespace_alias() {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
    }

    #[test]
    fn transforms_script_component_with_runtime_require_escaped_template_literal_namespace_alias()
    {
        let output = compile(
            "const runtime = require(`react/compiler-\\x72untime`); const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
    }

    #[test]
    fn transforms_script_component_with_runtime_require_unicode_escaped_template_literal_namespace_alias(
    ) {
        let output = compile(
            "const runtime = require(`react/compiler-\\u0072untime`); const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_escaped_string_member_alias() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime['\\x63']; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_escaped_template_literal_member_alias() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime[`\\x63`]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_mutated() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x = unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime.x); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.x); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime.x++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime.x &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_conditionally_deleted_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime['x']); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_conditionally_deleted_via_optional_chain_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = delete runtime?.['x']); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_conditionally_reassigned_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] = unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_update_expression_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = runtime['x']++); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_compound_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] += 1)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_logical_assignment_in_nested_assignment_rhs(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime.c; cond && (value = (runtime['x'] &&= unknown)); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_member_is_deleted_via_optional_chain_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.x; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn reuses_existing_runtime_cache_when_runtime_namespace_non_c_computed_member_is_deleted_via_optional_chain_in_module(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.['x']; const cache = runtime.c; export function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("const $ = cache(0);"));
        assert!(!output.code.contains("import { c as _c }"));
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_is_deleted_via_optional_chain() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.x; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_deleted_via_optional_chain(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime?.['x']; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_update_expression() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_compound_assignment() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x += 1; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_member_uses_logical_assignment() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime.x &&= unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_mutated() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] = unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_is_deleted() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); delete runtime['x']; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_update_expression(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x']++; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_compound_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] += 1; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_when_runtime_namespace_non_c_computed_member_uses_logical_assignment(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); runtime['x'] &&= unknown; const cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidates_before_transform,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_namespace_candidates,
            vec!["runtime".to_string()]
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_name_before_transform
                .as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_name.as_deref(),
            Some("cache")
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates_before_transform,
            vec!["cache".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_runtime_callee_candidates,
            vec!["cache".to_string()]
        );
    }

    #[test]
    fn transforms_script_component_with_assigned_runtime_namespace_member_alias() {
        let output = compile(
            "let runtime; runtime = require('react/compiler-runtime'); let cache; cache = runtime.c; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_escaped_string_namespace_escaped_string_member_alias(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime['\\x63']; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_escaped_template_literal_namespace_escaped_string_member_alias(
    ) {
        let output = compile(
            "const runtime = require(`react/compiler-\\x72untime`); const cache = runtime['\\x63']; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_unicode_escaped_template_literal_namespace_escaped_string_member_alias(
    ) {
        let output = compile(
            "const runtime = require(`react/compiler-\\u0072untime`); const cache = runtime['\\x63']; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_unicode_escaped_string_namespace_escaped_string_member_alias(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime['\\x63']; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_escaped_string_namespace_unicode_escaped_string_member_alias(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime['\\u0063']; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_unicode_escaped_string_namespace_unicode_escaped_string_member_alias(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime['\\u0063']; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_escaped_string_namespace_escaped_template_literal_member_alias(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime[`\\x63`]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_unicode_escaped_string_namespace_escaped_template_literal_member_alias(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime[`\\x63`]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_escaped_string_namespace_unicode_escaped_template_literal_member_alias(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\x72untime'); const cache = runtime[`\\u0063`]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_require_unicode_escaped_string_namespace_unicode_escaped_template_literal_member_alias(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-\\u0072untime'); const cache = runtime[`\\u0063`]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_unicode_escaped_template_literal_member_alias(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime[`\\u0063`]; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_unicode_escaped_string_member_alias() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const cache = runtime['\\u0063']; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_destructure_alias() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c: cache } = runtime; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_destructure_alias_with_default() {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { c: cache = fallback } = runtime; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_computed_destructure_alias_with_default(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { ['c']: cache = fallback } = runtime; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_escaped_computed_destructure_alias_with_default(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { ['\\x63']: cache = fallback } = runtime; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_runtime_namespace_unicode_computed_destructure_alias_with_default(
    ) {
        let output = compile(
            "const runtime = require('react/compiler-runtime'); const { ['\\u0063']: cache = fallback } = runtime; function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn transforms_script_component_with_assigned_runtime_destructure_alias() {
        let output = compile(
            "let runtime; runtime = require('react/compiler-runtime'); let cache; ({ c: cache } = runtime); function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert!(output.code.contains("const $ = cache(0);"));
    }

    #[test]
    fn does_not_transform_script_component_without_runtime_binding() {
        let output = compile(
            "function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 0);
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec!["Component".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_transform_skipped_functions,
            vec!["Component".to_string()]
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_status,
            "blocked_missing_runtime_callee"
        );
        assert!(!output.metadata.placeholder_runtime_callee_reused);
        assert!(!output.metadata.placeholder_runtime_callee_generated);
        assert!(output.metadata.placeholder_transformed_functions.is_empty());
        assert!(output.metadata.placeholder_runtime_callee_name.is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_name_before_transform
            .is_none());
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates
            .is_empty());
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 0);
        assert!(output
            .metadata
            .placeholder_runtime_callee_candidates_before_transform
            .is_empty());
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            0
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates_before_transform
            .is_empty());
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidate_count_before_transform,
            0
        );
        assert!(output
            .metadata
            .placeholder_runtime_namespace_candidates
            .is_empty());
        assert_eq!(output.metadata.placeholder_runtime_namespace_candidate_count, 0);
        assert!(!output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_export_default_named_component() {
        let output = compile(
            "export default function Component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_export_default_anonymous_component() {
        let output = compile(
            "export default function (){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 1);
        assert_eq!(output.metadata.detected_hook_function_count, 0);
        assert_eq!(
            output.metadata.detected_component_functions,
            vec![super::DEFAULT_EXPORT_COMPONENT_NAME.to_string()]
        );
        assert!(output.metadata.detected_hook_functions.is_empty());
        assert_eq!(
            output.metadata.react_functions[0].name,
            super::DEFAULT_EXPORT_COMPONENT_NAME
        );
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec![super::DEFAULT_EXPORT_COMPONENT_NAME.to_string()]
        );
        assert!(output
            .metadata
            .placeholder_transform_skipped_functions
            .is_empty());
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            1
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_status, "transformed");
        assert!(!output.metadata.placeholder_runtime_callee_reused);
        assert!(output.metadata.placeholder_runtime_callee_generated);
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            0
        );
        assert_eq!(output.metadata.placeholder_runtime_namespace_candidate_count, 0);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_namespace_candidate_count_before_transform,
            0
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_export_default_arrow_component() {
        let output = compile(
            "export default () => <div />;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(
            output.metadata.react_functions[0].name,
            super::DEFAULT_EXPORT_COMPONENT_NAME
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_parenthesized_export_default_arrow_component() {
        let output = compile(
            "export default (() => <div />);",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(
            output.metadata.react_functions[0].name,
            super::DEFAULT_EXPORT_COMPONENT_NAME
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_typescript_asserted_export_default_arrow_component() {
        let output = compile(
            "export default ((() => <div />) as any);",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.tsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(
            output.metadata.react_functions[0].name,
            super::DEFAULT_EXPORT_COMPONENT_NAME
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_named_default_export_component_even_when_name_is_not_react_like() {
        let output = compile(
            "export default function component(){ return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_default_export_identifier() {
        let output = compile(
            "function component(){ return <div />; } export default component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_aliased_default_export_identifier() {
        let output = compile(
            "function component(){ return <div />; } const alias = component; export default alias;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_assigned_default_export_identifier() {
        let output = compile(
            "function component(){ return <div />; } let alias; alias = component; export default alias;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_assigned_function_expression_default_export_identifier() {
        let output = compile(
            "let alias; alias = () => <div />; export default alias;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "alias");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_assigned_function_expression_default_export_with_component_name(
    ) {
        let output = compile(
            "let component; component = () => <div />; export default component;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_multi_aliased_default_export_identifier() {
        let output = compile(
            "function component(){ return <div />; } const aliasA = component; const aliasB = aliasA; export default aliasB;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_named_default_export_specifier() {
        let output = compile(
            "function component(){ return <div />; } export {component as default};",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_react_like_assignment_expression_in_module() {
        let output = compile(
            "let Component; Component = () => <div />; export {Component};",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
        assert_eq!(output.metadata.statement_count, 3);
        assert_eq!(output.metadata.statement_count_after_transform, 4);
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_assigned_function_expression_named_default_export_specifier(
    ) {
        let output = compile(
            "let component; component = () => <div />; export {component as default};",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn transforms_function_referenced_by_aliased_named_default_export_specifier() {
        let output = compile(
            "function component(){ return <div />; } const alias = component; export {alias as default};",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "component");
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn does_not_duplicate_existing_placeholder_memo_stmt() {
        let output = compile(
            "import { c as _c } from 'react/compiler-runtime'; export function Component(){ const $ = _c(0); return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.code.matches("const $ = _c(0);").count(), 1);
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn does_not_duplicate_existing_placeholder_memo_stmt_after_directive() {
        let output = compile(
            "import { c as _c } from 'react/compiler-runtime'; export function Component(){ \"use strict\"; const $ = _c(0); return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.code.matches("const $ = _c(0);").count(), 1);
        let directive_index = output
            .code
            .find("\"use strict\";")
            .expect("expected use strict directive to exist");
        let memo_index = output
            .code
            .find("const $ = _c(0);")
            .expect("expected memo statement to exist");
        assert!(directive_index < memo_index);
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn does_not_duplicate_existing_nonzero_placeholder_memo_stmt() {
        let output = compile(
            "import { c as _c } from 'react/compiler-runtime'; export function Component(){ const $ = _c(2); return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.code.matches("const $ = _c(2);").count(), 1);
        assert_eq!(output.code.matches("const $ = _c(0);").count(), 0);
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn does_not_duplicate_existing_placeholder_memo_stmt_with_let_binding() {
        let output = compile(
            "import { c as _c } from 'react/compiler-runtime'; export function Component(){ let $ = _c(0); return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        assert_eq!(output.code.matches("let $ = _c(0);").count(), 1);
        assert_eq!(output.code.matches("const $ = _c(0);").count(), 0);
        assert_eq!(output.code.matches("react/compiler-runtime").count(), 1);
    }

    #[test]
    fn inserts_placeholder_memo_stmt_after_function_directives() {
        let output = compile(
            "export function Component(){ \"use strict\"; return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid JavaScript to parse");

        let directive_index = output
            .code
            .find("\"use strict\";")
            .expect("expected use strict directive to exist");
        let memo_index = output
            .code
            .find("const $ = _c(0);")
            .expect("expected inserted memo statement to exist");
        assert!(directive_index < memo_index);
    }

    #[test]
    fn parses_typescript_source() {
        let output = compile(
            "export function id<T>(value: T): T { return value; }",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.tsx".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid TypeScript to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.statement_count_after_transform, 1);
        assert_eq!(output.metadata.detected_react_functions, 0);
        assert!(output.metadata.react_functions.is_empty());
        assert!(!output.code.contains("react/compiler-runtime"));
    }

    #[test]
    fn detects_hook_by_name() {
        let output = compile(
            "function useValue() { return 1; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 0);
        assert_eq!(output.metadata.detected_hook_function_count, 1);
        assert!(output.metadata.detected_component_functions.is_empty());
        assert_eq!(
            output.metadata.detected_hook_functions,
            vec!["useValue".to_string()]
        );
        assert_eq!(output.metadata.react_functions[0].name, "useValue");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Hook
        );
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec!["useValue".to_string()]
        );
        assert_eq!(
            output.metadata.placeholder_transform_skipped_functions,
            vec!["useValue".to_string()]
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_status,
            "blocked_missing_runtime_callee"
        );
        assert!(!output.code.contains("react/compiler-runtime"));
    }

    #[test]
    fn transforms_hook_in_module_mode_and_reports_hook_counts() {
        let output = compile(
            "export function useValue() { return 1; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: true,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.detected_component_function_count, 0);
        assert_eq!(output.metadata.detected_hook_function_count, 1);
        assert!(output.metadata.detected_component_functions.is_empty());
        assert_eq!(
            output.metadata.detected_hook_functions,
            vec!["useValue".to_string()]
        );
        assert_eq!(output.metadata.placeholder_transforms_applied, 1);
        assert_eq!(
            output.metadata.placeholder_transform_candidates,
            vec!["useValue".to_string()]
        );
        assert!(output
            .metadata
            .placeholder_transform_skipped_functions
            .is_empty());
        assert_eq!(output.metadata.placeholder_transform_candidate_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_count, 0);
        assert_eq!(
            output.metadata.placeholder_transform_candidate_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_candidate_hook_count, 1);
        assert_eq!(
            output.metadata.placeholder_transform_transformed_component_count,
            0
        );
        assert_eq!(output.metadata.placeholder_transform_transformed_hook_count, 1);
        assert_eq!(output.metadata.placeholder_transform_skipped_component_count, 0);
        assert_eq!(output.metadata.placeholder_transform_skipped_hook_count, 0);
        assert_eq!(output.metadata.placeholder_transform_status, "transformed");
        assert!(!output.metadata.placeholder_runtime_callee_reused);
        assert!(output.metadata.placeholder_runtime_callee_generated);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_before_transform,
            0
        );
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_helper_import_count_after_transform,
            1
        );
        assert!(output.metadata.placeholder_runtime_helper_import_added);
        assert_eq!(output.metadata.placeholder_runtime_callee_candidate_count, 1);
        assert_eq!(
            output
                .metadata
                .placeholder_runtime_callee_candidate_count_before_transform,
            0
        );
        assert!(output.code.contains("react/compiler-runtime"));
        assert!(output.code.contains("const $ = _c(0);"));
    }

    #[test]
    fn detects_react_function_from_variable_declarator() {
        let output = compile(
            "const Component = () => <div />;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
        assert!(!output.code.contains("react/compiler-runtime"));
    }

    #[test]
    fn detects_react_function_from_assignment_expression() {
        let output = compile(
            "let Component; Component = () => <div />;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 2);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
        assert_eq!(
            output.metadata.react_functions[0].kind,
            super::ReactFunctionKind::Component
        );
    }

    #[test]
    fn detects_react_function_from_parenthesized_variable_declarator() {
        let output = compile(
            "const Component = (() => <div />);",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
    }

    #[test]
    fn detects_react_function_from_typescript_asserted_variable_declarator() {
        let output = compile(
            "const Component = (() => <div />) as any;",
            &CompilerOptions {
                dialect: InputDialect::TypeScript,
                filename: "fixture.tsx".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect("expected valid source to parse");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
    }

    #[test]
    fn parses_flow_dialect_when_file_uses_only_javascript_syntax() {
        let output = compile(
            "/* @flow */\nfunction Component() { return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::Flow,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect("flow dialect should support plain JavaScript source");

        assert_eq!(output.metadata.statement_count, 1);
        assert_eq!(output.metadata.detected_react_functions, 1);
        assert_eq!(output.metadata.react_functions[0].name, "Component");
    }

    #[test]
    fn rejects_flow_until_ported() {
        let err = compile(
            "/* @flow */ function Component(props: {x: number}) { return props.x; }",
            &CompilerOptions {
                dialect: InputDialect::Flow,
                filename: "fixture.js".to_string(),
                ..CompilerOptions::default()
            },
        )
        .expect_err("flow is not implemented yet");

        match err {
            CompilerError::UnsupportedFlowSyntax { location } => {
                assert!(location.is_some());
            }
            other => panic!("expected unsupported flow syntax error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failures_include_source_location_metadata() {
        let err = compile(
            "const = 1;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { location, .. } => {
                assert!(location.is_some());
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_unexpected_token() {
        let err = compile(
            "const = 1;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "unexpected_token");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_unterminated_syntax() {
        let err = compile(
            "const value = /foo",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "unterminated_syntax");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_expected_token() {
        let err = compile(
            "const value = ;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "expected_token");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_unexpected_eof() {
        let err = compile(
            "function Component(",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "unexpected_eof");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_reason_is_classified_as_invalid_syntax() {
        let err = compile(
            "const \\u00ZZ = 1;",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "broken.js".to_string(),
                is_module: false,
                ..CompilerOptions::default()
            },
        )
        .expect_err("invalid JavaScript should fail parsing");

        match err {
            CompilerError::ParseFailure { reason, .. } => {
                assert_eq!(reason, "invalid_syntax");
            }
            other => panic!("expected parse failure error, got {other:?}"),
        }
    }

    #[test]
    fn react_function_metadata_order_is_stable() {
        let source = r#"
          function useAlpha() { return 1; }
          function Component() { return <div />; }
          function useBeta() { return 2; }
        "#;
        let options = CompilerOptions {
            dialect: InputDialect::JavaScript,
            filename: "fixture.jsx".to_string(),
            is_module: false,
            ..CompilerOptions::default()
        };

        let first = compile(source, &options)
            .expect("expected first compile to succeed")
            .metadata
            .react_functions;
        let second = compile(source, &options)
            .expect("expected second compile to succeed")
            .metadata
            .react_functions;

        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["useAlpha", "Component", "useBeta"]
        );
    }

    #[test]
    fn renders_react_function_debug_snapshot() {
        let output = compile(
            "function Component() { return <div />; } function useThing() { return 1; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                is_module: false,
                apply_placeholder_transforms: false,
            },
        )
        .expect("expected compile to succeed");

        let debug = render_react_functions_debug(&output.metadata);
        assert!(debug.contains("ReactiveFunctionsDebug v0"));
        assert!(debug.contains("statement_count=2"));
        assert!(debug.contains("statement_count_after_transform=2"));
        assert!(debug.contains("placeholder_runtime_helper_import_count_before_transform=0"));
        assert!(debug.contains("placeholder_runtime_helper_import_count_after_transform=0"));
        assert!(debug.contains("placeholder_runtime_helper_import_added=false"));
        assert!(debug.contains("placeholder_runtime_callee_reused=false"));
        assert!(debug.contains("placeholder_runtime_callee_generated=false"));
        assert!(debug.contains("placeholder_transform_candidates=Component,useThing"));
        assert!(debug.contains("placeholder_transform_skipped_functions=Component,useThing"));
        assert!(debug.contains("placeholder_transform_candidate_count=2"));
        assert!(debug.contains("placeholder_transform_skipped_count=2"));
        assert!(debug.contains("placeholder_transform_candidate_component_count=1"));
        assert!(debug.contains("placeholder_transform_candidate_hook_count=1"));
        assert!(debug.contains("placeholder_transform_transformed_component_count=0"));
        assert!(debug.contains("placeholder_transform_transformed_hook_count=0"));
        assert!(debug.contains("placeholder_transform_skipped_component_count=1"));
        assert!(debug.contains("placeholder_transform_skipped_hook_count=1"));
        assert!(debug.contains("placeholder_transform_status=disabled"));
        assert!(debug.contains("detected_component_function_count=1"));
        assert!(debug.contains("detected_hook_function_count=1"));
        assert!(debug.contains("detected_component_functions=Component"));
        assert!(debug.contains("detected_hook_functions=useThing"));
        assert!(debug.contains("detected_react_functions=2"));
        assert!(debug.contains("placeholder_transforms_applied=0"));
        assert!(debug.contains("placeholder_transformed_functions="));
        assert!(debug.contains("placeholder_runtime_callee_name=none"));
        assert!(debug.contains("placeholder_runtime_callee_name_before_transform=none"));
        assert!(debug.contains("placeholder_runtime_callee_candidates="));
        assert!(debug.contains("placeholder_runtime_callee_candidate_count=0"));
        assert!(debug.contains("placeholder_runtime_callee_candidates_before_transform="));
        assert!(debug.contains(
            "placeholder_runtime_callee_candidate_count_before_transform=0"
        ));
        assert!(debug.contains("placeholder_runtime_namespace_candidates="));
        assert!(debug.contains("placeholder_runtime_namespace_candidate_count=0"));
        assert!(debug.contains("placeholder_runtime_namespace_candidates_before_transform="));
        assert!(debug.contains(
            "placeholder_runtime_namespace_candidate_count_before_transform=0"
        ));
        assert!(debug.contains("name=Component kind=Component"));
        assert!(debug.contains("name=useThing kind=Hook"));
    }

    #[test]
    fn renders_react_function_debug_snapshot_with_transform_state() {
        let output = compile(
            "export function Component() { return <div />; }",
            &CompilerOptions {
                dialect: InputDialect::JavaScript,
                filename: "fixture.jsx".to_string(),
                is_module: true,
                apply_placeholder_transforms: true,
            },
        )
        .expect("expected compile to succeed");

        let debug = render_react_functions_debug(&output.metadata);
        assert!(debug.contains("ReactiveFunctionsDebug v0"));
        assert!(debug.contains("statement_count=1"));
        assert!(debug.contains("statement_count_after_transform=2"));
        assert!(debug.contains("placeholder_runtime_helper_import_count_before_transform=0"));
        assert!(debug.contains("placeholder_runtime_helper_import_count_after_transform=1"));
        assert!(debug.contains("placeholder_runtime_helper_import_added=true"));
        assert!(debug.contains("placeholder_runtime_callee_reused=false"));
        assert!(debug.contains("placeholder_runtime_callee_generated=true"));
        assert!(debug.contains("placeholder_transform_candidates=Component"));
        assert!(debug.contains("placeholder_transform_skipped_functions="));
        assert!(debug.contains("placeholder_transform_candidate_count=1"));
        assert!(debug.contains("placeholder_transform_skipped_count=0"));
        assert!(debug.contains("placeholder_transform_candidate_component_count=1"));
        assert!(debug.contains("placeholder_transform_candidate_hook_count=0"));
        assert!(debug.contains("placeholder_transform_transformed_component_count=1"));
        assert!(debug.contains("placeholder_transform_transformed_hook_count=0"));
        assert!(debug.contains("placeholder_transform_skipped_component_count=0"));
        assert!(debug.contains("placeholder_transform_skipped_hook_count=0"));
        assert!(debug.contains("placeholder_transform_status=transformed"));
        assert!(debug.contains("detected_component_function_count=1"));
        assert!(debug.contains("detected_hook_function_count=0"));
        assert!(debug.contains("detected_component_functions=Component"));
        assert!(debug.contains("detected_hook_functions="));
        assert!(debug.contains("placeholder_transforms_applied=1"));
        assert!(debug.contains("placeholder_transformed_functions=Component"));
        assert!(debug.contains("placeholder_runtime_callee_name=_c"));
        assert!(debug.contains("placeholder_runtime_callee_name_before_transform=none"));
        assert!(debug.contains("placeholder_runtime_callee_candidate_count=1"));
        assert!(debug.contains(
            "placeholder_runtime_callee_candidate_count_before_transform=0"
        ));
        assert!(debug.contains("placeholder_runtime_namespace_candidate_count=0"));
        assert!(debug.contains(
            "placeholder_runtime_namespace_candidate_count_before_transform=0"
        ));
        assert!(debug.contains("name=Component kind=Component"));
    }

    #[test]
    fn compile_output_and_debug_snapshot_are_deterministic_across_runs() {
        let source = r#"
          import {c as cache} from "react/compiler-runtime";
          export function Component() {
            return <div>{cache}</div>;
          }
        "#;
        let options = CompilerOptions {
            dialect: InputDialect::JavaScript,
            filename: "fixture.jsx".to_string(),
            is_module: true,
            apply_placeholder_transforms: true,
        };

        let first = compile(source, &options).expect("expected first compile to succeed");
        let second = compile(source, &options).expect("expected second compile to succeed");

        assert_eq!(first, second);
        assert_eq!(
            render_react_functions_debug(&first.metadata),
            render_react_functions_debug(&second.metadata)
        );
    }
}
