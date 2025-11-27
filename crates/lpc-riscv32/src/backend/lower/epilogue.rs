//! Function epilogue generation.

use super::{
    super::{abi::AbiInfo, emit::CodeBuffer, frame::FrameLayout},
    types::ByteSize,
};
use crate::{Gpr, Inst as RiscvInst};

impl super::Lowerer {
    /// Generate function epilogue.
    pub(super) fn gen_epilogue(
        &mut self,
        code: &mut CodeBuffer,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) {
        let local_frame_size = frame_layout.local_frame_size();
        let tail_adjustment = frame_layout.tail_adjustment();

        // Epilogue reverses the two-phase SP adjustment:
        // Phase 1: Restore local frame (restore callee-saved regs, RA, then restore SP for local frame)
        // Phase 2: Undo tail adjustment (if any)

        if local_frame_size > 0 {
            // Restore callee-saved registers (reverse order)
            for reg in abi_info.used_callee_saved.iter().rev() {
                if let Some(offset) = frame_layout.callee_saved_offset(*reg) {
                    code.emit(RiscvInst::Lw {
                        rd: *reg,
                        rs1: Gpr::Sp,
                        imm: offset.as_i32(),
                    });
                }
            }

            // Restore return address if we saved it (before restoring SP)
            // RA is saved in the setup area, which is above the tail-args area
            // Offset = tail_args_size + (setup_area_size - 4)
            // For entry functions, we saved garbage RA at the start, so don't restore it.
            // The current RA (set by calls) should be used, or we'll emit ebreak.
            if frame_layout.has_function_calls && !self.is_entry_function {
                let ra_offset = if frame_layout.setup_area_size > ByteSize::new(0) {
                    i32::from(frame_layout.tail_args_size + frame_layout.setup_area_size) - 4
                } else {
                    0
                };
                code.emit(RiscvInst::Lw {
                    rd: Gpr::Ra,
                    rs1: Gpr::Sp,
                    imm: ra_offset,
                });
            }

            // Phase 1: Restore stack pointer for local frame
            code.emit(RiscvInst::Addi {
                rd: Gpr::Sp,
                rs1: Gpr::Sp,
                imm: local_frame_size as i32,
            });
        }

        // Phase 2: Undo tail adjustment (if any)
        if tail_adjustment > 0 {
            code.emit(RiscvInst::Addi {
                rd: Gpr::Sp,
                rs1: Gpr::Sp,
                imm: tail_adjustment,
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
                    rd: Gpr::Zero,
                    rs1: Gpr::Ra,
                    imm: 0,
                });
            } else {
                // Entry function with no calls: RA is garbage, halt execution
                code.emit(RiscvInst::Ebreak);
            }
        } else if frame_layout.has_function_calls {
            // RA was saved and restored, so we can return normally
            code.emit(RiscvInst::Jalr {
                rd: Gpr::Zero,
                rs1: Gpr::Ra,
                imm: 0,
            });
        } else {
            // Leaf function: RA is valid (set by caller), so we can return normally
            code.emit(RiscvInst::Jalr {
                rd: Gpr::Zero,
                rs1: Gpr::Ra,
                imm: 0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use lpc_lpir::parse_function;

    use crate::backend::{
        allocate_registers, compute_liveness, create_spill_reload_plan, Abi, FrameLayout, Lowerer,
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
            func.signature.returns.len(),
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
            Some(crate::Inst::Jalr { .. })
        ));
    }

    #[test]
    fn test_epilogue_restores_correct_order() {
        // Function that uses callee-saved registers and makes calls
        // Verify epilogue order: restore callee-saved → restore RA → adjust SP
        use lpc_lpir::parse_module;

        use crate::{Gpr, Inst};

        let ir_module = r#"
module {
    function %helper(i32) -> i32 {
    block0(v0: i32):
        v1 = iconst 1
        v2 = iadd v0, v1
        return v2
    }

    function %main(i32) -> i32 {
    block0(v0: i32):
        v1 = iconst 1
        v2 = iadd v0, v1
        v3 = iconst 2
        v4 = iadd v2, v3
        v5 = iconst 3
        v6 = iadd v4, v5
        v7 = iconst 4
        v8 = iadd v6, v7
        v9 = iconst 5
        v10 = iadd v8, v9
        v11 = iconst 6
        v12 = iadd v10, v11
        call %helper(v12) -> v13
        v14 = iconst 100
        v15 = iadd v13, v14
        return v15
    }
}"#;

        let module = parse_module(ir_module).expect("Failed to parse IR module");

        // Compile the function directly and check its epilogue
        let func = module
            .functions
            .get("main")
            .expect("main function not found");

        let liveness = crate::backend::compute_liveness(func);
        let allocation = crate::backend::allocate_registers(func, &liveness);
        let spill_reload = crate::backend::create_spill_reload_plan(func, &allocation, &liveness);

        let has_calls = true;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            8,
            func.signature.returns.len(),
            8,
        );

        let abi_info = Abi::compute_abi_info(func, &allocation, 8);

        let mut lowerer = Lowerer::new();
        let func_code = lowerer
            .lower_function(func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        // Get epilogue instructions (last few instructions before return)
        let bytes = func_code.as_bytes();
        let epilogue_start = bytes.len().saturating_sub(20 * 4); // Last 20 instructions
        let start_idx = epilogue_start / 4;
        let instructions = &func_code.instructions()[start_idx..];

        let ra_restore_pos = instructions.iter().position(|inst| {
            matches!(inst, Inst::Lw { rd, rs1, .. }
                    if rd == &Gpr::Ra && rs1 == &Gpr::Sp)
        });
        let sp_adjust_pos = instructions.iter().position(|inst| {
            matches!(inst, Inst::Addi { rd, rs1, imm }
                    if rd == &Gpr::Sp && rs1 == &Gpr::Sp && imm > &0)
        });

        // RA should be restored before SP is adjusted
        if let (Some(ra_pos), Some(sp_pos)) = (ra_restore_pos, sp_adjust_pos) {
            assert!(
                ra_pos < sp_pos,
                "RA should be restored before SP adjustment. RA at {}, SP at {}",
                ra_pos,
                sp_pos
            );
        } else {
            // If we don't have both, that's also a problem
            assert!(
                ra_restore_pos.is_some() && sp_adjust_pos.is_some(),
                "Epilogue should restore RA and adjust SP. Found RA restore: {:?}, SP adjust: {:?}",
                ra_restore_pos,
                sp_adjust_pos
            );
        }
    }

    #[test]
    fn test_epilogue_sp_restoration() {
        // Test that epilogue correctly restores SP
        // Nested calls to verify SP is restored at each level
        use crate::expect_ir_syscall;

        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %outer() -> v0
    syscall 0(v0)
    halt
}

function %inner(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iadd v0, v1
    return v2
}

function %middle(i32) -> i32 {
block0(v0: i32):
    ; Create local values that need frame
    v1 = iconst 10
    v2 = iadd v0, v1
    call %inner(v2) -> v3
    v4 = iconst 5
    v5 = iadd v3, v4
    return v5
}

function %outer() -> i32 {
block0:
    v0 = iconst 100
    call %middle(v0) -> v1
    v2 = iconst 50
    v3 = iadd v1, v2
    return v3
}
}"#;

        // outer: v0=100, middle(100): v2=110, inner(110)=111, v5=116, v3=166
        expect_ir_syscall(ir, 0, &[166]);
    }
}
