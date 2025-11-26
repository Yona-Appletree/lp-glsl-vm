//! RISC-V 32-bit instruction encoder.
//!
//! This crate provides functions to encode RISC-V 32-bit instructions
//! into their binary representation.

#![no_std]

extern crate alloc;

mod disasm;
mod encode;
mod inst;
mod regs;

pub use disasm::{disassemble_code, disassemble_instruction};
pub use encode::*;
pub use inst::Inst;
pub use regs::Gpr;
