//! RISC-V 32-bit target implementation.
//!
//! This crate provides:
//! - Instruction lowering (IR → RISC-V)
//! - Register allocation
//! - Code emission
//! - ELF generation

#![no_std]

extern crate alloc;

mod elf;
mod emit;
mod lower;
mod regalloc;

pub use elf::{debug_elf, generate_elf};
pub use emit::CodeBuffer;
pub use lower::Lowerer;

/// Compile an IR function to RISC-V 32-bit code.
///
/// # Deprecated
///
/// This function is deprecated. Use `compile_module` instead.
#[deprecated(note = "Use compile_module instead")]
pub fn compile_function(func: &r5_ir::Function) -> alloc::vec::Vec<u8> {
    let mut lowerer = Lowerer::new();
    let code = lowerer.lower_function(func);
    code.as_bytes().to_vec()
}

/// Compile an IR module to RISC-V 32-bit code.
///
/// This compiles all functions in the module and handles function call relocations.
/// Returns the compiled code with all functions concatenated.
pub fn compile_module(module: &r5_ir::Module) -> alloc::vec::Vec<u8> {
    use alloc::{collections::BTreeMap, vec::Vec};

    let mut lowerer = Lowerer::new();
    lowerer.set_module(module.clone());

    // First pass: compile all functions and record their addresses
    let mut function_code = BTreeMap::new();
    let mut function_addresses = BTreeMap::new();
    let mut current_address = 0u32;

    for (name, func) in &module.functions {
        let code = lowerer.lower_function(func);
        let code_bytes = code.as_bytes().to_vec();
        let code_size = code_bytes.len() as u32;

        function_code.insert(name.clone(), code_bytes);
        function_addresses.insert(name.clone(), current_address);
        lowerer.set_function_address(name.clone(), current_address);

        // Align to 4-byte boundary
        current_address += (code_size + 3) & !3;
    }

    // Second pass: fix up function call relocations
    // For now, we'll just concatenate the code
    // TODO: Implement proper relocation handling
    let mut result = Vec::new();
    for (_name, code_bytes) in &function_code {
        result.extend_from_slice(code_bytes);
        // Align to 4-byte boundary
        while result.len() % 4 != 0 {
            result.push(0);
        }
    }

    result
}
