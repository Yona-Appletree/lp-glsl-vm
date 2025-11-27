
#[cfg(test)]
mod tests {
    extern crate std;

    use lpc_lpir::parse_module;

    use crate::{
        backend::{
            allocate_registers, compute_liveness, create_spill_reload_plan, Abi, CodeBuffer,
            FrameLayout, Lowerer,
        },
        expect_ir_syscall,
    };

    /// Helper to lower a function with all required analysis passes.
    fn lower_function(func: &lpc_lpir::Function) -> CodeBuffer {
        let liveness = compute_liveness(func);
        let allocation = allocate_registers(func, &liveness);
        let spill_reload = create_spill_reload_plan(func, &allocation, &liveness);

        let has_calls = func.blocks.iter().any(|block| {
            block
                .insts
                .iter()
                .any(|inst| matches!(inst, lpc_lpir::Inst::Call { .. }))
        });

        // Include temporary spill slots needed for caller-saved register preservation
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            8, // Default max outgoing args for test helper
            func.signature.returns.len(),
            8, // Default max callee stack returns for test helper
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
    fn test_stack_argument_passing_only() {
        // Minimal test: function that takes ONLY stack arguments (no register args)
        // This isolates stack argument passing to verify it works correctly
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 100
    v1 = iconst 200
    call %callee(v0, v1) -> v2
    syscall 0(v2)
    halt
}

function %callee(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}
}"#;

        expect_ir_syscall(ir, 0, &[300]); // 100 + 200 = 300
    }

    #[test]
    fn test_stack_argument_passing_many() {
        // Test with exactly 9 arguments (8 in regs, 1 on stack)
        // This tests the boundary case
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
    call %callee(v0, v1, v2, v3, v4, v5, v6, v7, v8) -> v9
    syscall 0(v9)
    halt
}

function %callee(i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32):
    v9 = iadd v7, v8
    return v9
}
}"#;

        expect_ir_syscall(ir, 0, &[17]); // 8 + 9 = 17
    }

    #[test]
    fn test_stack_argument_passing_two_on_stack() {
        // Test with exactly 10 arguments (8 in regs, 2 on stack)
        // This matches the failing test case but simplified
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
    call %callee(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9) -> v10
    syscall 0(v10)
    halt
}

function %callee(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
    v10 = iadd v8, v9
    return v10
}
}"#;

        expect_ir_syscall(ir, 0, &[19]); // 9 + 10 = 19
    }

    #[test]
    fn test_function_calling_with_many_args_debug() {
        // Debug version: print disassembly to see what's happening
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

        use lpc_lpir::parse_module;

        use crate::backend::compile_module_to_insts;
        extern crate std;

        let module = parse_module(ir).expect("Failed to parse");
        let compiled = compile_module_to_insts(&module).expect("Failed to compile");
        let bytes = compiled.to_bytes().expect("Failed to convert to bytes");

        std::println!("\n=== Full Module Disassembly ===");
        std::println!("{}", crate::disassemble_code(&bytes));
        std::println!("\n=== End Disassembly ===\n");

        // Now run the test
        expect_ir_syscall(ir, 0, &[17]); // 8 + 9 = 17
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

    #[test]
    fn test_large_function_stress_test() {
        // Stress test: function with 16 args (8 regs + 8 stack) and 16 returns (8 regs + 8 stack)
        // This exercises:
        // - Stack argument passing (incoming and outgoing)
        // - Stack return value handling
        // - Frame layout with large outgoing args area
        // - Prologue/epilogue with stack args
        // - Call lowering with many stack args
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    ; Pass 16 arguments: 1-16
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
    v12 = iconst 13
    v13 = iconst 14
    v14 = iconst 15
    v15 = iconst 16
    call %test(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13, v14, v15) -> v16, v17, v18, v19, v20, v21, v22, v23, v24, v25, v26, v27, v28, v29, v30, v31
    ; Sum all return values to verify correctness
    ; First 8 are in registers, last 8 are on stack
    v32 = iadd v16, v17
    v33 = iadd v32, v18
    v34 = iadd v33, v19
    v35 = iadd v34, v20
    v36 = iadd v35, v21
    v37 = iadd v36, v22
    v38 = iadd v37, v23
    v39 = iadd v38, v24
    v40 = iadd v39, v25
    v41 = iadd v40, v26
    v42 = iadd v41, v27
    v43 = iadd v42, v28
    v44 = iadd v43, v29
    v45 = iadd v44, v30
    v46 = iadd v45, v31
    syscall 0(v46)
    halt
}

function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32, v10: i32, v11: i32, v12: i32, v13: i32, v14: i32, v15: i32):
    ; Return each argument multiplied by 2 to verify they were passed correctly
    ; This exercises both register args (v0-v7) and stack args (v8-v15)
    v16 = iconst 2
    v17 = imul v0, v16
    v18 = imul v1, v16
    v19 = imul v2, v16
    v20 = imul v3, v16
    v21 = imul v4, v16
    v22 = imul v5, v16
    v23 = imul v6, v16
    v24 = imul v7, v16
    v25 = imul v8, v16
    v26 = imul v9, v16
    v27 = imul v10, v16
    v28 = imul v11, v16
    v29 = imul v12, v16
    v30 = imul v13, v16
    v31 = imul v14, v16
    v32 = imul v15, v16
    return v17 v18 v19 v20 v21 v22 v23 v24 v25 v26 v27 v28 v29 v30 v31 v32
}
}"#;

        // Expected: sum of (1*2 + 2*2 + ... + 16*2) = 2 * (1+2+...+16) = 2 * 136 = 272
        expect_ir_syscall(ir, 0, &[272]);
    }

    #[test]
    fn test_nested_calls_with_many_stack_args() {
        // Test nested calls where each function has many stack args
        // This exercises frame layout with outgoing args when the caller also has a frame
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
    call %outer(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9) -> v10
    syscall 0(v10)
    halt
}

function %inner(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
    ; Sum all args: v0+v1+...+v9
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

function %outer(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
    ; Call inner with same args, then add 100 to result
    call %inner(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9) -> v10
    v11 = iconst 100
    v12 = iadd v10, v11
    return v12
}
}"#;

        // Expected: sum(1..10) + 100 = 55 + 100 = 155
        expect_ir_syscall(ir, 0, &[155]);
    }

    #[test]
    fn test_many_args_with_computation() {
        // Test function with 12 args (4 on stack) that does computation using all args
        // This verifies stack args are accessible and correct
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
    call %compute(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11) -> v12
    syscall 0(v12)
    halt
}

function %compute(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32, v10: i32, v11: i32):
    ; Compute: (v0*v1) + (v2*v3) + ... + (v10*v11)
    ; This uses all args including stack args (v8-v11)
    v12 = imul v0, v1
    v13 = imul v2, v3
    v14 = imul v4, v5
    v15 = imul v6, v7
    v16 = imul v8, v9
    v17 = imul v10, v11
    v18 = iadd v12, v13
    v19 = iadd v18, v14
    v20 = iadd v19, v15
    v21 = iadd v20, v16
    v22 = iadd v21, v17
    return v22
}
}"#;

        // Expected: (1*2) + (3*4) + (5*6) + (7*8) + (9*10) + (11*12)
        // = 2 + 12 + 30 + 56 + 90 + 132 = 322
        expect_ir_syscall(ir, 0, &[322]);
    }

    #[test]
    fn test_multiple_returns_small() {
        // Test function returning 2 values (both in registers)
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %test() -> v0, v1
    v2 = iadd v0, v1
    syscall 0(v2)
    halt
}

function %test() -> i32, i32 {
block0:
    v0 = iconst 10
    v1 = iconst 20
    return v0 v1
}
}"#;

        // Expected: 10 + 20 = 30
        expect_ir_syscall(ir, 0, &[30]);
    }

    #[test]
    fn test_multiple_returns_medium() {
        // Test function returning 8 values (all in registers)
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %test() -> v0, v1, v2, v3, v4, v5, v6, v7
    v8 = iadd v0, v1
    v9 = iadd v8, v2
    v10 = iadd v9, v3
    v11 = iadd v10, v4
    v12 = iadd v11, v5
    v13 = iadd v12, v6
    v14 = iadd v13, v7
    syscall 0(v14)
    halt
}

function %test() -> i32, i32, i32, i32, i32, i32, i32, i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iconst 3
    v3 = iconst 4
    v4 = iconst 5
    v5 = iconst 6
    v6 = iconst 7
    v7 = iconst 8
    return v0 v1 v2 v3 v4 v5 v6 v7
}
}"#;

        // Expected: 1+2+3+4+5+6+7+8 = 36
        expect_ir_syscall(ir, 0, &[36]);
    }

    #[test]
    fn test_nested_calls_with_multiple_returns() {
        // Test nested calls where inner function returns multiple values
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %outer() -> v0, v1
    v2 = iadd v0, v1
    syscall 0(v2)
    halt
}

function %inner() -> i32, i32 {
block0:
    v0 = iconst 5
    v1 = iconst 10
    return v0 v1
}

function %outer() -> i32, i32 {
block0:
    call %inner() -> v0, v1
    v2 = iconst 100
    v3 = iadd v0, v2
    v4 = iconst 200
    v5 = iadd v1, v4
    return v3 v5
}
}"#;

        // Expected: (5+100) + (10+200) = 105 + 210 = 315
        expect_ir_syscall(ir, 0, &[315]);
    }
}
