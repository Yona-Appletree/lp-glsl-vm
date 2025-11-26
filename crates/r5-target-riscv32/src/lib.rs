//! RISC-V 32-bit target implementation.
//!
//! This crate provides:
//! - Instruction lowering (IR → RISC-V)
//! - Register allocation
//! - Code emission
//! - ELF generation

#![no_std]

extern crate alloc;

mod abi;
mod elf;
mod emit;
mod frame;
mod liveness;
mod lower;
mod regalloc;
mod spill_reload;

pub use abi::{Abi, AbiInfo};
pub use elf::{debug_elf, generate_elf};
pub use emit::CodeBuffer;
pub use frame::FrameLayout;
pub use liveness::{compute_liveness, LivenessInfo};
pub use lower::Lowerer;
pub use regalloc::{allocate_registers, is_callee_saved, is_caller_saved, RegisterAllocation};
pub use spill_reload::{create_spill_reload_plan, SpillReloadPlan};

/// Compile an IR function to RISC-V 32-bit code.
///
/// # Deprecated
///
/// This function is deprecated. Use `compile_module` instead.
#[deprecated(note = "Use compile_module instead")]
pub fn compile_function(func: &r5_ir::Function) -> alloc::vec::Vec<u8> {
    // Compute liveness, allocation, spill/reload, frame layout, and ABI info
    let liveness = compute_liveness(func);
    let allocation = allocate_registers(func, &liveness);
    let spill_reload = create_spill_reload_plan(func, &allocation, &liveness);

    // Check if function has calls
    let has_calls = func.blocks.iter().any(|block| {
        block
            .insts
            .iter()
            .any(|inst| matches!(inst, r5_ir::Inst::Call { .. }))
    });

    // For deprecated function, use default max outgoing args
    let max_outgoing_args = 8;
    let frame_layout = FrameLayout::compute(
        &allocation.used_callee_saved,
        allocation.spill_slot_count,
        has_calls,
        func.signature.params.len(),
        max_outgoing_args,
    );

    let abi_info = Abi::compute_abi_info(func, &allocation, max_outgoing_args);

    let mut lowerer = Lowerer::new();
    let code = lowerer
        .lower_function(func, &allocation, &spill_reload, &frame_layout, &abi_info)
        .unwrap_or_else(|e| panic!("Failed to lower function: {}", e));
    code.as_bytes().to_vec()
}

/// Align a size to a 4-byte boundary.
fn align_to_4_bytes(size: usize) -> usize {
    (size + 3) & !3
}

/// Compute the maximum number of outgoing arguments for a function.
///
/// This analyzes all call sites in the function to determine the maximum
/// number of arguments passed to any callee.
fn compute_max_outgoing_args(func: &r5_ir::Function, module: &r5_ir::Module) -> usize {
    let mut max_args = 0;
    for block in &func.blocks {
        for inst in &block.insts {
            if let r5_ir::Inst::Call { callee, args, .. } = inst {
                // Look up callee signature in module
                if let Some(callee_func) = module.functions.get(callee) {
                    max_args = max_args.max(callee_func.signature.params.len());
                }
                // Also check direct args count (in case callee not in module)
                max_args = max_args.max(args.len());
            }
        }
    }
    max_args
}

/// Fix up relocations in compiled code.
///
/// This updates jal instructions with correct PC-relative offsets to their target functions.
fn fixup_relocations(
    code: &mut [u8],
    relocations: &[lower::Relocation],
    function_addresses: &alloc::collections::BTreeMap<alloc::string::String, u32>,
    current_offset: u32,
) -> Result<(), alloc::string::String> {
    for reloc in relocations {
        // Validate offset is within bounds
        if reloc.offset + 4 > code.len() {
            return Err(alloc::format!(
                "Relocation offset {} is out of bounds (code size: {})",
                reloc.offset,
                code.len()
            ));
        }

        // Calculate target address
        let target_addr = function_addresses
            .get(&reloc.callee)
            .ok_or_else(|| alloc::format!("Function '{}' not found in module", reloc.callee))?;

        // Calculate PC-relative offset
        // jal is PC-relative: target = PC + offset
        // When jal executes, PC points to the jal instruction
        // offset = target - PC = target - (current_offset + reloc.offset)
        let jal_pc = current_offset
            .checked_add(reloc.offset as u32)
            .ok_or_else(|| alloc::string::String::from("Relocation offset overflow"))?;
        let offset = (*target_addr as i32)
            .checked_sub(jal_pc as i32)
            .ok_or_else(|| {
                alloc::string::String::from("Relocation offset calculation underflow")
            })?;

        // Update the jal instruction
        let jal_inst = riscv32_encoder::jal(riscv32_encoder::Gpr::RA, offset);
        let jal_bytes = jal_inst.to_le_bytes();
        let inst_offset = reloc.offset;
        code[inst_offset..inst_offset + 4].copy_from_slice(&jal_bytes);
    }
    Ok(())
}

/// Compile an IR module to RISC-V 32-bit code.
///
/// This compiles all functions in the module and handles function call relocations.
/// Returns the compiled code with all functions concatenated.
///
/// # Two-Pass Compilation
///
/// The compilation uses a two-pass approach:
/// 1. First pass: Compile all functions and record their addresses and relocations
/// 2. Second pass: Concatenate code and fix up relocations with correct offsets
pub fn compile_module(module: &r5_ir::Module) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
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
            lowerer.clear_relocations();

            // Compute liveness, allocation, spill/reload, frame layout, and ABI info
            let liveness = compute_liveness(func);
            let allocation = allocate_registers(func, &liveness);
            let spill_reload = create_spill_reload_plan(func, &allocation, &liveness);

            let has_calls = func.blocks.iter().any(|block| {
                block
                    .insts
                    .iter()
                    .any(|inst| matches!(inst, r5_ir::Inst::Call { .. }))
            });

            let max_outgoing_args = compute_max_outgoing_args(func, module);
            let frame_layout = FrameLayout::compute(
                &allocation.used_callee_saved,
                allocation.spill_slot_count,
                has_calls,
                func.signature.params.len(),
                max_outgoing_args,
            );

            let abi_info = Abi::compute_abi_info(func, &allocation, max_outgoing_args);

            let code = lowerer
                .lower_function(func, &allocation, &spill_reload, &frame_layout, &abi_info)
                .map_err(|e| alloc::format!("Failed to lower function '{}': {}", entry_name, e))?;
            let mut code_bytes = code.as_bytes().to_vec();

            // Prepend SP initialization to entry function
            // SP = RAM_OFFSET + ram_size - STACK_SIZE (aligned to 16 bytes)
            // For now, use a reasonable default: assume 4MB RAM, 64KB stack
            // SP = 0x80000000 + 0x400000 - 0x10000 = 0x80030000
            // We'll use lui + addi to set SP
            use riscv32_encoder::{Gpr, Inst};
            const RAM_OFFSET: u32 = 0x80000000;
            const STACK_SIZE: u32 = 64 * 1024; // 64KB
            const DEFAULT_RAM_SIZE: u32 = 4 * 1024 * 1024; // 4MB
            let sp_value = RAM_OFFSET + DEFAULT_RAM_SIZE - STACK_SIZE;
            let sp_value = sp_value & !0xF; // Align to 16 bytes

            // Generate: lui sp, (sp_value >> 12)
            //          addi sp, sp, (sp_value & 0xFFF)
            // Note: lui sets rd = imm << 12, so we pass the full target value
            // and lui will extract bits [31:12] for encoding internally
            let sp_hi_value = sp_value & !0xFFF; // Clear lower 12 bits for lui
            let sp_lo = (sp_value & 0xFFF) as i32;
            let sp_lo_signed = if sp_lo & 0x800 != 0 {
                sp_lo | (-4096i32) // Sign-extend if bit 11 is set
            } else {
                sp_lo
            };

            let mut bootstrap_code = Vec::new();
            // lui sp, sp_hi_value
            // Inst::Lui expects the full 32-bit immediate (it extracts upper 20 bits internally)
            bootstrap_code.extend_from_slice(
                &Inst::Lui {
                    rd: Gpr::SP,
                    imm: sp_hi_value,
                }
                .encode()
                .to_le_bytes(),
            );
            // addi sp, sp, sp_lo_signed
            if sp_lo_signed != 0 {
                bootstrap_code.extend_from_slice(
                    &Inst::Addi {
                        rd: Gpr::SP,
                        rs1: Gpr::SP,
                        imm: sp_lo_signed,
                    }
                    .encode()
                    .to_le_bytes(),
                );
            }

            // Prepend bootstrap code to entry function
            let sp_init_size = bootstrap_code.len();
            bootstrap_code.extend_from_slice(&code_bytes);
            code_bytes = bootstrap_code;

            let code_size = code_bytes.len();
            let mut relocations = lowerer.relocations().to_vec();

            // Adjust relocation offsets for prepended SP initialization
            for reloc in &mut relocations {
                reloc.offset += sp_init_size;
            }

            function_code.insert(entry_name.clone(), code_bytes);
            function_addresses.insert(entry_name.clone(), current_address);
            function_relocations.insert(entry_name.clone(), relocations);
            lowerer.set_function_address(entry_name.clone(), current_address);

            current_address = align_to_4_bytes(current_address as usize + code_size) as u32;
        }
    }

    // Compile remaining functions
    for (name, func) in &module.functions {
        // Skip entry function (already compiled)
        if module
            .entry_function
            .as_ref()
            .map(|e| e == name)
            .unwrap_or(false)
        {
            continue;
        }
        lowerer.clear_relocations();

        // Compute liveness, allocation, spill/reload, frame layout, and ABI info
        let liveness = compute_liveness(func);
        let allocation = allocate_registers(func, &liveness);
        let spill_reload = create_spill_reload_plan(func, &allocation, &liveness);

        let has_calls = func.blocks.iter().any(|block| {
            block
                .insts
                .iter()
                .any(|inst| matches!(inst, r5_ir::Inst::Call { .. }))
        });

        let max_outgoing_args = compute_max_outgoing_args(func, module);
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            allocation.spill_slot_count,
            has_calls,
            func.signature.params.len(),
            max_outgoing_args,
        );

        let abi_info = Abi::compute_abi_info(func, &allocation, max_outgoing_args);

        let code = lowerer
            .lower_function(func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .map_err(|e| alloc::format!("Failed to lower function '{}': {}", name, e))?;
        let code_bytes = code.as_bytes().to_vec();
        let code_size = code_bytes.len();
        let relocations = lowerer.relocations().to_vec();

        function_code.insert(name.clone(), code_bytes);
        function_addresses.insert(name.clone(), current_address);
        function_relocations.insert(name.clone(), relocations);
        lowerer.set_function_address(name.clone(), current_address);

        current_address = align_to_4_bytes(current_address as usize + code_size) as u32;
    }

    // Second pass: concatenate code and fix up relocations
    let mut result = Vec::new();
    let mut current_offset = 0u32;

    for (name, code_bytes) in &function_code {
        // Get relocations for this function
        if let Some(relocations) = function_relocations.get(name) {
            // Copy code and fix up relocations
            let mut code_with_fixups = code_bytes.clone();

            fixup_relocations(
                &mut code_with_fixups,
                relocations,
                &function_addresses,
                current_offset,
            )
            .map_err(|e| alloc::format!("Failed to fix up relocations for function '{}': {}", name, e))?;

            result.extend_from_slice(&code_with_fixups);
        } else {
            result.extend_from_slice(code_bytes);
        }

        // Align to 4-byte boundary
        let aligned_len = align_to_4_bytes(result.len());
        result.resize(aligned_len, 0);

        current_offset = result.len() as u32;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use alloc::{collections::BTreeMap, string::String, vec};

    use r5_ir::{Block, Function, Module, Signature, Type, Value};

    use super::*;

    #[test]
    fn test_align_to_4_bytes() {
        assert_eq!(align_to_4_bytes(0), 0);
        assert_eq!(align_to_4_bytes(1), 4);
        assert_eq!(align_to_4_bytes(4), 4);
        assert_eq!(align_to_4_bytes(5), 8);
        assert_eq!(align_to_4_bytes(8), 8);
    }

    #[test]
    fn test_fixup_relocations() {
        use lower::Relocation;
        use riscv32_encoder;

        // Create mock code with a placeholder jal instruction
        let mut code = vec![0u8; 20];
        let jal_offset = 8;
        // Place a placeholder jal at offset 8
        let placeholder_jal = riscv32_encoder::jal(riscv32_encoder::Gpr::RA, 0);
        let placeholder_bytes = placeholder_jal.to_le_bytes();
        code[jal_offset..jal_offset + 4].copy_from_slice(&placeholder_bytes);

        // Create relocations
        let relocations = vec![Relocation {
            offset: jal_offset,
            callee: String::from("target_func"),
        }];

        // Create function addresses
        let mut function_addresses = BTreeMap::new();
        function_addresses.insert(String::from("target_func"), 100);

        // Fix up relocations
        let current_offset = 0;
        fixup_relocations(&mut code, &relocations, &function_addresses, current_offset).unwrap();

        // Verify the jal instruction was updated
        let fixed_jal_bytes = &code[jal_offset..jal_offset + 4];
        let fixed_jal = u32::from_le_bytes([
            fixed_jal_bytes[0],
            fixed_jal_bytes[1],
            fixed_jal_bytes[2],
            fixed_jal_bytes[3],
        ]);
        // The offset should be 100 - 8 = 92
        let expected_jal = riscv32_encoder::jal(riscv32_encoder::Gpr::RA, 92);
        assert_eq!(fixed_jal, expected_jal);
    }

    #[test]
    fn test_fixup_relocations_out_of_bounds() {
        use lower::Relocation;

        // Test with valid offset first
        let mut code = vec![0u8; 20];
        let relocations = vec![Relocation {
            offset: 8, // This is valid (8 + 4 = 12 <= 20)
            callee: String::from("target_func"),
        }];

        let mut function_addresses = BTreeMap::new();
        function_addresses.insert(String::from("target_func"), 100);

        let result = fixup_relocations(&mut code, &relocations, &function_addresses, 0);
        assert!(result.is_ok());

        // Now test with out-of-bounds offset
        let mut code2 = vec![0u8; 10];
        let relocations2 = vec![Relocation {
            offset: 8, // This is out of bounds (8 + 4 = 12 > 10)
            callee: String::from("target_func"),
        }];

        let result = fixup_relocations(&mut code2, &relocations2, &function_addresses, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_fixup_relocations_missing_function() {
        use lower::Relocation;

        let mut code = vec![0u8; 20];
        let relocations = vec![Relocation {
            offset: 8,
            callee: String::from("nonexistent_func"),
        }];

        let function_addresses = BTreeMap::new();

        let result = fixup_relocations(&mut code, &relocations, &function_addresses, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("nonexistent_func"));
    }

    #[test]
    fn test_compile_module_with_function_call() {
        let mut module = Module::new();

        // Create a simple callee function using direct IR
        let callee_sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let mut callee_func = Function::new(callee_sig);
        let mut callee_block = Block::new();
        let param = Value::new(0);
        callee_block.params.push(param);
        let result = Value::new(1);
        callee_block.push_inst(r5_ir::Inst::Iadd {
            result,
            arg1: param,
            arg2: param,
        });
        callee_block.push_inst(r5_ir::Inst::Return {
            values: vec![result],
        });
        callee_func.add_block(callee_block);
        module.add_function(String::from("callee"), callee_func);

        // Create a simple caller function
        let caller_sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let mut caller_func = Function::new(caller_sig);
        let mut caller_block = Block::new();
        let param = Value::new(0);
        caller_block.params.push(param);
        let result = Value::new(1);
        caller_block.push_inst(r5_ir::Inst::Call {
            callee: String::from("callee"),
            args: vec![param],
            results: vec![result],
        });
        caller_block.push_inst(r5_ir::Inst::Return {
            values: vec![result],
        });
        caller_func.add_block(caller_block);
        module.add_function(String::from("caller"), caller_func);
        module.set_entry_function(String::from("caller"));

        // Compile the module
        let code = compile_module(&module).expect("Compilation failed");

        // Should have compiled code
        assert!(!code.is_empty());
        // Code should be aligned
        assert_eq!(code.len() % 4, 0);
    }

    #[test]
    fn test_compile_module_empty() {
        let module = Module::new();
        let code = compile_module(&module).expect("Compilation failed");
        assert!(code.is_empty());
    }

    #[test]
    fn test_compile_module_single_function() {
        let mut module = Module::new();

        let sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let mut func = Function::new(sig);
        let mut block = Block::new();
        let param = Value::new(0);
        block.params.push(param);
        block.push_inst(r5_ir::Inst::Return {
            values: vec![param],
        });
        func.add_block(block);
        module.add_function(String::from("test"), func);
        module.set_entry_function(String::from("test"));

        let code = compile_module(&module).expect("Compilation failed");
        assert!(!code.is_empty());
        assert_eq!(code.len() % 4, 0);
    }

    #[test]
    fn test_compute_max_outgoing_args_single_call() {
        let ir_module = r#"
module {
    function %callee(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
    block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
        v10 = iadd v0, v1
        return v10
    }

    function %caller() -> i32 {
    block0:
        v0 = iconst 0
        v1 = iconst 1
        v2 = iconst 2
        v3 = iconst 3
        v4 = iconst 4
        v5 = iconst 5
        v6 = iconst 6
        v7 = iconst 7
        v8 = iconst 8
        v9 = iconst 9
        call %callee(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9) -> v10
        return v10
    }
}"#;

        let module = r5_ir::parse_module(ir_module).expect("Failed to parse module");
        let caller_func = module
            .get_function("caller")
            .expect("caller function not found");

        let max_args = compute_max_outgoing_args(caller_func, &module);
        assert_eq!(max_args, 10);
    }

    #[test]
    fn test_compute_max_outgoing_args_multiple_calls() {
        let ir_module = r#"
module {
    function %callee1(i32, i32, i32, i32, i32) -> i32 {
    block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32):
        return v0
    }

    function %callee2(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
    block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32, v10: i32, v11: i32):
        return v0
    }

    function %caller() -> i32 {
    block0:
        v0 = iconst 0
        v1 = iconst 1
        v2 = iconst 2
        v3 = iconst 3
        v4 = iconst 4
        v5 = iconst 5
        v6 = iconst 6
        v7 = iconst 7
        v8 = iconst 8
        v9 = iconst 9
        v10 = iconst 10
        v11 = iconst 11
        call %callee1(v0, v1, v2, v3, v4) -> v12
        call %callee2(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11) -> v13
        return v12
    }
}"#;

        let module = r5_ir::parse_module(ir_module).expect("Failed to parse module");
        let caller_func = module
            .get_function("caller")
            .expect("caller function not found");

        let max_args = compute_max_outgoing_args(caller_func, &module);
        assert_eq!(max_args, 12); // Should be max of 5 and 12
    }

    #[test]
    fn test_compute_max_outgoing_args_nested_calls() {
        let ir_module = r#"
module {
    function %callee(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
    block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32, v10: i32, v11: i32, v12: i32, v13: i32, v14: i32):
        return v0
    }

    function %intermediate() -> i32 {
    block0:
        v0 = iconst 0
        v1 = iconst 1
        v2 = iconst 2
        v3 = iconst 3
        v4 = iconst 4
        v5 = iconst 5
        v6 = iconst 6
        v7 = iconst 7
        v8 = iconst 8
        v9 = iconst 9
        v10 = iconst 10
        v11 = iconst 11
        v12 = iconst 12
        v13 = iconst 13
        v14 = iconst 14
        call %callee(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14) -> v15
        return v15
    }

    function %outer() -> i32 {
    block0:
        call %intermediate() -> v0
        return v0
    }
}"#;

        let module = r5_ir::parse_module(ir_module).expect("Failed to parse module");
        let outer_func = module
            .get_function("outer")
            .expect("outer function not found");

        let max_args = compute_max_outgoing_args(outer_func, &module);
        assert_eq!(max_args, 0); // Outer caller doesn't call callee directly
    }
}
