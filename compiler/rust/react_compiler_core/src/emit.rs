use swc_common::{comments::SingleThreadedComments, sync::Lrc, SourceMap};
use swc_ecma_ast::{Module, Script};
use swc_ecma_codegen::{text_writer::JsWriter, Config as CodegenConfig, Emitter};

use crate::CompilerError;

pub(crate) fn emit_module(
    cm: &Lrc<SourceMap>,
    comments: &SingleThreadedComments,
    module: &Module,
) -> Result<String, CompilerError> {
    let mut output = vec![];
    {
        let writer = JsWriter::new(cm.clone(), "\n", &mut output, None);
        let mut emitter = Emitter {
            cfg: CodegenConfig::default(),
            comments: Some(comments),
            cm: cm.clone(),
            wr: writer,
        };
        emitter
            .emit_module(module)
            .map_err(|err| CompilerError::CodegenFailure {
                message: err.to_string(),
            })?;
    }
    String::from_utf8(output).map_err(|err| CompilerError::CodegenFailure {
        message: err.to_string(),
    })
}

pub(crate) fn emit_script(
    cm: &Lrc<SourceMap>,
    comments: &SingleThreadedComments,
    script: &Script,
) -> Result<String, CompilerError> {
    let mut output = vec![];
    {
        let writer = JsWriter::new(cm.clone(), "\n", &mut output, None);
        let mut emitter = Emitter {
            cfg: CodegenConfig::default(),
            comments: Some(comments),
            cm: cm.clone(),
            wr: writer,
        };
        emitter
            .emit_script(script)
            .map_err(|err| CompilerError::CodegenFailure {
                message: err.to_string(),
            })?;
    }
    String::from_utf8(output).map_err(|err| CompilerError::CodegenFailure {
        message: err.to_string(),
    })
}
