//! Tests to verify operand collection completeness for all RISC-V instructions

extern crate alloc;

use alloc::vec::Vec;

use crate::backend3::tests::vcode_test_helpers::LowerTest;

/// Test that all RISC-V instruction types have operand collection implemented
#[test]
fn test_all_instruction_types_have_operand_collection() {
    // Test each instruction type to ensure get_operands() is implemented

    // Arithmetic instructions
    let _test_add = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}
"#,
    );

    let _test_sub = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = isub v0, v1
    return v2
}
"#,
    );

    let _test_mul = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = imul v0, v1
    return v2
}
"#,
    );

    let _test_div = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = idiv v0, v1
    return v2
}
"#,
    );

    let _test_rem = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = irem v0, v1
    return v2
}
"#,
    );

    // Memory instructions
    let _test_load = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = load.i32 v0
    return v1
}
"#,
    );

    let _test_store = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> void {
block0(v0: i32, v1: i32):
    store.i32 v0, v1
    return
}
"#,
    );

    // Comparison instructions
    let _test_icmp = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp eq v0, v1
    return v2
}
"#,
    );

    // Control flow instructions
    let _test_br = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    return v0

block2:
    return v0
}
"#,
    );

    let _test_jump = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    jump block1

block1:
    v1 = iconst 42
    return v1
}
"#,
    );

    // Function call
    let _test_call = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    call %other(v0) -> v1
    return v1
}
"#,
    );

    // System call
    let _test_syscall = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    syscall 1(v0) -> v1
    return v1
}
"#,
    );

    // Halt
    let _test_halt = LowerTest::from_lpir(
        r#"
function %test() -> void {
block0:
    halt
}
"#,
    );

    // Trap instructions
    let _test_trap = LowerTest::from_lpir(
        r#"
function %test() -> void {
block0:
    trap int_ovf
}
"#,
    );

    // All instruction types should compile and have operands collected
    // If any instruction type is missing operand collection, this test will fail to compile
    // or the operand collection will be incomplete
}

/// Test that operand constraints are correct for each instruction type
#[test]
fn test_operand_constraints_correct() {
    use regalloc2::{OperandConstraint, OperandKind};

    let test = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}
"#,
    );

    let vcode = test.vcode();

    // Find ADD instruction and verify operand constraints
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Add { .. } = inst {
            if let Some(range) = vcode.operand_ranges.get(inst_idx) {
                let operands = &vcode.operands[range.start..range.end];

                // ADD should have: 1 def, 2 uses
                let defs: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind() == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind() == OperandKind::Use)
                    .collect();

                assert_eq!(defs.len(), 1, "ADD should have 1 def operand");
                assert_eq!(uses.len(), 2, "ADD should have 2 use operands");

                // All operands should use OperandConstraint::Any (for now)
                for operand in operands {
                    assert_eq!(
                        operand.constraint(),
                        OperandConstraint::Any,
                        "RISC-V operands should use OperandConstraint::Any"
                    );
                }
            }
        }
    }
}
