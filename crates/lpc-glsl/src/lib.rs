//! GLSL frontend compiler for LPIR.
//!
//! This crate provides a GLSL frontend that parses GLSL source code,
//! performs type checking, and generates LPIR (Low-level Program
//! Intermediate Representation).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

/// Debug macro that prints when std feature is enabled.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        #[cfg(feature = "std")]
        {
            eprintln!($($arg)*);
        }
        #[cfg(not(feature = "std"))]
        {
            // No-op when std is not available
            let _ = core::format_args!($($arg)*);
        }
    };
}

mod codegen;
mod control;
mod decl;
mod error;
mod expr;
mod function;
mod parser;
mod stmt;
mod symbols;
mod types;
mod util;

pub use error::{GlslError, GlslResult};
pub use function::{
    codegen::CodeGen,
    typecheck::{extract_function_signature, TypeChecker},
};
pub use parser::{parse_glsl, FunctionInfo};
pub use symbols::{FunctionSignature, Parameter, ParameterQualifier, SymbolTable, Variable};
pub use types::GlslType;
