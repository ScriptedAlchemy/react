use swc_common::{errors::Handler, sync::Lrc, FileName, SourceMap};
use swc_ecma_ast::EsVersion;
use swc_ecma_parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax, TsSyntax};
use thiserror::Error;

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
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            dialect: InputDialect::JavaScript,
            is_module: true,
            filename: "unknown.js".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseMetadata {
    pub statement_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileOutput {
    pub code: String,
    pub metadata: ParseMetadata,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompilerError {
    #[error("Flow syntax is not supported by the Rust frontend yet")]
    UnsupportedFlowSyntax,
    #[error("Failed to parse input: {message}")]
    ParseFailure { message: String },
}

pub fn compile(source: &str, options: &CompilerOptions) -> Result<CompileOutput, CompilerError> {
    if options.dialect == InputDialect::Flow {
        return Err(CompilerError::UnsupportedFlowSyntax);
    }

    let cm: Lrc<SourceMap> = Default::default();
    let handler = Handler::with_emitter_writer(Box::new(std::io::stderr()), Some(cm.clone()));
    let fm = cm.new_source_file(
        FileName::Custom(options.filename.clone()).into(),
        source.to_string(),
    );

    let syntax = match options.dialect {
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
        InputDialect::Flow => unreachable!(),
    };

    let lexer = Lexer::new(syntax, EsVersion::EsNext, StringInput::from(&*fm), None);

    let mut parser = Parser::new_from(lexer);
    let statement_count = if options.is_module {
        parser
            .parse_module()
            .map(|module| module.body.len())
            .map_err(|err| {
                let message = err.kind().msg().to_string();
                err.into_diagnostic(&handler).emit();
                CompilerError::ParseFailure { message }
            })?
    } else {
        parser
            .parse_script()
            .map(|script| script.body.len())
            .map_err(|err| {
                let message = err.kind().msg().to_string();
                err.into_diagnostic(&handler).emit();
                CompilerError::ParseFailure { message }
            })?
    };

    Ok(CompileOutput {
        code: source.to_string(),
        metadata: ParseMetadata { statement_count },
    })
}

#[cfg(test)]
mod tests {
    use super::{compile, CompilerError, CompilerOptions, InputDialect};

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

        assert_eq!(err, CompilerError::UnsupportedFlowSyntax);
    }
}
