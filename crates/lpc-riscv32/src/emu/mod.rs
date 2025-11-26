mod decoder;
pub mod emulator;
pub mod error;
mod executor;
pub mod logging;
mod memory;
pub mod test_helpers;

pub use emulator::{Riscv32Emulator, StepResult, SyscallInfo};
pub use error::{EmulatorError, MemoryAccessKind};
pub use logging::{InstLog, LogLevel};
pub use test_helpers::{
    debug_riscv32_asm, debug_riscv32_asm_with_ram, debug_riscv32_bytes, debug_riscv32_ops,
    expect_a0, expect_error, expect_error_with_ram, expect_memory_error,
    expect_memory_error_with_ram, expect_ok, expect_register, expect_unaligned_error,
};
