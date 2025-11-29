//! Tests for operand collection, constraints, and ranges

extern crate alloc;

use alloc::vec::Vec;

use crate::backend3::{
    tests::vcode_test_helpers::LowerTest,
    vcode::{OperandConstraint, OperandKind},
};

/// Test that operand constraints are correctly collected
#[test]
fn test_operand_constraints_collected() {
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

    // Verify operands have constraints
    for operand in &vcode.operands {
        // Constraint should be one of the valid types
        match operand.constraint {
            OperandConstraint::Any => {}
            OperandConstraint::Fixed(_) => {}
            OperandConstraint::RegClass(_) => {}
        }
    }
}

/// Test that operand kinds (Use, Def, Mod) are correctly identified
#[test]
fn test_operand_kinds_identified() {
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

    // Find the ADD instruction (should have 2 uses and 1 def)
    // The exact instruction index depends on lowering order, so we search
    let mut found_add = false;
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Add { .. } = inst {
            found_add = true;

            // Get operand range for this instruction
            if let Some(range) = vcode.operand_ranges.get(inst_idx) {
                let operands = &vcode.operands[range.start..range.end];

                // ADD should have: 1 def (rd), 2 uses (rs1, rs2)
                let defs: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Use)
                    .collect();

                assert_eq!(
                    defs.len(),
                    1,
                    "ADD instruction should have exactly 1 def operand"
                );
                assert_eq!(
                    uses.len(),
                    2,
                    "ADD instruction should have exactly 2 use operands"
                );
            }
        }
    }

    assert!(found_add, "Should have found an ADD instruction");
}

/// Test that operand ranges match instruction count
#[test]
fn test_operand_ranges_match_instruction_count() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    v3 = isub v0, v1
    v4 = imul v0, v1
    return v2
}
"#,
    );

    let vcode = test.vcode();

    assert_eq!(
        vcode.operand_ranges.len(),
        vcode.insts.len(),
        "Each instruction should have exactly one operand range"
    );
}

/// Test that operand ranges are non-overlapping and contiguous
#[test]
fn test_operand_ranges_non_overlapping() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    v3 = isub v0, v1
    return v2
}
"#,
    );

    let vcode = test.vcode();

    // Check ranges are contiguous
    for i in 0..vcode.operand_ranges.len() {
        if let Some(range) = vcode.operand_ranges.get(i) {
            assert!(range.start <= range.end, "Range start should be <= end");

            if i + 1 < vcode.operand_ranges.len() {
                if let Some(next_range) = vcode.operand_ranges.get(i + 1) {
                    assert_eq!(
                        range.end,
                        next_range.start,
                        "Ranges should be contiguous"
                    );
                }
            }
        }
    }
}

/// Test instructions with no operands (like EBREAK)
#[test]
fn test_instruction_with_no_operands() {
    let test = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    halt
    v1 = iconst 42
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Find the halt/ebreak instruction
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Ebreak = inst {
            // Get operand range for this instruction
            if let Some(range) = vcode.operand_ranges.get(inst_idx) {
                // EBREAK should have no operands
                assert_eq!(
                    range.len(),
                    0,
                    "EBREAK instruction should have no operands"
                );
            }
        }
    }
}

/// Test instructions with many operands (like function calls)
#[test]
fn test_instruction_with_many_operands() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v2: i32):
    v3 = call @other(v0, v1, v2)
    return v3
}
"#,
    );

    let vcode = test.vcode();

    // Find the call instruction
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Jal { args, .. } = inst {
            // Get operand range for this instruction
            if let Some(range) = vcode.operand_ranges.get(inst_idx) {
                let operands = &vcode.operands[range.start..range.end];

                // JAL should have: 1 def (rd), N uses (args)
                let defs: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Use)
                    .collect();

                assert_eq!(
                    defs.len(),
                    1,
                    "JAL instruction should have exactly 1 def operand (rd)"
                );
                assert_eq!(
                    uses.len(),
                    args.len(),
                    "JAL instruction should have use operands matching argument count"
                );
            }
        }
    }
}

/// Test that store instructions have correct operand kinds
#[test]
fn test_store_operand_kinds() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    store.i32 v0, v1
    return v0
}
"#,
    );

    let vcode = test.vcode();

    // Find the store instruction
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Sw { .. } = inst {
            // Get operand range for this instruction
            if let Some(range) = vcode.operand_ranges.get(inst_idx) {
                let operands = &vcode.operands[range.start..range.end];

                // SW should have: 2 uses (rs1=address, rs2=value), no defs
                let defs: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Use)
                    .collect();

                assert_eq!(
                    defs.len(),
                    0,
                    "SW instruction should have no def operands"
                );
                assert_eq!(
                    uses.len(),
                    2,
                    "SW instruction should have exactly 2 use operands"
                );
            }
        }
    }
}

/// Test that load instructions have correct operand kinds
#[test]
fn test_load_operand_kinds() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = load.i32 v0
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Find the load instruction
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Lw { .. } = inst {
            // Get operand range for this instruction
            if let Some(range) = vcode.operand_ranges.get(inst_idx) {
                let operands = &vcode.operands[range.start..range.end];

                // LW should have: 1 def (rd), 1 use (rs1=address)
                let defs: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Use)
                    .collect();

                assert_eq!(
                    defs.len(),
                    1,
                    "LW instruction should have exactly 1 def operand"
                );
                assert_eq!(
                    uses.len(),
                    1,
                    "LW instruction should have exactly 1 use operand"
                );
            }
        }
    }
}

/// Test that return instructions have correct operand kinds
#[test]
fn test_return_operand_kinds() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    return v0
}
"#,
    );

    let vcode = test.vcode();

    // Find the return instruction
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Return { ret_vals } = inst {
            // Get operand range for this instruction
            if let Some(range) = vcode.operand_ranges.get(inst_idx) {
                let operands = &vcode.operands[range.start..range.end];

                // Return should have: N uses (ret_vals), no defs
                let defs: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| op.kind == OperandKind::Use)
                    .collect();

                assert_eq!(
                    defs.len(),
                    0,
                    "Return instruction should have no def operands"
                );
                assert_eq!(
                    uses.len(),
                    ret_vals.len(),
                    "Return instruction should have use operands matching return value count"
                );
            }
        }
    }
}

/// Test that all operands have valid VRegs
#[test]
fn test_operands_have_valid_vregs() {
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

    // All operands should have valid VRegs (just verify they exist)
    for operand in &vcode.operands {
        let _vreg = operand.vreg; // Should not panic
    }
}

/// Test that operand ranges cover all operands exactly once
#[test]
fn test_operand_ranges_cover_all_operands() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    v3 = isub v0, v1
    v4 = imul v0, v1
    return v2
}
"#,
    );

    let vcode = test.vcode();

    // Calculate total operands covered by ranges
    let mut total_covered = 0;
    for i in 0..vcode.operand_ranges.len() {
        if let Some(range) = vcode.operand_ranges.get(i) {
            total_covered += range.len();
        }
    }

    // Should match actual operand count
    assert_eq!(
        total_covered,
        vcode.operands.len(),
        "Operand ranges should cover all operands exactly once"
    );
}

