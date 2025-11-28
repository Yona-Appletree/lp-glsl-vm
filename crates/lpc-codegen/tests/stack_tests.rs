//! Comprehensive tests for stack operations, frame management, and call stack.
//! 
//! DISABLED: Uses old backend functions
//! TODO: Re-enable when backend3 is implemented

/*
use lpc_codegen::{expect_ir_ok, expect_ir_syscall, Gpr};

#[test]
fn test_basic_stack_operations() {
    // Test function with spill slots (many live values)
    // This forces values to be spilled to the stack
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
    ; Create many values to force spills
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    v3 = iconst 3
    v4 = iadd v2, v3
    v5 = iconst 4
    v6 = iadd v4, v5
    v7 = iconst 5
    v8 = iadd v6, v7
    v9 = iconst 6
    v10 = iadd v8, v9
    v11 = iconst 7
    v12 = iadd v10, v11
    v13 = iconst 8
    v14 = iadd v12, v13
    v15 = iconst 9
    v16 = iadd v14, v15
    v17 = iconst 10
    v18 = iadd v16, v17
    return v18
}
}"#;

    // 1+2+3+4+5+6+7+8+9+10 = 55
    expect_ir_syscall(ir, 0, &[55]);
}

#[test]
fn test_stack_writes_and_reads() {
    // Test that values written to stack can be read back correctly
    // Function with many live values across a call forces stack usage
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
    v1 = iconst 100
    v2 = iadd v0, v1
    return v2
}

function %main() -> i32 {
block0:
    ; Create values that will be spilled across call
    v0 = iconst 10
    v1 = iconst 20
    v2 = iconst 30
    v3 = iconst 40
    v4 = iconst 50
    ; Call helper - values above should be spilled
    call %helper(v4) -> v5
    ; Read back spilled values and use them
    v6 = iadd v0, v1
    v7 = iadd v2, v3
    v8 = iadd v6, v7
    v9 = iadd v8, v5
    return v9
}
}"#;

    // v0=10, v1=20, v2=30, v3=40, v4=50
    // helper(50) = 150
    // v6 = 10+20 = 30, v7 = 30+40 = 70, v8 = 30+70 = 100, v9 = 100+150 = 250
    expect_ir_syscall(ir, 0, &[250]);
}

#[test]
fn test_sp_initialization() {
    // Test that SP is properly initialized before function execution
    // Simple function that should work if SP is valid
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

    let emu = expect_ir_ok(ir);
    // Verify SP is initialized (not zero)
    let sp = emu.get_register(Gpr::Sp);
    assert_ne!(sp, 0, "SP should be initialized to non-zero value");
    // SP should be in valid memory region (cast to u32 for comparison)
    let sp_u32 = sp as u32;
    assert!(
        sp_u32 >= 0x80001000,
        "SP should be initialized to valid memory region"
    );
}

#[test]
fn test_prologue_sp_adjustment() {
    // Test that prologue correctly adjusts SP for frame
    // Function with frame (has calls and/or spills)
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
fn test_epilogue_sp_restoration() {
    // Test that epilogue correctly restores SP
    // Nested calls to verify SP is restored at each level
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

#[test]
fn test_nested_function_calls() {
    // Test nested calls with stack frames at each level
    let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    call %level1() -> v0
    syscall 0(v0)
    halt
}

function %level3(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iadd v0, v1
    return v2
}

function %level2(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 10
    v2 = iadd v0, v1
    call %level3(v2) -> v3
    v4 = iconst 5
    v5 = iadd v3, v4
    return v5
}

function %level1() -> i32 {
block0:
    v0 = iconst 100
    call %level2(v0) -> v1
    v2 = iconst 20
    v3 = iadd v1, v2
    return v3
}
}"#;

    // level1: v0=100, level2(100): v2=110, level3(110)=111, v5=116, v3=136
    expect_ir_syscall(ir, 0, &[136]);
}

#[test]
fn test_caller_saved_preservation() {
    // Test that caller-saved registers are properly preserved across calls
    // Values in caller-saved regs must be spilled before call
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
    v1 = iconst 1000
    v2 = iadd v0, v1
    return v2
}

function %main() -> i32 {
block0:
    ; Create values in caller-saved registers
    v0 = iconst 10
    v1 = iconst 20
    v2 = iconst 30
    v3 = iconst 40
    ; Call helper - v0, v1, v2, v3 should be spilled
    call %helper(v3) -> v4
    ; Use spilled values after call
    v5 = iadd v0, v1
    v6 = iadd v2, v3
    v7 = iadd v5, v6
    v8 = iadd v7, v4
    return v8
}
}"#;

    // v0=10, v1=20, v2=30, v3=40
    // helper(40) = 1040
    // v5 = 10+20 = 30, v6 = 30+40 = 70, v7 = 30+70 = 100, v8 = 100+1040 = 1140
    expect_ir_syscall(ir, 0, &[1140]);
}

#[test]
fn test_callee_saved_restoration() {
    // Test that callee-saved registers are properly saved and restored
    // Function that uses many registers (forces callee-saved usage)
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
    ; Create many values to force callee-saved register usage
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    v3 = iconst 3
    v4 = iadd v2, v3
    v5 = iconst 4
    v6 = iadd v4, v5
    v7 = iconst 5
    v8 = iadd v6, v7
    v9 = iconst 6
    v10 = iadd v8, v9
    v11 = iconst 7
    v12 = iadd v10, v11
    call %helper(v12) -> v13
    ; Use values after call - callee-saved should be restored
    v14 = iconst 100
    v15 = iadd v13, v14
    return v15
}
}"#;

    // v0=1, v1=2, v2=3, v3=3, v4=6, v5=4, v6=10, v7=5, v8=15, v9=6, v10=21, v11=7, v12=28
    // helper(28) = 29, v14=100, v15=129
    expect_ir_syscall(ir, 0, &[129]);
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
fn test_large_frame() {
    // Test function with large frame (>2047 bytes may need multi-instruction SP adjustment)
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
    ; Create many values to force large frame
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    v3 = iconst 3
    v4 = iadd v2, v3
    v5 = iconst 4
    v6 = iadd v4, v5
    v7 = iconst 5
    v8 = iadd v6, v7
    v9 = iconst 6
    v10 = iadd v8, v9
    v11 = iconst 7
    v12 = iadd v10, v11
    v13 = iconst 8
    v14 = iadd v12, v13
    v15 = iconst 9
    v16 = iadd v14, v15
    v17 = iconst 10
    v18 = iadd v16, v17
    v19 = iconst 11
    v20 = iadd v18, v19
    v21 = iconst 12
    v22 = iadd v20, v21
    v23 = iconst 13
    v24 = iadd v22, v23
    v25 = iconst 14
    v26 = iadd v24, v25
    v27 = iconst 15
    v28 = iadd v26, v27
    v29 = iconst 16
    v30 = iadd v28, v29
    v31 = iconst 17
    v32 = iadd v30, v31
    v33 = iconst 18
    v34 = iadd v32, v33
    v35 = iconst 19
    v36 = iadd v34, v35
    v37 = iconst 20
    v38 = iadd v36, v37
    return v38
}
}"#;

    // Sum 1+2+...+20 = 210
    expect_ir_syscall(ir, 0, &[210]);
}

#[test]
fn test_stack_overflow_protection() {
    // Test that stack overflow is detected (use very small RAM)
    // This test may not always trigger an error, but exercises stack bounds
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
    ; Create many values to use stack
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    v3 = iconst 3
    v4 = iadd v2, v3
    v5 = iconst 4
    v6 = iadd v4, v5
    v7 = iconst 5
    v8 = iadd v6, v7
    v9 = iconst 6
    v10 = iadd v8, v9
    v11 = iconst 7
    v12 = iadd v10, v11
    v13 = iconst 8
    v14 = iadd v12, v13
    v15 = iconst 9
    v16 = iadd v14, v15
    v17 = iconst 10
    v18 = iadd v16, v17
    return v18
}
}"#;

    // With very small RAM, this might cause memory errors
    // But with normal RAM, should work fine
    expect_ir_syscall(ir, 0, &[55]);
}

#[test]
fn test_complex_nested_calls() {
    // Test complex scenario with nested calls and many live values
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
    ; Create local values
    v1 = iconst 10
    v2 = iadd v0, v1
    v3 = iconst 20
    v4 = iadd v2, v3
    call %inner(v4) -> v5
    v6 = iconst 5
    v7 = iadd v5, v6
    return v7
}

function %outer() -> i32 {
block0:
    v0 = iconst 100
    v1 = iconst 200
    v2 = iadd v0, v1
    call %middle(v2) -> v3
    v4 = iconst 50
    v5 = iadd v3, v4
    return v5
}
}"#;

    // outer: v0=100, v1=200, v2=300
    // middle(300): v2=310, v4=330, inner(330)=331, v7=336
    // outer: v5=386
    expect_ir_syscall(ir, 0, &[386]);
}

#[test]
fn test_many_spill_slots() {
    // Test function with many spill slots
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
    ; Create many values that will need spill slots
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
    ; Call helper - many values will be spilled
    call %helper(v9) -> v10
    ; Use all spilled values
    v11 = iadd v0, v1
    v12 = iadd v2, v3
    v13 = iadd v4, v5
    v14 = iadd v6, v7
    v15 = iadd v8, v9
    v16 = iadd v11, v12
    v17 = iadd v13, v14
    v18 = iadd v16, v17
    v19 = iadd v18, v15
    v20 = iadd v19, v10
    return v20
}
}"#;

    // v0=1, v1=2, v2=3, v3=4, v4=5, v5=6, v6=7, v7=8, v8=9, v9=10
    // helper(10) = 11
    // v11=3, v12=7, v13=11, v14=15, v15=19, v16=10, v17=26, v18=36, v19=55, v20=66
    expect_ir_syscall(ir, 0, &[66]);
}

#[test]
fn test_stack_alignment() {
    // Test that stack operations maintain proper alignment
    // Function with frame should maintain 16-byte alignment
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
    ; Create values to force frame allocation
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    v3 = iconst 3
    v4 = iadd v2, v3
    v5 = iconst 4
    v6 = iadd v4, v5
    return v6
}
}"#;

    // 1+2+3+4 = 10
    expect_ir_syscall(ir, 0, &[10]);
}
*/
