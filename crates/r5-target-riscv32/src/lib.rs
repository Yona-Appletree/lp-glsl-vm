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
mod test_helpers;

pub use abi::{Abi, AbiInfo};
pub use elf::{debug_elf, generate_elf};
pub use emit::CodeBuffer;
pub use frame::FrameLayout;
pub use liveness::{compute_liveness, LivenessInfo};
pub use lower::Lowerer;
pub use regalloc::{allocate_registers, is_callee_saved, is_caller_saved, RegisterAllocation};
pub use spill_reload::{create_spill_reload_plan, SpillReloadPlan};
pub use test_helpers::{
    debug_ir, debug_ir_with_ram, expect_ir_a0, expect_ir_error, expect_ir_error_with_ram,
    expect_ir_memory_error, expect_ir_memory_error_with_ram, expect_ir_ok, expect_ir_register,
    expect_ir_syscall, expect_ir_unaligned_error,
};

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
    // Include temporary spill slots needed for caller-saved register preservation
    let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
    let frame_layout = FrameLayout::compute(
        &allocation.used_callee_saved,
        total_spill_slots,
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

/// A compiled module containing structured instructions and metadata.
///
/// This allows testing and inspection of generated code before converting to bytes.
pub struct CompiledModule {
    /// All instructions from all functions, concatenated
    pub instructions: alloc::vec::Vec<riscv32_encoder::Inst>,
    /// Relocations that need to be fixed up (instruction indices)
    pub relocations: alloc::vec::Vec<lower::Relocation>,
    /// Function addresses (instruction indices, not byte offsets)
    pub function_addresses: alloc::collections::BTreeMap<alloc::string::String, usize>,
    /// Bootstrap code size (instruction count for entry function)
    pub bootstrap_size: usize,
}

impl CompiledModule {
    /// Convert instructions to bytes.
    ///
    /// Note: Relocations are already fixed up in the instructions,
    /// so this just encodes them to bytes.
    pub fn to_bytes(&self) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
        use alloc::vec::Vec;

        // Convert instructions to bytes (relocations already fixed)
        let mut bytes = Vec::with_capacity(self.instructions.len() * 4);
        for inst in &self.instructions {
            let encoded = inst.encode();
            bytes.extend_from_slice(&encoded.to_le_bytes());
        }

        // Align to 4-byte boundary
        let aligned_len = align_to_4_bytes(bytes.len());
        bytes.resize(aligned_len, 0);

        Ok(bytes)
    }

    /// Format a single instruction as assembly.
    fn format_inst(inst: &riscv32_encoder::Inst) -> alloc::string::String {
        use alloc::format;

        use riscv32_encoder::Gpr;

        fn gpr_name(gpr: &Gpr) -> &'static str {
            match gpr.num() {
                0 => "zero",
                1 => "ra",
                2 => "sp",
                3 => "gp",
                4 => "tp",
                5 => "t0",
                6 => "t1",
                7 => "t2",
                8 => "s0",
                9 => "s1",
                10 => "a0",
                11 => "a1",
                12 => "a2",
                13 => "a3",
                14 => "a4",
                15 => "a5",
                16 => "a6",
                17 => "a7",
                18 => "s2",
                19 => "s3",
                20 => "s4",
                21 => "s5",
                22 => "s6",
                23 => "s7",
                24 => "s8",
                25 => "s9",
                26 => "s10",
                27 => "s11",
                28 => "t3",
                29 => "t4",
                30 => "t5",
                31 => "t6",
                _ => "?",
            }
        }

        match inst {
            riscv32_encoder::Inst::Add { rd, rs1, rs2 } => {
                format!("add {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2))
            }
            riscv32_encoder::Inst::Sub { rd, rs1, rs2 } => {
                format!("sub {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2))
            }
            riscv32_encoder::Inst::Mul { rd, rs1, rs2 } => {
                format!("mul {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2))
            }
            riscv32_encoder::Inst::Addi { rd, rs1, imm } => {
                format!("addi {}, {}, {}", gpr_name(rd), gpr_name(rs1), imm)
            }
            riscv32_encoder::Inst::Lw { rd, rs1, imm } => {
                format!("lw {}, {}({})", gpr_name(rd), imm, gpr_name(rs1))
            }
            riscv32_encoder::Inst::Sw { rs1, rs2, imm } => {
                format!("sw {}, {}({})", gpr_name(rs2), imm, gpr_name(rs1))
            }
            riscv32_encoder::Inst::Jal { rd, imm } => {
                format!("jal {}, {}", gpr_name(rd), imm)
            }
            riscv32_encoder::Inst::Jalr { rd, rs1, imm } => {
                format!("jalr {}, {}({})", gpr_name(rd), imm, gpr_name(rs1))
            }
            riscv32_encoder::Inst::Beq { rs1, rs2, imm } => {
                format!("beq {}, {}, {}", gpr_name(rs1), gpr_name(rs2), imm)
            }
            riscv32_encoder::Inst::Bne { rs1, rs2, imm } => {
                format!("bne {}, {}, {}", gpr_name(rs1), gpr_name(rs2), imm)
            }
            riscv32_encoder::Inst::Blt { rs1, rs2, imm } => {
                format!("blt {}, {}, {}", gpr_name(rs1), gpr_name(rs2), imm)
            }
            riscv32_encoder::Inst::Bge { rs1, rs2, imm } => {
                format!("bge {}, {}, {}", gpr_name(rs1), gpr_name(rs2), imm)
            }
            riscv32_encoder::Inst::Lui { rd, imm } => {
                format!("lui {}, 0x{:05x}", gpr_name(rd), imm)
            }
            riscv32_encoder::Inst::Auipc { rd, imm } => {
                format!("auipc {}, 0x{:05x}", gpr_name(rd), imm)
            }
            riscv32_encoder::Inst::Slt { rd, rs1, rs2 } => {
                format!("slt {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2))
            }
            riscv32_encoder::Inst::Slti { rd, rs1, imm } => {
                format!("slti {}, {}, {}", gpr_name(rd), gpr_name(rs1), imm)
            }
            riscv32_encoder::Inst::Sltu { rd, rs1, rs2 } => {
                format!(
                    "sltu {}, {}, {}",
                    gpr_name(rd),
                    gpr_name(rs1),
                    gpr_name(rs2)
                )
            }
            riscv32_encoder::Inst::Sltiu { rd, rs1, imm } => {
                format!("sltiu {}, {}, {}", gpr_name(rd), gpr_name(rs1), imm)
            }
            riscv32_encoder::Inst::Xori { rd, rs1, imm } => {
                format!("xori {}, {}, {}", gpr_name(rd), gpr_name(rs1), imm)
            }
            riscv32_encoder::Inst::Ecall => alloc::string::String::from("ecall"),
            riscv32_encoder::Inst::Ebreak => alloc::string::String::from("ebreak"),
        }
    }

    /// Get instructions for a specific function by name.
    ///
    /// Returns the instruction range for the function, including bootstrap code
    /// if it's the entry function.
    pub fn function_instructions(&self, function_name: &str) -> Option<&[riscv32_encoder::Inst]> {
        use alloc::vec::Vec;

        let start_idx = self.function_addresses.get(function_name)?;

        // Find the next function's start index to determine the end
        let mut sorted_functions: Vec<_> = self.function_addresses.iter().collect();
        sorted_functions.sort_by_key(|(_, idx)| *idx);

        let end_idx = sorted_functions
            .iter()
            .find(|(name, idx)| *name != function_name && **idx > *start_idx)
            .map(|(_, idx)| **idx)
            .unwrap_or(self.instructions.len());

        // Include bootstrap code if this is the entry function
        let actual_start = if *start_idx == self.bootstrap_size {
            // Entry function - include bootstrap
            0
        } else {
            *start_idx
        };

        Some(&self.instructions[actual_start..end_idx])
    }
}

impl core::fmt::Display for CompiledModule {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use alloc::vec::Vec;

        // Sort functions by their address
        let mut sorted_functions: Vec<_> = self.function_addresses.iter().collect();
        sorted_functions.sort_by_key(|(_, idx)| *idx);

        for (func_name, start_idx) in &sorted_functions {
            // Determine the end index
            let end_idx = sorted_functions
                .iter()
                .find(|(name, idx)| *name != *func_name && **idx > **start_idx)
                .map(|(_, idx)| **idx)
                .unwrap_or(self.instructions.len());

            // Determine the actual start (include bootstrap for entry function)
            let actual_start = if **start_idx == self.bootstrap_size {
                0
            } else {
                **start_idx
            };

            // Write function header
            writeln!(f, "{}:", func_name)?;
            if actual_start < **start_idx {
                writeln!(f, "  # Bootstrap code:")?;
                for (idx, inst) in self.instructions[actual_start..**start_idx]
                    .iter()
                    .enumerate()
                {
                    writeln!(
                        f,
                        "  {:04x}: {}",
                        (actual_start + idx) * 4,
                        Self::format_inst(inst)
                    )?;
                }
                writeln!(f, "  # Function code:")?;
            }

            // Write function instructions
            for (idx, inst) in self.instructions[**start_idx..end_idx].iter().enumerate() {
                writeln!(
                    f,
                    "  {:04x}: {}",
                    (**start_idx + idx) * 4,
                    Self::format_inst(inst)
                )?;
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

/// Fix up relocations in compiled code.
///
/// This updates jal instructions with correct PC-relative offsets to their target functions.
///
/// Note: This is kept for testing purposes. Relocations are now applied in-place
/// during `compile_module_to_insts`.
///
/// TODO: probably want to remove this
#[cfg(test)]
fn fixup_relocations(
    code: &mut [u8],
    relocations: &[lower::Relocation],
    function_addresses: &alloc::collections::BTreeMap<alloc::string::String, u32>,
    current_offset: u32,
) -> Result<(), alloc::string::String> {
    for reloc in relocations {
        // Skip function-internal relocations (Block, Epilogue) - these are fixed up per-function
        match &reloc.target {
            lower::RelocationTarget::Function(_) => {
                // Process function call relocations
            }
            lower::RelocationTarget::Block(_) | lower::RelocationTarget::Epilogue => {
                continue;
            }
        }

        // Convert instruction offset to byte offset
        let byte_offset: lower::ByteOffset = reloc.offset.into();
        let byte_offset_usize = byte_offset.as_i32() as usize;

        // Validate offset is within bounds
        if byte_offset_usize + 4 > code.len() {
            return Err(alloc::format!(
                "Relocation offset {} is out of bounds (code size: {})",
                byte_offset_usize,
                code.len()
            ));
        }

        // Calculate target address
        let callee_name = match &reloc.target {
            lower::RelocationTarget::Function(name) => name,
            _ => unreachable!(), // Already filtered above
        };

        let target_addr = function_addresses
            .get(callee_name)
            .ok_or_else(|| alloc::format!("Function '{}' not found in module", callee_name))?;

        // Calculate PC-relative offset
        // jal is PC-relative: target = PC + offset
        // When jal executes, PC points to the jal instruction
        // offset = target - PC = target - (current_offset + reloc.offset)
        let jal_pc = current_offset
            .checked_add(byte_offset.as_i32() as u32)
            .ok_or_else(|| alloc::string::String::from("Relocation offset overflow"))?;
        let offset = (*target_addr as i32)
            .checked_sub(jal_pc as i32)
            .ok_or_else(|| {
                alloc::string::String::from("Relocation offset calculation underflow")
            })?;

        // Update the jal instruction based on inst_type
        match &reloc.inst_type {
            lower::RelocationInstType::Jal { rd } => {
                let jal_inst = riscv32_encoder::jal(*rd, offset);
                let jal_bytes = jal_inst.to_le_bytes();
                code[byte_offset_usize..byte_offset_usize + 4].copy_from_slice(&jal_bytes);
            }
            lower::RelocationInstType::Beq { .. } => {
                // beq relocations are function-internal and should be fixed up per-function
                // This should not happen at module level
                return Err(alloc::format!(
                    "Unexpected Beq relocation at module level (offset: {})",
                    reloc.offset.as_usize()
                ));
            }
        }
    }
    Ok(())
}

/// Compile an IR module to structured instructions.
///
/// This compiles all functions in the module and returns structured instructions
/// along with relocation information. Use `CompiledModule::to_bytes()` to convert
/// to bytes.
///
/// # Two-Pass Compilation
///
/// The compilation uses a two-pass approach:
/// 1. First pass: Compile all functions and record their addresses and relocations
/// 2. Second pass: Concatenate instructions and record relocation information
pub fn compile_module_to_insts(
    module: &r5_ir::Module,
) -> Result<CompiledModule, alloc::string::String> {
    use alloc::{collections::BTreeMap, vec::Vec};

    let mut lowerer = Lowerer::new();
    lowerer.set_module(module.clone());

    // First pass: compile all functions and record their addresses
    // Compile entry function first, then others
    let mut function_code_buffers = BTreeMap::new();
    let mut function_addresses = BTreeMap::new();
    let mut function_relocations = BTreeMap::new();
    let mut current_inst_idx = 0usize;
    let mut bootstrap_size = 0usize;

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
            // Include temporary spill slots needed for caller-saved register preservation
            let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
            let frame_layout = FrameLayout::compute(
                &allocation.used_callee_saved,
                total_spill_slots,
                has_calls,
                func.signature.params.len(),
                max_outgoing_args,
            );

            let abi_info = Abi::compute_abi_info(func, &allocation, max_outgoing_args);

            // Mark this as an entry function
            lowerer.set_is_entry_function(true);
            let code = lowerer
                .lower_function(func, &allocation, &spill_reload, &frame_layout, &abi_info)
                .map_err(|e| alloc::format!("Failed to lower function '{}': {}", entry_name, e))?;
            lowerer.set_is_entry_function(false);

            // Prepend SP initialization to entry function
            use riscv32_encoder::{Gpr, Inst};
            const RAM_OFFSET: u32 = 0x80000000;
            const SP_OFFSET: u32 = 0x1000; // SP at 0x1000 above RAM boundary
            let sp_value = RAM_OFFSET + SP_OFFSET;
            let sp_value = sp_value & !0xF; // Align to 16 bytes

            let sp_hi_value = sp_value & !0xFFF; // Clear lower 12 bits for lui
            let sp_lo = (sp_value & 0xFFF) as i32;
            let sp_lo_signed = if sp_lo & 0x800 != 0 {
                sp_lo | (-4096i32) // Sign-extend if bit 11 is set
            } else {
                sp_lo
            };

            let mut bootstrap_insts = Vec::new();
            // lui sp, sp_hi_value
            // The lui instruction expects the full 32-bit value; it extracts bits [31:12]
            // sp_hi_value already has lower 12 bits cleared, so we pass it as-is
            bootstrap_insts.push(Inst::Lui {
                rd: Gpr::SP,
                imm: sp_hi_value,
            });
            // addi sp, sp, sp_lo_signed
            if sp_lo_signed != 0 {
                bootstrap_insts.push(Inst::Addi {
                    rd: Gpr::SP,
                    rs1: Gpr::SP,
                    imm: sp_lo_signed,
                });
            }

            bootstrap_size = bootstrap_insts.len();

            // Function address is after bootstrap code
            let function_inst_idx = current_inst_idx + bootstrap_size;

            // Store code buffer and relocations
            let mut relocations = lowerer.relocations().to_vec();
            // Adjust relocation offsets for prepended bootstrap
            // reloc.offset is already an instruction index within the function's code
            for reloc in &mut relocations {
                // Add the bootstrap size and current offset to get final instruction index
                reloc.offset = lower::InstOffset::from(current_inst_idx + bootstrap_size + reloc.offset.as_usize());
            }

            let code_inst_count = code.instruction_count();
            function_code_buffers.insert(entry_name.clone(), (bootstrap_insts, code));
            function_addresses.insert(entry_name.clone(), function_inst_idx);
            function_relocations.insert(entry_name.clone(), relocations);
            lowerer.set_function_address(entry_name.clone(), function_inst_idx as u32 * 4);

            // Update current_inst_idx (bootstrap + function code)
            current_inst_idx += bootstrap_size + code_inst_count.as_usize();
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
        lowerer.set_is_entry_function(false); // Not an entry function

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
        // Include temporary spill slots needed for caller-saved register preservation
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            max_outgoing_args,
        );

        let abi_info = Abi::compute_abi_info(func, &allocation, max_outgoing_args);

        let code = lowerer
            .lower_function(func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .map_err(|e| alloc::format!("Failed to lower function '{}': {}", name, e))?;

        // Adjust relocation offsets (already instruction indices)
        let code_inst_count = code.instruction_count();
        let mut relocations = lowerer.relocations().to_vec();
        for reloc in &mut relocations {
            // reloc.offset is already an instruction index within the function's code
            // Add current offset to get final instruction index
            reloc.offset = lower::InstOffset::from(current_inst_idx + reloc.offset.as_usize());
        }

        function_code_buffers.insert(name.clone(), (Vec::new(), code));
        function_addresses.insert(name.clone(), current_inst_idx);
        function_relocations.insert(name.clone(), relocations);
        lowerer.set_function_address(name.clone(), current_inst_idx as u32 * 4);

        current_inst_idx += code_inst_count.as_usize();
    }

    // Second pass: concatenate all instructions in the order they were processed
    // (entry function first, then others in module order)
    // Relocations are already adjusted to instruction indices in this order
    let mut all_instructions = Vec::new();
    let mut all_relocations = Vec::new();

    // First, add entry function if it exists
    if let Some(entry_name) = &module.entry_function {
        if let Some((bootstrap_insts, code_buffer)) = function_code_buffers.get(entry_name) {
            all_instructions.extend_from_slice(bootstrap_insts);
            all_instructions.extend_from_slice(code_buffer.instructions());
            if let Some(relocs) = function_relocations.get(entry_name) {
                all_relocations.extend_from_slice(relocs);
            }
        }
    }

    // Then add remaining functions in module order
    for (name, _) in &module.functions {
        // Skip entry function (already added)
        if module
            .entry_function
            .as_ref()
            .map(|e| e == name)
            .unwrap_or(false)
        {
            continue;
        }

        if let Some((bootstrap_insts, code_buffer)) = function_code_buffers.get(name) {
            all_instructions.extend_from_slice(bootstrap_insts);
            all_instructions.extend_from_slice(code_buffer.instructions());
            if let Some(relocs) = function_relocations.get(name) {
                all_relocations.extend_from_slice(relocs);
            }
        }
    }

    // Third pass: fix up relocations in-place
    // This allows tests to see the final opcodes directly
    // Relocations already have correct instruction indices, we just need to fix the jal offsets
    // Note: Function-internal relocations (Block, Epilogue) are already fixed up per-function
    for reloc in &all_relocations {
        // Skip function-internal relocations - these are already fixed up
        match &reloc.target {
            lower::RelocationTarget::Function(_) => {
                // Process function call relocations
            }
            lower::RelocationTarget::Block(_) | lower::RelocationTarget::Epilogue => {
                continue;
            }
        }

        let inst_idx = reloc.offset.as_usize();
        if inst_idx >= all_instructions.len() {
            return Err(alloc::format!(
                "Relocation offset {} is out of bounds (instruction count: {})",
                inst_idx,
                all_instructions.len()
            ));
        }

        // Calculate target address (instruction index)
        let callee_name = match &reloc.target {
            lower::RelocationTarget::Function(name) => name,
            _ => unreachable!(), // Already filtered above
        };

        let target_inst_idx = function_addresses
            .get(callee_name)
            .ok_or_else(|| alloc::format!("Function '{}' not found", callee_name))?;

        // Calculate PC-relative offset in bytes
        // jal is PC-relative: target = PC + offset
        // When jal executes, PC points to the jal instruction
        // offset = target - PC = (target_inst_idx * 4) - (inst_idx * 4)
        let target_byte_offset: lower::ByteOffset = lower::InstOffset::from(*target_inst_idx).into();
        let jal_pc_byte_offset: lower::ByteOffset = reloc.offset.into();
        let offset = target_byte_offset.as_i32() - jal_pc_byte_offset.as_i32();

        // Update the jal instruction in-place based on inst_type
        match &reloc.inst_type {
            lower::RelocationInstType::Jal { rd } => {
                all_instructions[inst_idx] = riscv32_encoder::Inst::Jal {
                    rd: *rd,
                    imm: offset,
                };
            }
            lower::RelocationInstType::Beq { .. } => {
                // beq relocations are function-internal and should be fixed up per-function
                // This should not happen at module level
                return Err(alloc::format!(
                    "Unexpected Beq relocation at module level (offset: {})",
                    inst_idx
                ));
            }
        }
    }

    Ok(CompiledModule {
        instructions: all_instructions,
        relocations: all_relocations, // Keep for debugging/inspection
        function_addresses,
        bootstrap_size,
    })
}

/// Compile an IR module to RISC-V 32-bit code.
///
/// This compiles all functions in the module and handles function call relocations.
/// Returns the compiled code with all functions concatenated.
///
/// This is a convenience function that calls `compile_module_to_insts()` and then
/// converts to bytes. For testing and inspection, use `compile_module_to_insts()` directly.
pub fn compile_module(
    module: &r5_ir::Module,
) -> Result<alloc::vec::Vec<u8>, alloc::string::String> {
    compile_module_to_insts(module)?.to_bytes()
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
            offset: lower::InstOffset::from(jal_offset),
            target: lower::RelocationTarget::Function(String::from("target_func")),
            inst_type: lower::RelocationInstType::Jal {
                rd: riscv32_encoder::Gpr::RA,
            },
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
            offset: lower::InstOffset::from(8), // This is valid (8 + 4 = 12 <= 20)
            target: lower::RelocationTarget::Function(String::from("target_func")),
            inst_type: lower::RelocationInstType::Jal {
                rd: riscv32_encoder::Gpr::RA,
            },
        }];

        let mut function_addresses = BTreeMap::new();
        function_addresses.insert(String::from("target_func"), 100);

        let result = fixup_relocations(&mut code, &relocations, &function_addresses, 0);
        assert!(result.is_ok());

        // Now test with out-of-bounds offset
        let mut code2 = vec![0u8; 10];
        let relocations2 = vec![Relocation {
            offset: lower::InstOffset::from(8), // This is out of bounds (8 + 4 = 12 > 10)
            target: lower::RelocationTarget::Function(String::from("target_func")),
            inst_type: lower::RelocationInstType::Jal {
                rd: riscv32_encoder::Gpr::RA,
            },
        }];

        let result = fixup_relocations(&mut code2, &relocations2, &function_addresses, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_fixup_relocations_missing_function() {
        use lower::Relocation;

        let mut code = vec![0u8; 20];
        let relocations = vec![Relocation {
            offset: lower::InstOffset::from(8),
            target: lower::RelocationTarget::Function(String::from("nonexistent_func")),
            inst_type: lower::RelocationInstType::Jal {
                rd: riscv32_encoder::Gpr::RA,
            },
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
