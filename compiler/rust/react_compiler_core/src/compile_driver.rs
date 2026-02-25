use swc_common::{comments::SingleThreadedComments, sync::Lrc, FileName, SourceMap, Spanned};
use swc_ecma_ast::EsVersion;
use swc_ecma_parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax, TsSyntax};

use crate::{
    compile_module::compile_module_output, compile_script::compile_script_output,
    helpers::span_to_location, parse::parse_syntax_error_reason, CompileOutput, CompilerError,
    CompilerOptions, InputDialect,
};

fn parser_syntax(dialect: InputDialect) -> Syntax {
    match dialect {
        InputDialect::JavaScript => Syntax::Es(EsSyntax {
            jsx: true,
            fn_bind: true,
            decorators: true,
            decorators_before_export: true,
            export_default_from: true,
            import_attributes: true,
            allow_super_outside_method: true,
            allow_return_outside_function: true,
            auto_accessors: true,
            explicit_resource_management: true,
        }),
        InputDialect::TypeScript => Syntax::Typescript(TsSyntax {
            tsx: true,
            decorators: true,
            dts: false,
            no_early_errors: true,
            disallow_ambiguous_jsx_like: false,
        }),
        InputDialect::Flow => Syntax::Es(EsSyntax {
            jsx: true,
            fn_bind: true,
            decorators: true,
            decorators_before_export: true,
            export_default_from: true,
            import_attributes: true,
            allow_super_outside_method: true,
            allow_return_outside_function: true,
            auto_accessors: true,
            explicit_resource_management: true,
        }),
    }
}

fn map_parse_error(
    err: swc_ecma_parser::error::Error,
    options: &CompilerOptions,
    cm: &Lrc<SourceMap>,
) -> CompilerError {
    let message = err.kind().msg().to_string();
    let reason = parse_syntax_error_reason(err.kind());
    let location = span_to_location(cm, err.span());
    if options.dialect == InputDialect::Flow {
        CompilerError::UnsupportedFlowSyntax { location }
    } else {
        CompilerError::ParseFailure {
            message,
            reason,
            location,
        }
    }
}

pub fn compile(source: &str, options: &CompilerOptions) -> Result<CompileOutput, CompilerError> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom(options.filename.clone()).into(),
        source.to_string(),
    );

    let comments = SingleThreadedComments::default();
    let lexer = Lexer::new(
        parser_syntax(options.dialect),
        EsVersion::EsNext,
        StringInput::from(&*fm),
        Some(&comments),
    );

    let mut parser = Parser::new_from(lexer);
    let (metadata, code) = if options.is_module {
        let mut module = parser
            .parse_module()
            .map_err(|err| map_parse_error(err, options, &cm))?;
        compile_module_output(&cm, &comments, &mut module, options)?
    } else {
        let mut script = parser
            .parse_script()
            .map_err(|err| map_parse_error(err, options, &cm))?;
        compile_script_output(&cm, &comments, &mut script, options)?
    };

    Ok(CompileOutput { code, metadata })
}
