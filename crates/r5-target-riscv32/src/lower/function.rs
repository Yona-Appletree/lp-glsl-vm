//! Main function lowering logic.

use alloc::collections::BTreeMap;

use riscv32_encoder::Inst as RiscvInst;

use crate::{
    abi::AbiInfo,
    emit::CodeBuffer,
    frame::FrameLayout,
    liveness::InstPoint,
    regalloc::RegisterAllocation,
    spill_reload::SpillReloadPlan,
};

use super::types::{LoweringError, RelocationInstType, RelocationTarget};
use r5_ir::{Function, Value};

/// Lower a function to RISC-V 32-bit code.
///
/// Uses pre-computed allocation, spill/reload plan, and frame layout.
pub(super) fn lower_function_impl(
    lowerer: &mut super::Lowerer,
    func: &Function,
    allocation: &RegisterAllocation,
    spill_reload: &SpillReloadPlan,
    frame_layout: &FrameLayout,
    abi_info: &AbiInfo,
) -> Result<CodeBuffer, LoweringError> {
    let mut code = CodeBuffer::new();

    // Clear function-internal relocations for this function
    lowerer.function_relocations.clear();

    // 1. Generate prologue
    lowerer.gen_prologue(&mut code, func, allocation, frame_layout, abi_info);

    // 2. Track block addresses (instruction indices) for fixup phase
    let mut block_addresses = BTreeMap::new();

    // 3. Phase 1: Lower all blocks, emitting placeholder branches/returns
    // Block addresses are recorded as we go, but branches to future blocks
    // will use relocations that get fixed up in Phase 2
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Record block address (instruction index)
        let block_start_addr = code.instruction_count() as u32;
        block_addresses.insert(block_idx, block_start_addr);

        // Lower block parameters (if any) - these are already in registers from entry
        // Block parameters are handled by the register allocator

        // Lower each instruction
        for (inst_idx, inst) in block.insts.iter().enumerate() {
            let point = InstPoint::new(block_idx, inst_idx + 1);

            // Emit spill/reload operations before instruction
            if let Some(ops) = spill_reload.before.get(&point) {
                for op in ops {
                    lowerer.emit_spill_reload(&mut code, op, frame_layout);
                }
            }

            // Lower the instruction (emits placeholders for branches/returns)
            lowerer.lower_inst(&mut code, inst, allocation, frame_layout, abi_info)?;

            // Emit spill/reload operations after instruction
            let after_point = InstPoint::new(block_idx, inst_idx + 2);
            if let Some(ops) = spill_reload.after.get(&after_point) {
                for op in ops {
                    lowerer.emit_spill_reload(&mut code, op, frame_layout);
                }
            }
        }
    }

    // 4. Generate epilogue
    let epilogue_inst_idx = code.instruction_count() as u32;
    lowerer.gen_epilogue(&mut code, frame_layout, abi_info);

    // 5. Phase 2: Fix up function-internal relocations
    for reloc in &lowerer.function_relocations {
        // Calculate target address (instruction index)
        let target_inst_idx = match &reloc.target {
            RelocationTarget::Block(block_idx) => {
                // Block indices in IR are 0-based and match Vec indices
                // block0 -> index 0, block1 -> index 1, etc.
                *block_addresses.get(block_idx).ok_or_else(|| {
                    LoweringError::UnimplementedInstruction {
                        inst: r5_ir::Inst::Br {
                            condition: Value::new(0),
                            target_true: *block_idx as u32,
                            target_false: 0,
                        },
                    }
                })?
            }
            RelocationTarget::Epilogue => epilogue_inst_idx,
            RelocationTarget::Function(_) => {
                // Function relocations are handled at module level, skip
                continue;
            }
        };

        // Calculate PC-relative offset in bytes
        // When the instruction executes, PC points to the instruction
        // offset = target - PC = (target_inst_idx * 4) - (reloc.offset * 4)
        let target_byte_addr = (target_inst_idx * 4) as i32;
        let inst_byte_addr = (reloc.offset * 4) as i32;
        let offset = target_byte_addr - inst_byte_addr;

        // Update instruction in-place
        match &reloc.inst_type {
            RelocationInstType::Beq { rs1, rs2 } => {
                code.set_instruction(
                    reloc.offset,
                    RiscvInst::Beq {
                        rs1: *rs1,
                        rs2: *rs2,
                        imm: offset,
                    },
                );
            }
            RelocationInstType::Jal { rd } => {
                code.set_instruction(
                    reloc.offset,
                    RiscvInst::Jal {
                        rd: *rd,
                        imm: offset,
                    },
                );
            }
        }
    }

    Ok(code)
}

