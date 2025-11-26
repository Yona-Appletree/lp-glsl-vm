//! Tests for caller-saved register handling.

use r5_builder::FunctionBuilder;
use r5_ir::{parse_module, Module, Signature, Type};
use r5_target_riscv32::{compile_module, generate_elf};
use r5_test_util::VmRunner;

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
    use riscv32_encoder::disassemble_code;
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
