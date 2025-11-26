//! Tests for stack arguments and return values (>8 args/returns).

use r5_ir::parse_module;
use r5_target_riscv32::{compile_module, generate_elf};
use r5_test_util::VmRunner;

/// Helper to test a module with stack arguments
fn test_module(module: &r5_ir::Module, entry_func_name: &str, args: &[i32], expected: i32) {
    let code = compile_module(module).expect("Compilation failed");
    let elf = generate_elf(&code);
    let mut runner = VmRunner::new(4 * 1024 * 1024);

    let result = runner.run(&elf, args);
    match result {
        Ok(r) => {
            assert_eq!(
                r.return_value,
                Some(expected),
                "Expected return value {} but got {:?}",
                expected,
                r.return_value
            );
        }
        Err(e) => {
            panic!("Test failed: {}", e);
        }
    }
}

#[test]
fn test_function_with_many_args() {
    // Function with 10 arguments (8 in regs, 2 on stack)
    let ir_module = r#"
module {
    function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
    block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
        v10 = iadd v0, v9
        return v10
    }
}"#;

    let module = parse_module(ir_module).expect("Failed to parse module");
    test_module(&module, "test", &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 11); // 1 + 10 = 11
}

#[test]
fn test_function_calling_with_many_args() {
    // Function that calls another with >8 arguments
    let ir_module = r#"
module {
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

    let mut module = parse_module(ir_module).expect("Failed to parse module");
    module.set_entry_function("caller".to_string());
    test_module(&module, "caller", &[], 17); // 8 + 9 = 17
}

#[test]
fn test_mixed_reg_and_stack_args() {
    // Function with 12 arguments (8 in regs, 4 on stack)
    let ir_module = r#"
module {
    function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
    block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32, v10: i32, v11: i32):
        v12 = iadd v0, v11
        return v12
    }
}"#;

    let module = parse_module(ir_module).expect("Failed to parse module");
    test_module(&module, "test", &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], 13); // 1 + 12 = 13
}

