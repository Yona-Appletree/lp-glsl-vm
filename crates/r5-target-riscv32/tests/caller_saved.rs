//! Tests for caller-saved register handling.

use r5_builder::FunctionBuilder;
use r5_ir::{parse_module, Module, Signature, Type};
use r5_target_riscv32::{compile_module, generate_elf, Lowerer};
use r5_test_util::VmRunner;
use riscv32_encoder::{disassemble_code, disassemble_instruction};

/// Helper to test a module with multiple functions
fn test_module(module: &Module, entry_func_name: &str, args: &[i32], expected: i32) {
    let mut test_module = module.clone();

    // Create bootstrap wrapper
    let sig = Signature::new(Vec::new(), vec![Type::I32]);
    let mut builder = FunctionBuilder::new(sig);
    let block = builder.create_block();

    // Create argument values
    let mut arg_values = Vec::new();
    for &arg in args.iter().take(8) {
        let arg_val = builder.new_value();
        {
            let mut bb = builder.block_builder(block);
            bb.iconst(arg_val, arg as i64);
        }
        arg_values.push(arg_val);
    }

    // Call main function
    let result_val = builder.new_value();
    {
        let mut bb = builder.block_builder(block);
        bb.call(entry_func_name.to_string(), arg_values, vec![result_val]);
    }

    // Call syscall 0 with result
    {
        let mut bb = builder.block_builder(block);
        bb.syscall(0, vec![result_val]);
    }

    // Halt
    {
        let mut bb = builder.block_builder(block);
        bb.halt();
    }

    let bootstrap = builder.finish();
    test_module.add_function("bootstrap".to_string(), bootstrap);
    test_module.set_entry_function("bootstrap".to_string());

    // Compile and run
    let code = compile_module(&test_module);
    let elf = generate_elf(&code);
    let mut runner = VmRunner::new(4 * 1024 * 1024);

    // Print debug info
    eprintln!("\n=== Module ===");
    eprintln!("{}", test_module);
    eprintln!("\n=== Compiled Code ===");
    eprintln!("{}", disassemble_code(&code));

    let result = runner.run(&elf, args);
    match result {
        Ok(r) => {
            eprintln!("\n=== Result ===");
            eprintln!("Return value: {:?}", r.return_value);
            assert_eq!(
                r.return_value,
                Some(expected),
                "Expected return value {} but got {:?}",
                expected,
                r.return_value
            );
        }
        Err(e) => {
            eprintln!("\n=== Error ===");
            eprintln!("{}", e);
            panic!("Test failed: {}", e);
        }
    }
}

#[test]
fn test_simple_call() {
    // Simple test: just call a function that returns a constant
    let ir_module = r#"
module {
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

    let module = parse_module(ir_module).expect("Failed to parse IR module");
    test_module(&module, "main", &[], 42);
}

#[test]
fn test_call_preserves_caller_saved_registers() {
    // Create a helper function that modifies a0-a7
    // Main: fn main(a: i32) -> i32 {
    //   let temp = a * 2;  // Uses caller-saved register
    //   let result = helper(temp);  // Call may clobber caller-saved regs
    //   return temp + result;  // temp must still be valid!
    // }
    let ir_module = r#"
module {
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

    let module = parse_module(ir_module).expect("Failed to parse IR module");

    // Test: main(5) should return (5*2) + (5*2 + 100) = 10 + 110 = 120
    // This will fail if temp is clobbered by the call
    test_module(&module, "main", &[5], 120);
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
    let ir_module = r#"
module {
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

    let module = parse_module(ir_module).expect("Failed to parse IR module");

    // Test: outer(5) = middle(15) + 15 = (inner(30) + 30) + 15 = (31 + 30) + 15 = 76
    test_module(&module, "outer", &[5], 76);
}

#[test]
fn test_multiple_live_values_across_call() {
    // fn main(a: i32, b: i32) -> i32 {
    //   let x = a * 2;
    //   let y = b * 3;
    //   let z = helper(x);  // Call
    //   return x + y + z;  // x and y must be preserved
    // }
    let ir_module = r#"
module {
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

    let module = parse_module(ir_module).expect("Failed to parse IR module");

    // Test: main(5, 7) = (5*2) + (7*3) + helper(10) = 10 + 21 + 110 = 141
    test_module(&module, "main", &[5, 7], 141);
}

/// Helper to count SP adjustments in prologue (addi sp, sp, -N instructions)
fn count_sp_adjustments_in_prologue(code: &[u8], function_start: usize) -> usize {
    let disasm = disassemble_code(&code[function_start..]);
    let mut count = 0;
    for line in disasm.lines() {
        if line.contains("addi sp, sp,") && line.contains('-') {
            count += 1;
        }
    }
    count
}

/// Helper to get function prologue instructions
fn get_prologue_instructions(
    code: &[u8],
    function_start: usize,
    max_instructions: usize,
) -> Vec<String> {
    let mut instructions = Vec::new();
    let mut offset = function_start;
    let mut count = 0;

    while offset + 4 <= code.len() && count < max_instructions {
        let inst_bytes = [
            code[offset],
            code[offset + 1],
            code[offset + 2],
            code[offset + 3],
        ];
        let inst = u32::from_le_bytes(inst_bytes);

        // Disassemble instruction
        let disasm = disassemble_instruction(inst);
        instructions.push(disasm);

        offset += 4;
        count += 1;
    }

    instructions
}

/// Helper to verify epilogue order
fn verify_epilogue_order(code: &[u8], function_end: usize, max_instructions: usize) -> Vec<String> {
    // Get last max_instructions instructions before function end
    let start_offset = function_end.saturating_sub(max_instructions * 4);
    get_prologue_instructions(code, start_offset, max_instructions)
}

#[test]
fn test_prologue_adjusts_sp_once() {
    // Create a function with calls and callee-saved registers
    // This will force the allocator to use callee-saved registers
    // The function has enough register pressure to require spills and callee-saved regs
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
    let mut lowerer = Lowerer::new();
    lowerer.set_module(module.clone());
    let func_code = lowerer.lower_function(func);

    // Get prologue instructions for inspection
    let prologue_insts = get_prologue_instructions(func_code.as_bytes(), 0, 20);

    // Count SP adjustments in prologue (addi sp, sp, -N instructions)
    let sp_adjustments = count_sp_adjustments_in_prologue(func_code.as_bytes(), 0);

    eprintln!("\n=== Function Code ===");
    eprintln!("{}", disassemble_code(func_code.as_bytes()));
    eprintln!("\n=== Prologue Instructions (first 20) ===");
    for (i, inst) in prologue_insts.iter().enumerate() {
        eprintln!("  {}: {}", i, inst);
    }
    eprintln!("\n=== SP Adjustments in Prologue ===");
    eprintln!("Count: {}", sp_adjustments);
    eprintln!("Expected: 1");

    // Expected: SP should be adjusted exactly once in prologue
    // The prologue should:
    // 1. Save RA (if needed)
    // 2. Save callee-saved registers (if any)
    // 3. Adjust SP ONCE for the entire frame
    //
    // Currently failing: SP is adjusted multiple times (once for RA, once for callee-saved, etc.)
    assert_eq!(
        sp_adjustments,
        1,
        "Prologue should adjust SP exactly once, but found {} adjustments.\nPrologue \
         instructions:\n{}\nFull function disassembly:\n{}",
        sp_adjustments,
        prologue_insts.join("\n"),
        disassemble_code(func_code.as_bytes())
    );
}

#[test]
fn test_epilogue_restores_correct_order() {
    // Function that uses callee-saved registers and makes calls
    // Verify epilogue order: restore callee-saved → restore RA → adjust SP
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
    let mut lowerer = Lowerer::new();
    lowerer.set_module(module.clone());
    let func_code = lowerer.lower_function(func);

    // Get epilogue instructions (last few instructions before return)
    let bytes = func_code.as_bytes();
    let epilogue_start = bytes.len().saturating_sub(20 * 4); // Last 20 instructions
    let epilogue_instructions = get_prologue_instructions(bytes, epilogue_start, 20);

    eprintln!("\n=== Function Code ===");
    eprintln!("{}", disassemble_code(bytes));
    eprintln!("\n=== Epilogue Instructions (last 10) ===");
    for (i, inst) in epilogue_instructions.iter().rev().take(10).enumerate() {
        eprintln!("  {}: {}", i, inst);
    }

    // Verify epilogue order: restore callee-saved (if any) → restore RA → adjust SP
    // For this function, we should see: lw ra, ... → addi sp, sp, ...
    let ra_restore_pos = epilogue_instructions
        .iter()
        .position(|s| s.contains("lw ra,"));
    let sp_adjust_pos = epilogue_instructions
        .iter()
        .position(|s| s.contains("addi sp, sp,") && !s.contains('-'));

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
