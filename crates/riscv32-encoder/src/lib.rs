//! RISC-V 32-bit instruction encoder.
//!
//! This crate provides functions to encode RISC-V 32-bit instructions
//! into their binary representation.

#![no_std]

extern crate alloc;

mod asm;
mod disasm;
mod encode;
mod inst;
mod regs;

pub use asm::{assemble_code, assemble_instruction};
pub use disasm::{disassemble_code, disassemble_code_with_labels, disassemble_instruction};
pub use encode::*;
pub use inst::Inst;
pub use regs::Gpr;
