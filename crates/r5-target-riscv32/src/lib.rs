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
    // Compile entry function first, then others
    let mut function_code = BTreeMap::new();
    let mut function_addresses = BTreeMap::new();
    let mut function_relocations = BTreeMap::new();
    let mut current_address = 0u32;

    // Compile entry function first (if set)
    if let Some(entry_name) = &module.entry_function {
        if let Some(func) = module.functions.get(entry_name) {
            lowerer.set_function_start(current_address as usize);
            lowerer.clear_relocations();
            let code = lowerer.lower_function(func);
            let code_bytes = code.as_bytes().to_vec();
            let code_size = code_bytes.len() as u32;
            let relocations = lowerer.relocations().to_vec();

            function_code.insert(entry_name.clone(), code_bytes);
            function_addresses.insert(entry_name.clone(), current_address);
            function_relocations.insert(entry_name.clone(), relocations);
            lowerer.set_function_address(entry_name.clone(), current_address);

            current_address += (code_size + 3) & !3;
        }
    }

    // Compile remaining functions
    for (name, func) in &module.functions {
        // Skip entry function (already compiled)
        if module.entry_function.as_ref().map(|e| e == name).unwrap_or(false) {
            continue;
        }
        lowerer.set_function_start(current_address as usize);
        lowerer.clear_relocations();
        let code = lowerer.lower_function(func);
        let code_bytes = code.as_bytes().to_vec();
        let code_size = code_bytes.len() as u32;
        let relocations = lowerer.relocations().to_vec();

        function_code.insert(name.clone(), code_bytes);
        function_addresses.insert(name.clone(), current_address);
        function_relocations.insert(name.clone(), relocations);
        lowerer.set_function_address(name.clone(), current_address);

        // Align to 4-byte boundary
        current_address += (code_size + 3) & !3;
    }

    // Second pass: concatenate code and fix up relocations
    let mut result = Vec::new();
    let mut current_offset = 0u32;

    for (name, code_bytes) in &function_code {
        // Get relocations for this function
        if let Some(relocations) = function_relocations.get(name) {
            // Copy code and fix up relocations
            let mut code_with_fixups = code_bytes.clone();
            
            for reloc in relocations {
                // Calculate target address
                let target_addr = function_addresses
                    .get(&reloc.callee)
                    .copied()
                    .unwrap_or_else(|| {
                        panic!("Function '{}' not found in module", reloc.callee);
                    });

                // Calculate PC-relative offset
                // jal is PC-relative: target = PC + offset
                // When jal executes, PC points to the jal instruction
                // offset = target - PC = target - (current_offset + reloc.offset)
                let jal_pc = current_offset + reloc.offset as u32;
                let offset = target_addr as i32 - jal_pc as i32;

                // Update the jal instruction
                let jal_inst = riscv32_encoder::jal(riscv32_encoder::Gpr::RA, offset);
                let jal_bytes = jal_inst.to_le_bytes();
                let inst_offset = reloc.offset;
                code_with_fixups[inst_offset..inst_offset + 4].copy_from_slice(&jal_bytes);
            }

            result.extend_from_slice(&code_with_fixups);
        } else {
            result.extend_from_slice(code_bytes);
        }

        // Align to 4-byte boundary
        while result.len() % 4 != 0 {
            result.push(0);
        }

        current_offset = result.len() as u32;
    }

    result
}
