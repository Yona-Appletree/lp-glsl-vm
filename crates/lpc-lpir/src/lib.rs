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

mod analysis;
mod block;
mod builder;
mod condcodes;
mod dfg;
mod entity;
mod entity_map;
mod function;
mod layout;
mod module;
mod parser;
mod signature;
mod sourceloc;
mod trapcode;
mod types;
mod value;
mod verifier;

pub use analysis::{ControlFlowGraph, DominatorTree};
pub use block::BlockData;
pub use builder::{
    function_builder::FunctionBuilder,
    traits::{InstBuilder, InstBuilderBase, InstInserterBase},
    InsertBuilder, ReplaceBuilder,
};
pub use condcodes::{CondCode, FloatCC, IntCC};
pub use dfg::{BlockArgs, Immediate, InstData, Opcode, DFG};
pub use entity::{Block as BlockEntity, EntityRef, Inst as InstEntity};
pub use entity_map::PrimaryMap;
pub use function::Function;
pub use layout::Layout;
pub use module::Module;
pub use parser::{parse_function, parse_module, ParseError};
pub use signature::Signature;
pub use sourceloc::{RelSourceLoc, SourceLoc};
pub use trapcode::TrapCode;
pub use types::Type;
pub use value::Value;
pub use verifier::{verify, VerifierError};
