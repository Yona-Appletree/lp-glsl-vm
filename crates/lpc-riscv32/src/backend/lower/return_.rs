//! Return instruction lowering.

use lpc_lpir::Value;
use crate::{Gpr, Inst as RiscvInst};

use super::types::{LoweringError, Relocation, RelocationInstType, RelocationTarget};
use super::super::{abi::AbiInfo, emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation};

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
        // - Values at index >= 8 go to stack at positive offsets from SP
        for (idx, value) in values.iter().enumerate() {
            if let Some(return_reg) = abi_info.return_regs.get(&idx) {
                // Move to return register (first 8 values)
                self.load_value_into_reg(code, *value, *return_reg, allocation, frame_layout)?;
            } else if idx >= 8 {
                // Store to stack (values at index >= 8)
                // Stack returns are stored in the tail-args area, above outgoing args
                // Before epilogue: SP points to bottom of tail-args area (SP = caller_SP - total_size)
                // We store at SP + outgoing_args_size + (idx-8)*4
                // After epilogue: SP is restored to caller_SP, so returns are at:
                //   caller_SP - total_size + outgoing_args_size + (idx-8)*4
                // But caller loads from caller_SP + (idx-8)*4, so we need:
                //   caller_SP - total_size + outgoing_args_size + (idx-8)*4 = caller_SP + (idx-8)*4
                //   => -total_size + outgoing_args_size = 0
                //   => total_size = outgoing_args_size (not true!)
                //
                // Actually, the tail-args area persists after epilogue because it's part of the caller's frame.
                // The caller reserves tail_args_size, which includes space for stack returns.
                // Stack returns should be stored at a location that, after epilogue, is accessible at
                // caller_SP + (idx-8)*4. Since tail-args is at caller_SP+0 to caller_SP+tail_args_size-1,
                // and stack returns go above outgoing args, we store at:
                //   SP + outgoing_args_size + (idx-8)*4 (before epilogue)
                //   = (caller_SP - total_size) + outgoing_args_size + (idx-8)*4
                // After epilogue, SP = caller_SP, so we need to load from:
                //   caller_SP + outgoing_args_size + (idx-8)*4
                // But the caller expects them at caller_SP + (idx-8)*4.
                //
                // Wait, I think the issue is that stack returns should be stored ABOVE the tail-args area,
                // not within it. But that doesn't match Cranelift.
                //
                // Let me re-read Cranelift: they use StackAMode::OutgoingArg(offset + stack_arg_space),
                // which means the offset already includes stack_arg_space. So for a return at index 8,
                // offset is 0, and stack_arg_space is outgoing_args_size, so total offset is outgoing_args_size.
                // They store at SP + outgoing_args_size, and load from SP + outgoing_args_size after epilogue.
                // But the caller's tail-args area must be large enough to include this space.
                //
                // Actually, I think the issue is simpler: stack returns are stored in the caller's tail-args
                // area, which is reserved by the caller. The callee stores them at SP + outgoing_args_size + offset,
                // and after epilogue, the caller loads them from SP + outgoing_args_size + offset (where SP is caller_SP).
                // But the ABI says they should be at SP + offset. So either:
                // 1. The ABI is wrong, or
                // 2. We need to store them differently
                //
                // Let me check: according to RISC-V ABI, stack returns go at (idx-8)*4 relative to SP.
                // But Cranelift stores them at outgoing_args_size + (idx-8)*4. This suggests that
                // the caller's tail-args area layout is: [outgoing args] [stack returns].
                // So the caller loads from SP + outgoing_args_size + (idx-8)*4, not SP + (idx-8)*4.
                //
                // So the fix is: update call lowering to load from SP + outgoing_args_size + offset.
                // But wait, we already do that! So the issue must be in return lowering.
                //
                // Actually, I think the real issue is that we're storing at the wrong location.
                // We should store at SP + outgoing_args_size + offset, which we do.
                // But after epilogue, SP is restored, so they're at caller_SP - total_size + outgoing_args_size + offset.
                // For this to equal caller_SP + outgoing_args_size + offset (where caller loads from),
                // we need total_size = 0, which is wrong.
                //
                // I think the issue is that tail-args area is NOT part of the callee's frame.
                // It's part of the caller's frame, and the callee just uses it.
                // So we should store at SP + total_size + outgoing_args_size + offset, so that after epilogue
                // (when SP is restored by total_size), they're at caller_SP + outgoing_args_size + offset.
                if let Some(stack_offset) = abi_info.return_stack_offsets.get(&idx) {
                    // Load value into temp register
                    let temp_reg = Gpr::T0;
                    self.load_value_into_reg(code, *value, temp_reg, allocation, frame_layout)?;

                    // Store above the frame: SP + total_size + outgoing_args_size + stack_offset
                    // After epilogue restores SP by total_size, these will be at caller_SP + outgoing_args_size + stack_offset
                    let storage_offset = frame_layout.total_size() as i32 + frame_layout.outgoing_args_size as i32 + *stack_offset;
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::Sp,
                        rs2: temp_reg,
                        imm: storage_offset,
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

    use crate::backend::Lowerer;
    use crate::backend::{
        Abi, FrameLayout, compute_liveness, allocate_registers, create_spill_reload_plan,
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
            .filter(|reloc| matches!(reloc.target, crate::backend::lower::RelocationTarget::Epilogue))
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
                assert_eq!(
                    rd,
                    &crate::Gpr::Zero,
                    "Return jal should have rd=ZERO"
                );
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
            "Return value should be in a0 (return register) before jal - either created there or moved"
        );
    }
}
