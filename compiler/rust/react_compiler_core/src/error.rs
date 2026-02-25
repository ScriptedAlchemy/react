use thiserror::Error;

use crate::SourceLocation;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompilerError {
    #[error("Flow syntax is not supported by the Rust frontend yet")]
    UnsupportedFlowSyntax { location: Option<SourceLocation> },
    #[error("Failed to parse input: {message}")]
    ParseFailure {
        message: String,
        reason: &'static str,
        location: Option<SourceLocation>,
    },
    #[error("Failed to emit compiled output: {message}")]
    CodegenFailure { message: String },
}

impl CompilerError {
    pub fn code(&self) -> &'static str {
        match self {
            CompilerError::UnsupportedFlowSyntax { .. } => "unsupported_flow_syntax",
            CompilerError::ParseFailure { .. } => "parse_failure",
            CompilerError::CodegenFailure { .. } => "codegen_failure",
        }
    }

    pub fn category(&self) -> &'static str {
        match self {
            CompilerError::UnsupportedFlowSyntax { .. } => "syntax",
            CompilerError::ParseFailure { .. } => "syntax",
            CompilerError::CodegenFailure { .. } => "internal",
        }
    }

    pub fn reason(&self) -> &'static str {
        match self {
            CompilerError::UnsupportedFlowSyntax { .. } => "flow_syntax_not_supported",
            CompilerError::ParseFailure { reason, .. } => reason,
            CompilerError::CodegenFailure { .. } => "codegen_error",
        }
    }

    pub fn severity(&self) -> &'static str {
        "error"
    }

    pub fn location(&self) -> Option<&SourceLocation> {
        match self {
            CompilerError::UnsupportedFlowSyntax { location } => location.as_ref(),
            CompilerError::ParseFailure { location, .. } => location.as_ref(),
            _ => None,
        }
    }
}
