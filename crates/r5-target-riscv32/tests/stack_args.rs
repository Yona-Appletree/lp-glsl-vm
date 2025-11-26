//! Tests for stack arguments and return values (>8 args/returns).

use r5_target_riscv32::expect_ir_syscall;

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

