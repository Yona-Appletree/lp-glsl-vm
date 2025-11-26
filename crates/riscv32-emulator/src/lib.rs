//! RISC-V 32-bit emulator for testing and debugging.
//!
//! This crate provides a simple, debuggable RISC-V 32-bit emulator
//! designed for testing and debugging encoding issues.

#![no_std]

extern crate alloc;

mod decoder;
mod emulator;
mod error;
mod executor;
mod logging;
mod memory;
mod helpers;

pub use emulator::{Riscv32Emulator, StepResult, SyscallInfo};
pub use error::{EmulatorError, MemoryAccessKind};
pub use logging::LogLevel;
pub use helpers::{
    debug_riscv32_asm, debug_riscv32_asm_with_ram, debug_riscv32_bytes, debug_riscv32_ops,
    expect_a0, expect_error, expect_error_with_ram, expect_memory_error,
    expect_memory_error_with_ram, expect_ok, expect_register, expect_unaligned_error,
};

