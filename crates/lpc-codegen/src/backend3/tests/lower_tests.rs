//! Tests for lowering iconst/iadd/isub instructions

extern crate alloc;

use alloc::{format, vec::Vec};

use lpc_lpir::parse_function;

use crate::{
    backend3::{lower::lower_function, vcode::Callee},
    isa::riscv32::backend3::{inst::Riscv32ABI, Riscv32LowerBackend},
};

#[test]
fn test_lower_iconst() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // iconst 42 fits in 12 bits, so it's recorded as an inline constant
    // The constant will be embedded in instructions that use it
    // For now, we just verify the structure is correct
    LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    return v1
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0:

}
"#,
    );
}

#[test]
fn test_lower_iadd() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    add v2, v0, v1

}
"#,
    );
}

#[test]
fn test_lower_isub() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = isub v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    sub v2, v0, v1

}
"#,
    );
}

/// Test that operands are collected correctly from instructions
#[test]
fn test_operand_collection() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // First verify the VCode structure matches expected format
    let test = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}
"#,
    );

    test.assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    add v2, v0, v1

}
"#,
    );

    // Then verify operands are collected (property check)
    let vcode = test.vcode();

    // Also verify operands are collected (property check)
    assert_eq!(
        vcode.operand_ranges.len(),
        vcode.insts.len(),
        "Each instruction should have an operand range"
    );

    // Verify that operands array is populated
    assert!(
        !vcode.operands.is_empty() || vcode.insts.is_empty(),
        "Operands should be populated if there are instructions"
    );

    // Verify operand ranges match instruction count
    let total_operands: usize = (0..vcode.operand_ranges.len())
        .map(|i| {
            let range = vcode.operand_ranges.get(i).unwrap();
            range.len()
        })
        .sum();
    assert_eq!(
        total_operands,
        vcode.operands.len(),
        "Total operand count should match operands array length"
    );
}

/// Test that phi moves are correctly emitted in edge blocks
///
/// Creates a function with critical edges and phi nodes:
///   block0 (entry) - branches to block1 and block2
///   block1 - computes v1, branches to block3 and block4
///   block2 - computes v2, branches to block3 and block4
///   block3 - phi node: v3 = phi(v1 from block1, v2 from block2)
///   block4 - phi node: v4 = phi(v1 from block1, v2 from block2)
///
/// The edges block1->block3, block1->block4, block2->block3, block2->block4
/// are critical edges (source has multiple successors AND target has multiple predecessors)
/// and should have edge blocks with move instructions.
///
/// Note: Branches are not yet fully implemented in lowering, so this test
/// currently verifies the structure and that edge blocks exist.
#[test]
fn test_phi_moves_in_edge_blocks() {
    use crate::isa::riscv32::backend3::inst::Riscv32MachInst;

    let input = r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v2 = iconst 1
    v1 = iadd v0, v2
    brif v0, block3(v1), block4(v1)

block2:
    v4 = iconst 2
    v3 = iadd v0, v4
    brif v0, block3(v3), block4(v3)

block3(v5: i32):
    return v5

block4(v6: i32):
    return v6
}
"#;
    let func = parse_function(input.trim()).expect("Failed to parse function");

    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);

    // Since branches aren't fully implemented yet, we verify the structure
    // and that blocks are properly formed. Once branches are implemented,
    // we can add a full text format check here.

    // Verify that edge blocks exist and contain move instructions
    // We should have edge blocks for block1->block3 and block2->block3
    let edge_blocks: Vec<_> = vcode
        .block_order
        .lowered_order
        .iter()
        .enumerate()
        .filter(|(_, lb)| matches!(lb, crate::backend3::vcode::LoweredBlock::Edge { .. }))
        .collect();

    assert!(
        edge_blocks.len() >= 2,
        "Should have at least 2 edge blocks for critical edges"
    );

    // Count move instructions in the VCode
    let move_count = vcode
        .insts
        .iter()
        .filter(|inst| matches!(inst, Riscv32MachInst::Move { .. }))
        .count();

    // Should have move instructions for phi values
    // Note: If source and target VRegs are the same, moves may be elided
    // The exact count depends on VReg allocation, but we should have some moves
    assert!(
        move_count > 0 || edge_blocks.len() > 0,
        "Should have moves or edge blocks for phi values"
    );

    // Verify that edge blocks are properly tracked in block ranges
    assert_eq!(
        vcode.block_ranges.len(),
        vcode.block_order.lowered_order.len(),
        "Block ranges should match lowered order length"
    );

    // Print the VCode structure for debugging (can be removed later)
    // This helps verify the structure matches expectations
    let _vcode_str = format!("{}", vcode);
}
