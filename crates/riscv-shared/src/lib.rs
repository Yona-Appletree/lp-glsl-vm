//! Shared code for RISC-V JIT testing.
//!
//! This crate provides common functionality for building and compiling
//! JIT code that can be used both in the embive VM and on real hardware.

#![no_std]

extern crate alloc;

use alloc::vec;

use r5_builder::FunctionBuilder;
use r5_ir::{Signature, Type};
use r5_target_riscv32::{compile_function, generate_elf};

/// Result of JIT compilation
pub struct JitResult {
    /// Compiled RISC-V code (raw bytes)
    pub code: alloc::vec::Vec<u8>,
    /// ELF file data
    pub elf: alloc::vec::Vec<u8>,
}

/// Build and compile a simple multiplication function: fn mul(a: i32, b: i32) -> i32 { a * b }
pub fn build_and_compile_mul() -> JitResult {
    // Build IR: fn mul(a: i32, b: i32) -> i32 { a * b }
    let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
    let mut builder = FunctionBuilder::new(sig);
    let block_idx = builder.create_block();

    // Create values for parameters and result
    let a = builder.new_value();
    let b = builder.new_value();
    let result = builder.new_value();

    {
        let mut block_builder = builder.block_builder(block_idx);
        block_builder.imul(result, a, b);
        block_builder.return_(&vec![result]);
    }

    let func = builder.finish();

    // Compile IR to RISC-V code
    let riscv_code = compile_function(&func);

    // Generate ELF file
    let elf_data = generate_elf(&riscv_code);

    JitResult {
        code: riscv_code,
        elf: elf_data,
    }
}
