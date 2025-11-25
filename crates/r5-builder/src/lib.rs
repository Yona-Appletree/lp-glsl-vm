//! IR builder for constructing RISC-V 5 IR.
//!
//! This crate provides builders for constructing IR in SSA form:
//! - `FunctionBuilder`: Build functions with blocks and instructions
//! - `BlockBuilder`: Build instructions within a block
//! - SSA construction helpers for managing variables

#![no_std]

extern crate alloc;

mod block_builder;
mod function_builder;
mod ssa;

pub use block_builder::BlockBuilder;
pub use function_builder::FunctionBuilder;
pub use ssa::SSABuilder;
