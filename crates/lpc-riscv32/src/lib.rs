//! RISC-V 32-bit instruction encoder.
//!
//! This crate provides functions to encode RISC-V 32-bit instructions
//! into their binary representation.

#![no_std]

extern crate alloc;

pub mod asm_parser;
pub mod backend;
pub mod debug;
pub mod decode;
pub mod disasm;
pub mod emu;
pub mod encode;
pub mod inst;
pub mod regs;
mod register_role;
mod elf;
mod inst_buffer;
mod backend3;

pub use asm_parser::{assemble_code, assemble_instruction};
// Re-export backend items for convenience
pub use backend::{FrameLayout, Abi, compile_module_to_insts, CompiledModule};

// Re-export test helpers (always available for tests)
pub use backend::{expect_ir_a0, expect_ir_ok, expect_ir_syscall};
pub use decode::{decode_instruction, DecodedFields};
pub use disasm::{disassemble_code, disassemble_code_with_labels, disassemble_instruction};
// Re-export emu items for convenience
pub use emu::{
    EmulatorError, LogLevel, MemoryAccessKind, Riscv32Emulator, StepResult, SyscallInfo,
};
pub use encode::*;
pub use inst::Inst;
pub use regs::Gpr;
