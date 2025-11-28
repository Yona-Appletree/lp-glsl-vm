//! RISC-V 5 Intermediate Representation (IR).
//!
//! This crate defines the core IR types for the compiler:
//! - Types (i32, u32, f32, etc.)
//! - Values (SSA value identifiers)
//! - Instructions (iadd, isub, iconst, return, etc.)
//! - Blocks (basic blocks)
//! - Functions (functions with blocks and signature)
//! - Signatures (function signatures)

#![no_std]

extern crate alloc;

mod block;
mod builder;
mod function;
mod inst;
mod module;
mod parser;
mod signature;
mod types;
mod value;

pub use block::Block;
pub use builder::function_builder::FunctionBuilder;
pub use function::Function;
pub use inst::Inst;
pub use module::Module;
pub use parser::{parse_function, parse_module, ParseError};
pub use signature::Signature;
pub use types::Type;
pub use value::Value;
