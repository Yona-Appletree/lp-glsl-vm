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

pub use emulator::{Riscv32Emulator, StepResult, SyscallInfo};
pub use error::{EmulatorError, MemoryAccessKind};
pub use logging::LogLevel;

