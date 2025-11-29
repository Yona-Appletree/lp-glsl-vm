//! GLSL frontend compiler for LPIR.
//!
//! This crate provides a GLSL frontend that parses GLSL source code,
//! performs type checking, and generates LPIR (Low-level Program
//! Intermediate Representation).

#![no_std]

extern crate alloc;

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
