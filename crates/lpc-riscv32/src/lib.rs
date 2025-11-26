//! RISC-V 32-bit instruction encoder.
//!
//! This crate provides functions to encode RISC-V 32-bit instructions
//! into their binary representation.

#![no_std]

extern crate alloc;

pub mod asm_parser;
pub mod decode;
pub mod disasm;
pub mod encode;
pub mod inst;
pub mod regs;
pub mod backend;
pub mod emu;

pub use asm_parser::{assemble_code, assemble_instruction};
pub use decode::{decode_instruction, DecodedFields};
pub use disasm::{disassemble_code, disassemble_code_with_labels, disassemble_instruction};
pub use encode::*;
pub use inst::Inst;
pub use regs::Gpr;

// Re-export backend items for convenience
pub use backend::{
    compile_module, compile_module_to_insts, debug_elf, debug_ir, debug_ir_with_ram, expect_ir_a0,
    expect_ir_error, expect_ir_error_with_ram, expect_ir_memory_error,
    expect_ir_memory_error_with_ram, expect_ir_ok, expect_ir_register, expect_ir_syscall,
    expect_ir_unaligned_error, generate_elf, CompiledModule,
};

// Re-export emu items for convenience
pub use emu::{EmulatorError, LogLevel, MemoryAccessKind, Riscv32Emulator, StepResult, SyscallInfo};

