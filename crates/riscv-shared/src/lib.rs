//! Shared code for RISC-V JIT testing.
//!
//! This crate provides common functionality for building and compiling
//! JIT code that can be used both in the embive VM and on real hardware.

#![no_std]

extern crate alloc;

use alloc::string::String;

use r5_ir::parse_module;
use r5_target_riscv32::{compile_module, generate_elf};

/// Result of JIT compilation
pub struct JitResult {
    /// Compiled RISC-V code (raw bytes)
    pub code: alloc::vec::Vec<u8>,
    /// ELF file data
    pub elf: alloc::vec::Vec<u8>,
}

/// Compile an SSA string to an ELF file.
///
/// This is a utility function similar to the test helpers that takes an SSA
/// string (IR module format) and produces an ELF file.
pub fn compile_ssa_to_elf(ssa: &str) -> Result<JitResult, String> {
    // Parse the SSA string into a module
    let module = parse_module(ssa).map_err(|e| alloc::format!("Parse error: {}", e))?;

    // Compile IR to RISC-V code
    let riscv_code =
        compile_module(&module).map_err(|e| alloc::format!("Compilation failed: {}", e))?;

    // Generate ELF file
    let elf_data = generate_elf(&riscv_code);

    Ok(JitResult {
        code: riscv_code,
        elf: elf_data,
    })
}

/// Build and compile a simple multiplication function: fn mul(a: i32, b: i32) -> i32 { a * b }
/// Uses SSA string format.
pub fn build_and_compile_mul() -> JitResult {
    let ssa = r#"
module {
entry: %mul

function %mul(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = imul v0, v1
    return v2
}
}"#;

    compile_ssa_to_elf(ssa).expect("Failed to compile mul function")
}
const FIB_SSA: &str = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 10
    call %fib(v0) -> v1
    halt
}

function %fib(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = icmp_le v0, v1
    brif v2, block1, block2

block1:
    return v0

block2:
    v3 = iconst 2
    v4 = isub v0, v1
    v5 = isub v0, v3
    call %fib(v4) -> v6
    call %fib(v5) -> v7
    v8 = iadd v6, v7
    return v8
}
}"#;

/// Build and compile a recursive fibonacci function: fn fib(n: i32) -> i32
/// Uses SSA string format with recursive calls.
/// Includes a bootstrap function that calls fib(10) and returns the result.
pub fn build_and_compile_fib() -> JitResult {
    compile_ssa_to_elf(FIB_SSA).expect("Failed to compile fib function")
}

#[cfg(test)]
mod tests {
    use r5_target_riscv32::expect_ir_a0;

    use super::*;

    #[test]
    fn test_fibonacci() {
        // Test fib(10) = 55
        expect_ir_a0(FIB_SSA, 55);
    }
}
