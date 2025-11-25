//! RISC-V 32-bit target implementation.
//!
//! This crate provides:
//! - Instruction lowering (IR → RISC-V)
//! - Register allocation
//! - Code emission
//! - ELF generation

#![no_std]

extern crate alloc;

mod elf;
mod emit;
mod lower;
mod regalloc;

pub use elf::generate_elf;
pub use emit::CodeBuffer;
pub use lower::Lowerer;

/// Compile an IR function to RISC-V 32-bit code.
pub fn compile_function(func: &r5_ir::Function) -> alloc::vec::Vec<u8> {
    let mut lowerer = Lowerer::new();
    let code = lowerer.lower_function(func);
    code.as_bytes().to_vec()
}
