//! Function epilogue generation.

use riscv32_encoder::{Gpr, Inst as RiscvInst};

use crate::{abi::AbiInfo, emit::CodeBuffer, frame::FrameLayout};

impl super::Lowerer {
    /// Generate function epilogue.
    pub(super) fn gen_epilogue(
        &mut self,
        code: &mut CodeBuffer,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) {
        let frame_size = frame_layout.total_size();

        if frame_size > 0 {
            // Restore callee-saved registers (reverse order)
            for reg in abi_info.used_callee_saved.iter().rev() {
                if let Some(offset) = frame_layout.callee_saved_offset(*reg) {
                    code.emit(RiscvInst::Lw {
                        rd: *reg,
                        rs1: Gpr::SP,
                        imm: offset,
                    });
                }
            }

            // Restore return address if we saved it (before restoring SP)
            // For entry functions, we saved garbage RA at the start, so don't restore it.
            // The current RA (set by calls) should be used, or we'll emit ebreak.
            if frame_layout.has_function_calls && !self.is_entry_function {
                let ra_offset = if frame_layout.setup_area_size > 0 {
                    frame_layout.setup_area_size as i32 - 4
                } else {
                    0
                };
                code.emit(RiscvInst::Lw {
                    rd: Gpr::RA,
                    rs1: Gpr::SP,
                    imm: ra_offset,
                });
            }

            // Restore stack pointer: addi sp, sp, frame_size
            code.emit(RiscvInst::Addi {
                rd: Gpr::SP,
                rs1: Gpr::SP,
                imm: frame_size as i32,
            });
        }

        // Return: jalr x0, ra, 0 (if RA is valid)
        // For entry functions, we saved garbage RA at the start, so we didn't restore it.
        // If the function made calls, RA should be valid (set by the last call).
        // If it didn't make calls, RA is still garbage, so emit ebreak.
        if self.is_entry_function {
            if frame_layout.has_function_calls {
                // Entry function that made calls: RA is valid (set by calls), use it
                code.emit(RiscvInst::Jalr {
                    rd: Gpr::ZERO,
                    rs1: Gpr::RA,
                    imm: 0,
                });
            } else {
                // Entry function with no calls: RA is garbage, halt execution
                code.emit(RiscvInst::Ebreak);
            }
        } else if frame_layout.has_function_calls {
            // RA was saved and restored, so we can return normally
            code.emit(RiscvInst::Jalr {
                rd: Gpr::ZERO,
                rs1: Gpr::RA,
                imm: 0,
            });
        } else {
            // Leaf function: RA is valid (set by caller), so we can return normally
            code.emit(RiscvInst::Jalr {
                rd: Gpr::ZERO,
                rs1: Gpr::RA,
                imm: 0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use r5_ir::parse_function;

    use super::super::Lowerer;
    use crate::{
        abi::Abi, frame::FrameLayout, liveness::compute_liveness, regalloc::allocate_registers,
        spill_reload::create_spill_reload_plan,
    };

    #[test]
    fn test_epilogue_ends_with_jalr() {
        // Test that epilogue ends with jalr instruction
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 42
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
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        // Should end with jalr (return)
        let instructions = code.instructions();
        assert!(matches!(
            instructions.last(),
            Some(riscv32_encoder::Inst::Jalr { .. })
        ));
    }
}
