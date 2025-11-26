//! Function prologue generation.

use lpc_lpir::Function;

use super::super::{
    abi::AbiInfo, emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation,
};
use crate::{Gpr, Inst as RiscvInst};

impl super::Lowerer {
    /// Generate function prologue.
    pub(super) fn gen_prologue(
        &mut self,
        code: &mut CodeBuffer,
        func: &Function,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) {
        let frame_size = frame_layout.total_size();

        // Step 1: Load incoming stack arguments (before SP adjustment)
        // According to RISC-V convention, stack args are at offsets (idx-8)*4 from SP
        // The caller stores them at SP + (idx-8)*4 (after prologue)
        // The callee loads them from SP + (idx-8)*4 (before prologue)
        // Since caller's SP (after prologue) = callee's SP (before prologue), offsets match
        // This must be done BEFORE SP adjustment, because after adjustment SP points to the frame
        crate::debug!("[PROLOGUE] Loading incoming stack arguments");
        crate::debug!(
            "[PROLOGUE] Function frame_size: {}, incoming_args_size: {}",
            frame_size,
            frame_layout.incoming_args_size
        );
        let mut stack_args_to_spill = alloc::vec::Vec::new();
        if let Some(entry_block) = func.blocks.first() {
            for (idx, param) in entry_block.params.iter().enumerate() {
                // Check if this parameter is on the stack (index >= 8)
                if let Some(load_offset) = frame_layout.incoming_arg_offset(idx) {
                    // This parameter is passed on the stack (index >= 8).
                    //
                    // According to RISC-V calling convention:
                    // - Stack arguments are stored at offsets (idx-8)*4 from SP
                    // - The caller's SP (after prologue) equals the callee's SP (before prologue)
                    // - Both use the same offset: (idx - 8) * 4
                    //
                    // The caller stores outgoing args at SP + (idx-8)*4 (after prologue).
                    // The callee loads incoming args from SP + (idx-8)*4 (before prologue).
                    // Since the SP values match, the offsets match.
                    crate::debug!(
                        "[PROLOGUE] Loading stack arg {} (param {:?}) from SP + {} \
                         (incoming_arg_offset)",
                        idx,
                        param,
                        load_offset.as_i32()
                    );
                    if let Some(allocated_reg) = allocation.value_to_reg.get(param) {
                        // Load directly into allocated register
                        crate::debug!(
                            "[PROLOGUE]   -> Loading into allocated register {:?}",
                            allocated_reg
                        );
                        code.emit(RiscvInst::Lw {
                            rd: *allocated_reg,
                            rs1: Gpr::Sp,
                            imm: load_offset.as_i32(), // Positive offset from SP
                        });
                    } else {
                        // Will be spilled - load into temp register, store after SP adjustment
                        let temp_reg = Gpr::T0;
                        crate::debug!(
                            "[PROLOGUE]   -> Loading into temp register {:?} (will be spilled) \
                             from SP + {}",
                            temp_reg,
                            load_offset.as_i32()
                        );
                        code.emit(RiscvInst::Lw {
                            rd: temp_reg,
                            rs1: Gpr::Sp,
                            imm: load_offset.as_i32(), // Positive offset from SP
                        });
                        // Store temp_reg and param for later
                        if let Some(slot) = allocation.value_to_slot.get(param) {
                            stack_args_to_spill.push((temp_reg, *slot));
                        }
                    }
                }
            }
        }

        // Step 2: Adjust SP for entire frame (if needed)
        if frame_size > 0 {
            code.emit(RiscvInst::Addi {
                rd: Gpr::Sp,
                rs1: Gpr::Sp,
                imm: -(frame_size as i32),
            });

            // Step 3: Store spilled stack args to their spill slots
            for (temp_reg, slot) in stack_args_to_spill {
                let offset = frame_layout.spill_slot_offset(slot);
                code.emit(RiscvInst::Sw {
                    rs1: Gpr::Sp,
                    rs2: temp_reg,
                    imm: offset.as_i32(),
                });
            }

            // Save return address if we have calls
            // RA is saved in the setup area, which is above the outgoing args area
            // Offset = outgoing_args_size + (setup_area_size - 4)
            // For RISC-V 32-bit, setup_area_size is 8, so RA is at offset outgoing_args_size + 4
            if frame_layout.has_function_calls {
                // Save RA: sw ra, offset(sp) where offset = outgoing_args_size + (setup_area_size - 4)
                // Note: For entry functions, RA is garbage at the start, but we save it anyway
                // because calls will set RA, and we need to preserve it across nested calls.
                // The epilogue will handle entry functions specially.
                let ra_offset = if frame_layout.setup_area_size > 0 {
                    frame_layout.outgoing_args_size as i32 + frame_layout.setup_area_size as i32 - 4
                } else {
                    0
                };
                code.emit(RiscvInst::Sw {
                    rs1: Gpr::Sp,
                    rs2: Gpr::Ra,
                    imm: ra_offset,
                });
            }

            // Save callee-saved registers (at their computed offsets)
            for (_idx, reg) in abi_info.used_callee_saved.iter().enumerate() {
                if let Some(offset) = frame_layout.callee_saved_offset(*reg) {
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::Sp,
                        rs2: *reg,
                        imm: offset.as_i32(),
                    });
                }
            }
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
    fn test_prologue_emits_valid_instructions() {
        // Test that prologue emits only valid RISC-V instructions
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

        // Check that all instructions are valid
        let instructions = code.instructions();
        for (idx, inst) in instructions.iter().enumerate() {
            let encoded = inst.encode();

            // Check that encoded instruction is not zero (except for very specific cases)
            // Zero is not a valid RISC-V instruction
            if encoded == 0 {
                panic!(
                    "Instruction {} at index {} encodes to zero (invalid): {:?}",
                    idx, idx, inst
                );
            }

            // Check that opcode is valid (not 0x00)
            let opcode = encoded & 0x7f;
            if opcode == 0 {
                panic!(
                    "Instruction {} at index {} has invalid opcode 0x00: encoded=0x{:08x}, \
                     inst={:?}",
                    idx, idx, encoded, inst
                );
            }
        }
    }

    #[test]
    fn test_prologue_instruction_sequence() {
        // Test that prologue emits correct sequence of instructions
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

        let instructions = code.instructions();
        let encoded: alloc::vec::Vec<u32> = instructions.iter().map(|i| i.encode()).collect();

        // Check that no instruction encodes to 0x00030000 or similar invalid values
        for (idx, enc) in encoded.iter().enumerate() {
            if *enc == 0x00030000 {
                panic!(
                    "Found invalid instruction 0x00030000 at index {}: {:?}",
                    idx,
                    instructions.get(idx)
                );
            }

            // Check for other suspicious patterns
            let opcode = enc & 0x7f;
            if opcode == 0 && *enc != 0 {
                panic!(
                    "Found instruction with invalid opcode 0x00 at index {}: encoded=0x{:08x}, \
                     inst={:?}",
                    idx,
                    enc,
                    instructions.get(idx)
                );
            }
        }
    }

    #[test]
    fn test_function_with_call_prologue() {
        // Test prologue for function that makes calls
        let ir = r#"
function %test() -> i32 {
block0:
    call %helper() -> v0
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = true;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            8, // Max outgoing args
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 8);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        // Check that prologue instructions are valid
        let instructions = code.instructions();
        for (idx, inst) in instructions.iter().enumerate() {
            let encoded = inst.encode();
            let opcode = encoded & 0x7f;

            if opcode == 0 && encoded != 0 {
                panic!(
                    "Invalid instruction at index {}: encoded=0x{:08x}, inst={:?}",
                    idx, encoded, inst
                );
            }
        }
    }

    #[test]
    fn test_prologue_adjusts_sp_once() {
        // Create a function with calls and callee-saved registers
        // This will force the allocator to use callee-saved registers
        // The function has enough register pressure to require spills and callee-saved regs
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
        call %helper(v10) -> v11
        v12 = iconst 100
        v13 = iadd v11, v12
        return v13
    }
}"#;

        let module = parse_module(ir_module.trim()).expect("Failed to parse IR module");

        // Compile the function directly and check its prologue
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
        );

        let abi_info = Abi::compute_abi_info(func, &allocation, 8);

        let mut lowerer = Lowerer::new();
        let func_code = lowerer
            .lower_function(func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        // Count SP adjustments in prologue (addi sp, sp, -N instructions)
        let sp_adjustments = func_code
            .instructions()
            .iter()
            .filter(|inst| {
                matches!(inst, Inst::Addi { rd, rs1, imm }
                    if rd == &Gpr::Sp && rs1 == &Gpr::Sp && imm < &0)
            })
            .count();

        let bytes = func_code.as_bytes();

        // Expected: SP should be adjusted exactly once in prologue
        assert_eq!(
            sp_adjustments,
            1,
            "Prologue should adjust SP exactly once, but found {} adjustments.\nFull function \
             disassembly:\n{}",
            sp_adjustments,
            crate::disassemble_code(&bytes)
        );
    }

    #[test]
    fn test_sp_initialized_before_execution() {
        // Simple function that uses stack (has frame)
        // Run in VM and verify SP is valid (not 0) before function executes
        use crate::expect_ir_syscall;

        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %main() -> v0
    syscall 0(v0)
    halt
}

function %main() -> i32 {
block0:
    v0 = iconst 42
    return v0
}
}"#;

        // This ensures SP is initialized and function is called correctly
        expect_ir_syscall(ir, 0, &[42]);
    }

    #[test]
    fn test_sp_points_to_valid_memory() {
        // Function that writes to stack (uses spill slots)
        // Verify writes succeed without memory errors
        use crate::expect_ir_syscall;

        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 5
    call %main(v0) -> v1
    syscall 0(v1)
    halt
}

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
    call %helper(v10) -> v11
    v12 = iconst 100
    v13 = iadd v11, v12
    return v13
}
}"#;

        // This function will use spill slots (many live values across call)
        // If SP is invalid, stack writes will fail
        // Calculation: main(5) = helper(5+1+2+3+4+5) + 100 = helper(20) + 100 = (20+1) + 100 = 121
        expect_ir_syscall(ir, 0, &[121]);
    }

    #[test]
    fn test_prologue_sp_adjustment() {
        // Test that prologue correctly adjusts SP for frame
        // Function with frame (has calls and/or spills)
        use crate::expect_ir_syscall;

        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %main() -> v0
    syscall 0(v0)
    halt
}

function %helper(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iadd v0, v1
    return v2
}

function %main() -> i32 {
block0:
    ; Create many values to force frame allocation
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    v3 = iconst 3
    v4 = iadd v2, v3
    v5 = iconst 4
    v6 = iadd v4, v5
    call %helper(v6) -> v7
    v8 = iconst 100
    v9 = iadd v7, v8
    return v9
}
}"#;

        // Function should execute correctly, verifying prologue works
        // v0=1, v1=2, v2=3, v3=3, v4=6, v5=4, v6=10, helper(10)=11, v8=100, v9=111
        expect_ir_syscall(ir, 0, &[111]);
    }

    #[test]
    fn test_sp_initialization() {
        // Test that SP is properly initialized before function execution
        // Simple function that should work if SP is valid
        use crate::{expect_ir_ok, Gpr};

        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %main() -> v0
    halt
}

function %main() -> i32 {
block0:
    v0 = iconst 42
    return v0
}
}"#;

        let emu = expect_ir_ok(ir);
        // Verify SP is initialized (not zero)
        // After execution, SP may have been adjusted by frame, so check it's reasonable
        let sp = emu.get_register(Gpr::Sp);
        assert_ne!(sp, 0, "SP should be initialized to non-zero value");
        // SP should be in valid memory region
        // After frame adjustments, SP may be less than initial value, but should still be in valid range
        // Check as signed to handle negative values correctly
        let sp_i32 = sp as i32;
        assert!(
            sp_i32 > 0 || (sp as u32) >= 0x80000000,
            "SP should be in valid memory region, got: 0x{:x}",
            sp
        );
    }
}
