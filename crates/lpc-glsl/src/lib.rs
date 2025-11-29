//! GLSL frontend compiler for LPIR.
//!
//! This crate provides a GLSL frontend that parses GLSL source code,
//! performs type checking, and generates LPIR (Low-level Program
//! Intermediate Representation).

#![no_std]

extern crate alloc;

mod codegen;
mod error;
mod parser;
mod symbols;
mod typecheck;
mod types;

pub use codegen::CodeGen;
pub use error::{GlslError, GlslResult};
pub use parser::{parse_glsl, FunctionInfo};
pub use symbols::{FunctionSignature, Parameter, ParameterQualifier, SymbolTable, Variable};
pub use typecheck::TypeChecker;
pub use types::GlslType;
