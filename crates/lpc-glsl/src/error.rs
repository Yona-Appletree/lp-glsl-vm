//! Error types for GLSL compilation.

use alloc::string::String;

/// Result type for GLSL compilation operations.
pub type GlslResult<T> = Result<T, GlslError>;

/// Error that can occur during GLSL compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlslError {
    /// Parsing error from glsl-parser
    ParseError(String),
    /// Type checking error
    TypeError(String),
    /// Code generation error
    CodeGenError(String),
    /// Void function call used as expression (not allowed unless in expression statement)
    VoidFunctionCall(String),
    /// Other error
    Other(String),
}

impl GlslError {
    /// Create a new parse error.
    pub fn parse(msg: impl Into<String>) -> Self {
        GlslError::ParseError(msg.into())
    }

    /// Create a new type error.
    pub fn type_error(msg: impl Into<String>) -> Self {
        GlslError::TypeError(msg.into())
    }

    /// Create a new code generation error.
    pub fn codegen(msg: impl Into<String>) -> Self {
        GlslError::CodeGenError(msg.into())
    }

    /// Create a new other error.
    pub fn other(msg: impl Into<String>) -> Self {
        GlslError::Other(msg.into())
    }

    /// Create a void function call error.
    pub fn void_function_call(function_name: impl Into<String>) -> Self {
        GlslError::VoidFunctionCall(function_name.into())
    }
}

impl core::fmt::Display for GlslError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            GlslError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            GlslError::TypeError(msg) => write!(f, "Type error: {}", msg),
            GlslError::CodeGenError(msg) => write!(f, "Code generation error: {}", msg),
            GlslError::VoidFunctionCall(name) => {
                write!(
                    f,
                    "Function '{}' returns void and cannot be used as an expression",
                    name
                )
            }
            GlslError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}
