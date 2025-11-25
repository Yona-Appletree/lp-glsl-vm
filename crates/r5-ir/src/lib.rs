//! RISC-V 5 Intermediate Representation (IR).
//!
//! This crate defines the core IR types for the compiler:
//! - Types (i32, i64, f32, f64, etc.)
//! - Values (SSA value identifiers)
//! - Instructions (iadd, isub, iconst, return, etc.)
//! - Blocks (basic blocks)
//! - Functions (functions with blocks and signature)
//! - Signatures (function signatures)

#![no_std]

extern crate alloc;

mod block;
mod function;
mod inst;
mod signature;
mod types;
mod value;

pub use block::Block;
pub use function::Function;
pub use inst::Inst;
pub use signature::Signature;
pub use types::Type;
pub use value::Value;
