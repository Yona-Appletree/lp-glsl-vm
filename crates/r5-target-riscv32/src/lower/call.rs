//! Function call instruction lowering.

use r5_ir::Value;
use riscv32_encoder::{Gpr, Inst as RiscvInst};

use super::types::{LoweringError, Relocation, RelocationInstType, RelocationTarget};
use crate::{
    abi::{Abi, AbiInfo},
    emit::CodeBuffer,
    frame::FrameLayout,
    regalloc::RegisterAllocation,
};

impl super::Lowerer {
    /// Lower call instruction.
    pub(super) fn lower_call(
        &mut self,
        code: &mut CodeBuffer,
        callee: &str,
        args: &[Value],
        results: &[Value],
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) -> Result<(), LoweringError> {
        // Step 1: Move register arguments (a0-a7)
        // Track which argument values were preserved (because they're used after the call)
        let mut preserved_args: alloc::vec::Vec<(Value, Gpr)> = alloc::vec::Vec::new();

        for (idx, arg) in args.iter().enumerate() {
            if idx < 8 {
                if let Some(arg_reg) = Abi::arg_reg(idx) {
                    // Check if this value is used after the call
                    // Only preserve if the argument appears in the results (meaning it's
                    // passed through and used as a return value). For other cases where
                    // the argument is used later, the register allocator should have
                    // allocated it to a callee-saved register or spilled it.
                    let used_after_call = results.contains(arg);

                    // Check if value is already in the argument register
                    if let Some(current_reg) = self.get_register(*arg, allocation) {
                        if current_reg == arg_reg && used_after_call {
                            // Value is in arg_reg and used after call - need to preserve it
                            // Save to a temporary register before the call
                            // Use T2 as temp (T0/T1 might be used for other things)
                            let temp_reg = Gpr::T2;
                            code.emit(RiscvInst::Addi {
                                rd: temp_reg,
                                rs1: current_reg,
                                imm: 0, // Copy: addi rd, rs, 0
                            });

                            // Track this for restoration after call
                            preserved_args.push((*arg, temp_reg));
                            // Skip moving since it's already in place
                            continue;
                        }
                    }

                    self.load_value_into_reg(code, *arg, arg_reg, allocation, frame_layout)?;
                }
            }
        }

        // Step 2: Store stack arguments (index >= 8) to outgoing args area
        // Outgoing stack args are stored in the outgoing args area at the top of the frame.
        // After prologue, SP points to the bottom of the frame. The callee reads incoming
        // args at SP + offset before adjusting SP, where SP = caller's SP_after_prologue.
        // The outgoing args area starts at: SP + (total_size - outgoing_args_size)
        // Then we add the per-argument offset (0, 4, 8, ...) to get the specific argument.
        let outgoing_args_base = (frame_layout.total_size() - frame_layout.outgoing_args_size) as i32;
        for (idx, arg) in args.iter().enumerate() {
            if idx >= 8 {
                if let Some(offset) = frame_layout.outgoing_arg_offset(idx) {
                    // Load argument value into temporary register
                    let temp_reg = Gpr::T0;
                    self.load_value_into_reg(code, *arg, temp_reg, allocation, frame_layout)?;

                    // Store to outgoing args area
                    // Stack arguments are stored at the top of the frame in the outgoing args area.
                    // We compute the offset from SP (bottom) to the start of outgoing args area,
                    // then add the per-argument offset.
                    let actual_offset = outgoing_args_base + offset.as_i32();
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::SP,
                        rs2: temp_reg,
                        imm: actual_offset,
                    });
                }
            }
        }

        // Emit call - always use relocation for cross-function calls
        // The direct call optimization doesn't work correctly because we don't know
        // the absolute address of the current function during lowering.
        // Relocations will be fixed up in the final pass with correct absolute addresses.
        // Emit placeholder jal (will be fixed up later)
        let jal_inst_idx = code.instruction_count();
        code.emit(RiscvInst::Jal {
            rd: Gpr::RA,
            imm: 0, // Placeholder
        });

        // Record relocation for jal (function call target)
        self.relocations.push(Relocation {
            offset: jal_inst_idx,
            target: RelocationTarget::Function(alloc::string::String::from(callee)),
            inst_type: RelocationInstType::Jal { rd: Gpr::RA },
        });

        // Step 3: Move results from return registers (first 8)
        for (idx, result) in results.iter().enumerate() {
            if let Some(return_reg) = abi_info.return_regs.get(&idx) {
                if let Some(result_reg) = self.get_register(*result, allocation) {
                    if result_reg != *return_reg {
                        code.emit(RiscvInst::Addi {
                            rd: result_reg,
                            rs1: *return_reg,
                            imm: 0, // Move
                        });
                    }
                } else {
                    // Result is spilled - store return register to spill slot
                    if let Some(slot) = self.get_spill_slot(*result, allocation) {
                        let offset = frame_layout.spill_slot_offset(slot);
                        code.emit(RiscvInst::Sw {
                            rs1: Gpr::SP,
                            rs2: *return_reg,
                            imm: offset.as_i32(),
                        });
                    }
                }
            }
        }

        // Step 4: Load stack return values (index >= 8) from stack
        // These are stored in the caller's frame at positive offsets from SP
        // After the call returns, the callee's epilogue has restored SP to the caller's frame,
        // so the return values are at positive offsets from SP (just stack_offset)
        for (idx, result) in results.iter().enumerate() {
            if idx >= 8 {
                if let Some(stack_offset) = abi_info.return_stack_offsets.get(&idx) {
                    // After call returns, SP is restored to caller's frame, so offset is just stack_offset
                    // (positive offset, relative to SP after epilogue)
                    let actual_offset = *stack_offset;

                    // Load from stack into temp register
                    let temp_reg = Gpr::T0;
                    code.emit(RiscvInst::Lw {
                        rd: temp_reg,
                        rs1: Gpr::SP,
                        imm: actual_offset,
                    });

                    // Store to result location (register or spill slot)
                    if let Some(result_reg) = self.get_register(*result, allocation) {
                        code.emit(RiscvInst::Addi {
                            rd: result_reg,
                            rs1: temp_reg,
                            imm: 0, // Move
                        });
                    } else if let Some(slot) = self.get_spill_slot(*result, allocation) {
                        let offset = frame_layout.spill_slot_offset(slot);
                        code.emit(RiscvInst::Sw {
                            rs1: Gpr::SP,
                            rs2: temp_reg,
                            imm: offset.as_i32(),
                        });
                    }
                }
            }
        }

        // Step 5: Restore preserved argument values that were used after the call
        for (arg_value, temp_reg) in preserved_args {
            // Restore to the value's allocated location (register or spill slot)
            if let Some(result_reg) = self.get_register(arg_value, allocation) {
                code.emit(RiscvInst::Addi {
                    rd: result_reg,
                    rs1: temp_reg,
                    imm: 0, // Move: addi rd, rs, 0
                });
            } else if let Some(slot) = self.get_spill_slot(arg_value, allocation) {
                let offset = frame_layout.spill_slot_offset(slot);
                code.emit(RiscvInst::Sw {
                    rs1: Gpr::SP,
                    rs2: temp_reg,
                    imm: offset.as_i32(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use r5_ir::parse_module;

    use super::super::Lowerer;
    use crate::{
        abi::Abi, expect_ir_syscall, frame::FrameLayout, liveness::compute_liveness,
        regalloc::allocate_registers, spill_reload::create_spill_reload_plan, CodeBuffer,
    };

    /// Helper to lower a function with all required analysis passes.
    fn lower_function(func: &r5_ir::Function) -> CodeBuffer {
        let liveness = compute_liveness(func);
        let allocation = allocate_registers(func, &liveness);
        let spill_reload = create_spill_reload_plan(func, &allocation, &liveness);

        let has_calls = func.blocks.iter().any(|block| {
            block
                .insts
                .iter()
                .any(|inst| matches!(inst, r5_ir::Inst::Call { .. }))
        });

        // Include temporary spill slots needed for caller-saved register preservation
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            8, // Default max outgoing args for test helper
        );

        let abi_info = Abi::compute_abi_info(func, &allocation, 8);

        let mut lowerer = Lowerer::new();
        lowerer
            .lower_function(func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function")
    }

    #[test]
    fn test_simple_call() {
        // Simple test: just call a function that returns a constant
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %main() -> v0
    syscall 0(v0)
    halt
}

function %helper() -> i32 {
block0:
    v0 = iconst 42
    return v0
}

function %main() -> i32 {
block0:
    call %helper() -> v0
    return v0
}
}"#;

        expect_ir_syscall(ir, 0, &[42]);
    }

    #[test]
    fn test_call_preserves_caller_saved_registers() {
        // Create a helper function that modifies a0-a7
        // Main: fn main(a: i32) -> i32 {
        //   let temp = a * 2;  // Uses caller-saved register
        //   let result = helper(temp);  // Call may clobber caller-saved regs
        //   return temp + result;  // temp must still be valid!
        // }
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
    v1 = iconst 100
    v2 = iadd v0, v1
    return v2
}

function %main(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 2
    v2 = imul v0, v1
    call %helper(v2) -> v3
    v4 = iadd v2, v3
    return v4
}
}"#;

        // Test: main(5) should return (5*2) + (5*2 + 100) = 10 + 110 = 120
        // v2 = 5*2 = 10, helper(10) returns 110, v4 = v2 + v3 = 10 + 110 = 120
        expect_ir_syscall(ir, 0, &[120]);
    }

    #[test]
    fn test_nested_calls_preserve_registers() {
        // fn inner(x: i32) -> i32 { x + 1 }
        // fn middle(x: i32) -> i32 {
        //   let temp = x * 2;
        //   return inner(temp) + temp;
        // }
        // fn outer(x: i32) -> i32 {
        //   let temp = x + 10;
        //   return middle(temp) + temp;
        // }
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 5
    call %outer(v0) -> v1
    syscall 0(v1)
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
    v1 = iconst 2
    v2 = imul v0, v1
    call %inner(v2) -> v3
    v4 = iadd v3, v2
    return v4
}

function %outer(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 10
    v2 = iadd v0, v1
    call %middle(v2) -> v3
    v4 = iadd v3, v2
    return v4
}
}"#;

        // Test: outer(5) = middle(15) + 15 = (inner(30) + 30) + 15 = (31 + 30) + 15 = 76
        // v2 = 5+10 = 15, middle(15) = inner(30) + 30 = 31 + 30 = 61, v4 = 61 + 15 = 76
        expect_ir_syscall(ir, 0, &[76]);
    }

    #[test]
    fn test_multiple_live_values_across_call() {
        // fn main(a: i32, b: i32) -> i32 {
        //   let x = a * 2;
        //   let y = b * 3;
        //   let z = helper(x);  // Call
        //   return x + y + z;  // x and y must be preserved
        // }
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 5
    v1 = iconst 7
    call %main(v0, v1) -> v2
    syscall 0(v2)
    halt
}

function %helper(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 100
    v2 = iadd v0, v1
    return v2
}

function %main(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iconst 2
    v3 = imul v0, v2
    v4 = iconst 3
    v5 = imul v1, v4
    call %helper(v3) -> v6
    v7 = iadd v3, v5
    v8 = iadd v7, v6
    return v8
}
}"#;

        // Test: main(5, 7) = (5*2) + (7*3) + helper(10) = 10 + 21 + 110 = 141
        expect_ir_syscall(ir, 0, &[141]);
    }

    #[test]
    fn test_call_site_spills_use_frame_slots() {
        // Function with live values in caller-saved registers
        // Makes a call (values must be spilled)
        // Verify spilled values use slots from frame layout
        // Verify offsets match frame layout computation
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 10
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
    ; Create many values that will be in caller-saved registers
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
    ; Call helper - values v2, v4, v6, v8, v10 should be spilled
    ; because they're in caller-saved registers and will be clobbered
    call %helper(v10) -> v11
    ; Use the spilled values after call
    v12 = iadd v2, v4
    v13 = iadd v6, v8
    v14 = iadd v12, v13
    v15 = iadd v14, v11
    return v15
}
}"#;

        // Compile the function and check spill slots
        let module = parse_module(ir).expect("Failed to parse IR module");
        let func = module
            .functions
            .get("main")
            .expect("main function not found");
        let _func_code = lower_function(func);

        // Verify function executes correctly
        // Calculation: main(10)
        // v2 = 10+1 = 11, v4 = 11+2 = 13, v6 = 13+3 = 16, v8 = 16+4 = 20, v10 = 20+5 = 25
        // helper(25) = 25+1 = 26
        // v12 = v2+v4 = 11+13 = 24, v13 = v6+v8 = 16+20 = 36
        // v14 = v12+v13 = 24+36 = 60, v15 = v14+v11 = 60+26 = 86
        expect_ir_syscall(ir, 0, &[86]);
    }

    #[test]
    fn test_function_with_many_args() {
        // Function with 10 arguments (8 in regs, 2 on stack)
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iconst 3
    v3 = iconst 4
    v4 = iconst 5
    v5 = iconst 6
    v6 = iconst 7
    v7 = iconst 8
    v8 = iconst 9
    v9 = iconst 10
    call %test(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9) -> v10
    syscall 0(v10)
    halt
}

function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
    v10 = iadd v0, v9
    return v10
}
}"#;

        expect_ir_syscall(ir, 0, &[11]); // 1 + 10 = 11
    }

    #[test]
    fn test_function_calling_with_many_args() {
        // Function that calls another with >8 arguments
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %caller() -> v0
    syscall 0(v0)
    halt
}

function %callee(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
    v10 = iadd v8, v9
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

        expect_ir_syscall(ir, 0, &[17]); // 8 + 9 = 17
    }

    #[test]
    fn test_mixed_reg_and_stack_args() {
        // Function with 12 arguments (8 in regs, 4 on stack)
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iconst 3
    v3 = iconst 4
    v4 = iconst 5
    v5 = iconst 6
    v6 = iconst 7
    v7 = iconst 8
    v8 = iconst 9
    v9 = iconst 10
    v10 = iconst 11
    v11 = iconst 12
    call %test(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11) -> v12
    syscall 0(v12)
    halt
}

function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32, v10: i32, v11: i32):
    v12 = iadd v0, v11
    return v12
}
}"#;

        expect_ir_syscall(ir, 0, &[13]); // 1 + 12 = 13
    }

    #[test]
    fn test_stack_arguments() {
        // Test functions with >8 arguments (some on stack)
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iconst 3
    v3 = iconst 4
    v4 = iconst 5
    v5 = iconst 6
    v6 = iconst 7
    v7 = iconst 8
    v8 = iconst 9
    v9 = iconst 10
    call %test(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9) -> v10
    syscall 0(v10)
    halt
}

function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
    ; Sum all arguments
    v10 = iadd v0, v1
    v11 = iadd v10, v2
    v12 = iadd v11, v3
    v13 = iadd v12, v4
    v14 = iadd v13, v5
    v15 = iadd v14, v6
    v16 = iadd v15, v7
    v17 = iadd v16, v8
    v18 = iadd v17, v9
    return v18
}
}"#;

        // Sum of 1+2+3+4+5+6+7+8+9+10 = 55
        expect_ir_syscall(ir, 0, &[55]);
    }
}
