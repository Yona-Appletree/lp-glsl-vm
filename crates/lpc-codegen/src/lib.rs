//! RISC-V 32-bit instruction encoder.
//!
//! This crate provides functions to encode RISC-V 32-bit instructions
//! into their binary representation.

#![no_std]

extern crate alloc;

mod backend3;
pub mod debug;
mod elf;
pub mod emu;
mod isa;

// Re-export modules from isa::riscv32
// Re-export emu items for convenience
pub use emu::{
    EmulatorError, LogLevel, MemoryAccessKind, Riscv32Emulator, StepResult, SyscallInfo,
};
pub use isa::riscv32::{
    asm_parser::{assemble_code, assemble_instruction},
    decode::{decode_instruction, DecodedFields},
    disasm::{disassemble_code, disassemble_code_with_labels, disassemble_instruction},
    encode::*,
    inst::Inst,
    regs::Gpr,
};

// Old backend removed - see isa/riscv32/backend_old/ for reference
// pub use backend::{compile_module_to_insts, Abi, CompiledModule, FrameLayout};
// pub use backend::{expect_ir_a0, expect_ir_ok, expect_ir_syscall};
