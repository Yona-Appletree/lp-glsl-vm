//! Return instruction lowering.

use lpc_lpir::Value;

use super::{
    super::{abi::AbiInfo, emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation},
    types::{LoweringError, Relocation, RelocationInstType, RelocationTarget},
};
use crate::{Gpr, Inst as RiscvInst};

impl super::Lowerer {
    /// Lower return instruction.
    ///
    /// Moves return values to return registers and jumps to the epilogue.
    /// Emits a placeholder jal and records a relocation for fixup.
    pub(super) fn lower_return(
        &mut self,
        code: &mut CodeBuffer,
        values: &[Value],
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) -> Result<(), LoweringError> {
        // Process all return values in a single pass:
        // - First 8 values go to return registers (a0-a7)
        // - Values at index >= 8 go to stack in the tail-args area
        for (idx, value) in values.iter().enumerate() {
            if let Some(return_reg) = abi_info.return_regs.get(&idx) {
                // Move to return register (first 8 values)
                self.load_value_into_reg(code, *value, *return_reg, allocation, frame_layout)?;
            } else if idx >= 8 {
                // Store to stack (values at index >= 8)
                // Use FrameLayout helper to get the correct offset for storing stack returns.
                // Stack returns are stored in the tail-args area, above outgoing args.
                // The offset is relative to SP after prologue (callee's adjusted SP).
                if let Some(store_offset) = frame_layout.stack_return_store_offset(idx) {
                    // Load value into temp register
                    let temp_reg = Gpr::T0;
                    self.load_value_into_reg(code, *value, temp_reg, allocation, frame_layout)?;

                    // Store to tail-args area using FrameLayout helper
                    // The offset is already correct relative to SP after prologue
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::Sp,
                        rs2: temp_reg,
                        imm: store_offset.as_i32(),
                    });
                }
            }
        }

        // Emit placeholder jal instruction (offset 0, will be fixed up)
        let jal_inst_idx = code.instruction_count();
        code.emit(RiscvInst::Jal {
            rd: Gpr::Zero,
            imm: 0, // Placeholder
        });

        // Record relocation for jal (epilogue target)
        self.function_relocations.push(Relocation {
            offset: jal_inst_idx,
            target: RelocationTarget::Epilogue,
            inst_type: RelocationInstType::Jal { rd: Gpr::Zero },
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec::Vec;

    use lpc_lpir::parse_function;

    use crate::backend::{
        allocate_registers, compute_liveness, create_spill_reload_plan, Abi, FrameLayout, Lowerer,
    };

    #[test]
    fn test_lower_return() {
        // Function that returns a value
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
            func.signature.returns.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        assert!(code.instruction_count().as_usize() > 0);

        // Verify that a relocation was recorded for the epilogue
        let epilogue_relocs: Vec<_> = lowerer
            .function_relocations
            .iter()
            .filter(|reloc| {
                matches!(
                    reloc.target,
                    crate::backend::lower::RelocationTarget::Epilogue
                )
            })
            .collect();
        assert_eq!(
            epilogue_relocs.len(),
            1,
            "Expected exactly one epilogue relocation for return instruction"
        );

        // Verify that the relocation is for a jal instruction with rd=ZERO
        let reloc = &epilogue_relocs[0];
        match &reloc.inst_type {
            crate::backend::lower::RelocationInstType::Jal { rd } => {
                assert_eq!(rd, &crate::Gpr::Zero, "Return jal should have rd=ZERO");
            }
            _ => panic!("Return relocation should be for jal instruction"),
        }

        // Verify that a jal instruction was emitted at the relocation offset
        // (Note: the offset is in instruction count, and instructions are 4 bytes)
        let instructions = code.instructions();
        let jal_inst = instructions
            .get(reloc.offset.as_usize())
            .expect("Jal instruction should exist at relocation offset");
        assert!(
            matches!(jal_inst, crate::Inst::Jal { .. }),
            "Jal instruction should be emitted by return (not jalr)"
        );

        // Verify that the return value is in a0 (return register)
        // The return value (v0 = iconst 10) should either:
        // 1. Be created directly in a0 (addi a0, x0, 10), or
        // 2. Be moved to a0 before the jal
        let mut found_a0_usage = false;
        for inst in instructions.iter().take(reloc.offset.as_usize()) {
            match inst {
                crate::Inst::Addi { rd, rs1, imm } if *rd == crate::Gpr::A0 => {
                    if *rs1 == crate::Gpr::Zero {
                        // Value created directly in a0 (addi a0, x0, imm)
                        found_a0_usage = true;
                        break;
                    } else if *imm == 0 {
                        // This is a move instruction (addi rd, rs, 0) to a0
                        found_a0_usage = true;
                        break;
                    }
                }
                crate::Inst::Lw { rd, .. } if *rd == crate::Gpr::A0 => {
                    // Value was reloaded from spill slot to a0
                    found_a0_usage = true;
                    break;
                }
                crate::Inst::Lui { rd, .. } if *rd == crate::Gpr::A0 => {
                    // Part of large constant loading into a0
                    found_a0_usage = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(
            found_a0_usage,
            "Return value should be in a0 (return register) before jal - either created there or \
             moved"
        );
    }
}
