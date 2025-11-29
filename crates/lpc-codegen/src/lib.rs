//! RISC-V 32-bit instruction encoder.
//!
//! This crate provides functions to encode RISC-V 32-bit instructions
//! into their binary representation.

#![no_std]

extern crate alloc;

pub mod backend3;
pub mod debug;
mod elf;
pub mod emu;
pub mod isa;

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

use alloc::{format, string::String, vec::Vec};
use lpc_lpir::Module;

use crate::{
    backend3::{
        lower::lower_function,
        symbols::SymbolTable,
        vcode::Callee,
    },
    isa::riscv32::{
        backend3::{inst::Riscv32ABI, Riscv32LowerBackend},
        inst_buffer::InstBuffer,
    },
};

/// Result of compiling a module to RISC-V instructions.
///
/// This structure contains the compiled code and metadata about the compilation,
/// including the bootstrap function size (for entry function).
pub struct CompiledModule {
    /// Compiled RISC-V code buffer
    buffer: InstBuffer,
    /// Size of the bootstrap/entry function in instructions
    /// This is the number of instructions in the entry function, used to skip
    /// bootstrap code when calling functions directly.
    bootstrap_size: usize,
}

impl CompiledModule {
    /// Convert the compiled module to bytes.
    ///
    /// Returns the raw RISC-V machine code as a byte vector.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.buffer.as_bytes())
    }

    /// Get the bootstrap function size in instructions.
    pub fn bootstrap_size(&self) -> usize {
        self.bootstrap_size
    }

    /// Get the instruction buffer (for internal use).
    pub fn buffer(&self) -> &InstBuffer {
        &self.buffer
    }
}

/// Compile a module to RISC-V instructions.
///
/// This function compiles all functions in the module sequentially, building
/// a symbol table as it goes to resolve cross-function calls. The entry function
/// (if specified) is compiled first.
///
/// # Arguments
///
/// * `module` - The LPIR module to compile
///
/// # Returns
///
/// A `CompiledModule` containing the compiled code and metadata.
///
/// # Errors
///
/// Returns an error string if compilation fails (e.g., register allocation fails,
/// function not found, etc.).
pub fn compile_module_to_insts(module: &Module) -> Result<CompiledModule, String> {
    use crate::backend3::symbols::Symbol;

    let mut symbol_table = SymbolTable::new();
    let mut combined_buffer = InstBuffer::new();
    let mut bootstrap_size = 0;

    // Determine compilation order: entry function first (if exists), then others
    let mut compilation_order = Vec::new();
    
    // Add entry function first if it exists
    if let Some(entry_name) = &module.entry_function {
        if module.functions.contains_key(entry_name) {
            compilation_order.push(entry_name.clone());
        }
    }

    // Add all other functions
    for func_name in module.functions.keys() {
        if !compilation_order.contains(func_name) {
            compilation_order.push(func_name.clone());
        }
    }

    // Compile each function in order
    for func_name in &compilation_order {
        let func = module
            .get_function(func_name)
            .ok_or_else(|| format!("Function '{}' not found in module", func_name))?;

        // Lower function to VCode
        let backend = Riscv32LowerBackend;
        let abi = Callee { abi: Riscv32ABI };
        let vcode = lower_function(func.clone(), &backend, abi);

        // Run register allocation
        let regalloc = vcode
            .run_regalloc()
            .map_err(|e| format!("Register allocation failed for '{}': {:?}", func_name, e))?;

        // Track the current buffer size before emitting this function
        let function_start_offset = combined_buffer.instruction_count();

        // Emit code with symbol table
        let function_buffer = vcode.emit(&regalloc, Some(&mut symbol_table), Some(func_name));

        // Register this function in the symbol table with its code offset
        let code_offset = combined_buffer.instruction_count() as u32;
        symbol_table.add_local(Symbol::local(func_name.clone()), code_offset);

        // Append function code to combined buffer
        for inst in function_buffer.instructions() {
            combined_buffer.emit(inst.clone());
        }

        // Track bootstrap size if this is the entry function
        if Some(func_name) == module.entry_function.as_ref() {
            bootstrap_size = combined_buffer.instruction_count() - function_start_offset;
        }
    }

    Ok(CompiledModule {
        buffer: combined_buffer,
        bootstrap_size,
    })
}

/// Compile a module to RISC-V instructions (legacy function name).
///
/// This is an alias for `compile_module_to_insts` for backward compatibility.
/// Note: This returns the buffer directly, losing bootstrap_size information.
pub fn compile_module(module: &Module) -> Result<InstBuffer, String> {
    let _compiled = compile_module_to_insts(module)?;
    // Extract the buffer by taking ownership
    // Since CompiledModule owns the buffer, we need to restructure
    // For now, we'll compile again but only return the buffer
    // TODO: Refactor to avoid double compilation
    let mut symbol_table = SymbolTable::new();
    let mut buffer = InstBuffer::new();

    // Determine compilation order
    let mut compilation_order = Vec::new();
    if let Some(entry_name) = &module.entry_function {
        if module.functions.contains_key(entry_name) {
            compilation_order.push(entry_name.clone());
        }
    }
    for func_name in module.functions.keys() {
        if !compilation_order.contains(func_name) {
            compilation_order.push(func_name.clone());
        }
    }

    // Compile each function
    for func_name in &compilation_order {
        let func = module
            .get_function(func_name)
            .ok_or_else(|| format!("Function '{}' not found in module", func_name))?;

        let backend = Riscv32LowerBackend;
        let abi = Callee { abi: Riscv32ABI };
        let vcode = lower_function(func.clone(), &backend, abi);
        let regalloc = vcode
            .run_regalloc()
            .map_err(|e| format!("Register allocation failed for '{}': {:?}", func_name, e))?;

        use crate::backend3::symbols::Symbol;
        let code_offset = buffer.instruction_count() as u32;
        symbol_table.add_local(Symbol::local(func_name.clone()), code_offset);

        let function_buffer = vcode.emit(&regalloc, Some(&mut symbol_table), Some(func_name));
        for inst in function_buffer.instructions() {
            buffer.emit(inst.clone());
        }
    }

    Ok(buffer)
}
