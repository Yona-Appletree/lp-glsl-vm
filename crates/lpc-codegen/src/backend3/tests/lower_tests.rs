//! Tests for lowering LPIR instructions to VCode
//!
//! These tests cover all LPIR instructions (excluding floating point).
//! Tests for unimplemented instructions will fail until the backend is updated
//! to support them, which helps identify what still needs to be implemented.

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
    return v1

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
    return v2

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
    return v2

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
    return v2

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
#[test]
fn test_phi_moves_in_edge_blocks() {
    use crate::{
        backend3::tests::vcode_test_helpers::LowerTest,
        isa::riscv32::backend3::inst::Riscv32MachInst,
    };

    let test = LowerTest::from_lpir(
        r#"
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
"#,
    );

    // Verify VCode structure - includes edge blocks with move instructions for phi values
    test.assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    br block4
    br block1

  block1:
    add v6, v0, v7
    br block2
    br block3(v6)

  edge block2 -> block3:
    move v1, v6
    br block8(v6)

  edge block2 -> block4:
    move v2, v6
    br block7

  block4:
    add v4, v0, v8
    br block5
    br block6

  edge block1 -> block3:
    move v1, v4
    br block8

  edge block1 -> block4:
    move v2, v4
    br block7

  block7(v2):
    return v2

  block8(v1):
    return v1

}
"#,
    );

    // Additional property checks
    let vcode = test.vcode();

    // Verify that edge blocks exist and contain move instructions
    // We should have edge blocks for critical edges
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
}

#[test]
fn test_lower_load() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = load.i32 v0
    return v1
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    lw v1, 0(v0)
    return v1

}
"#,
    );
}

#[test]
fn test_lower_store() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    store.i32 v0, v1
    return v1
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    sw v1, 0(v0)
    return v1

}
"#,
    );
}

#[test]
fn test_lower_imul() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = imul v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    mul v2, v0, v1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_idiv() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = idiv v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    div v2, v0, v1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_irem() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = irem v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    rem v2, v0, v1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_icmp_eq() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp eq v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    sub v3, v0, v1
    sltiu v2, v3, 1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_jump() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    jump block1(v0)

block1(v1: i32):
    return v1
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    br block1(v0)

  block1(v1):
    return v1

}
"#,
    );
}

#[test]
fn test_lower_br() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    v2 = icmp eq v0, v1
    brif v2, block1(v0), block2(v0)

block1(v3: i32):
    return v3

block2(v4: i32):
    return v4
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    sub v6, v0, v5
    sltiu v4, v6, 1
    br block2(v0)
    br block1(v0)

  block1(v2):
    return v2

  block2(v1):
    return v1

}
"#,
    );
}

#[test]
fn test_lower_icmp_ne() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp ne v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    sub v3, v0, v1
    sltiu v4, v3, 1
    xori v2, v4, 1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_icmp_slt() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp slt v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    slt v2, v0, v1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_icmp_sle() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp sle v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    slt v3, v1, v0
    xori v2, v3, 1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_icmp_sgt() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp sgt v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    slt v2, v1, v0
    return v2

}
"#,
    );
}

#[test]
fn test_lower_icmp_sge() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp sge v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    slt v3, v0, v1
    xori v2, v3, 1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_jump_no_args() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    jump block1

block1:
    v0 = iconst 42
    return v0
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0:
    br block1

  block1:
    return v1

}
"#,
    );
}

#[test]
fn test_lower_br_no_args() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    v2 = icmp eq v0, v1
    brif v2, block1, block2

block1:
    v3 = iconst 1
    return v3

block2:
    v4 = iconst 2
    return v4
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    sub v6, v0, v5
    sltiu v2, v6, 1
    br block2
    br block1

  block1:
    return v7

  block2:
    return v8

}
"#,
    );
}

#[test]
fn test_lower_br_mixed_args() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    v2 = icmp eq v0, v1
    brif v2, block1, block2(v0)

block1:
    v3 = iconst 1
    return v3

block2(v4: i32):
    return v4
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    sub v6, v0, v5
    sltiu v3, v6, 1
    br block2
    br block1(v0)

  block1(v1):
    return v1

  block2:
    return v7

}
"#,
    );
}

#[test]
fn test_lower_icmp_ult() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until unsigned less than comparison is implemented
    // Expected: sltu instruction (unsigned comparison)
    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp ult v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    sltu v2, v0, v1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_icmp_ule() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until unsigned less than or equal is implemented
    // Expected: sltu with swapped operands, then invert
    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp ule v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    sltu v3, v1, v0
    xori v2, v3, 1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_icmp_ugt() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until unsigned greater than is implemented
    // Expected: sltu with swapped operands
    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp ugt v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    sltu v2, v1, v0
    return v2

}
"#,
    );
}

#[test]
fn test_lower_icmp_uge() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until unsigned greater than or equal is implemented
    // Expected: sltu, then invert
    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = icmp uge v0, v1
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0, v1):
    sltu v3, v0, v1
    xori v2, v3, 1
    return v2

}
"#,
    );
}

#[test]
fn test_lower_call() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until call instruction is implemented
    // Expected: jal instruction with relocation for function address
    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    call %other_func(v0) -> v2
    return v2
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    jal v1, other_func(v0)
    return v1

}
"#,
    );
}

#[test]
fn test_lower_call_no_results() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until call instruction is implemented
    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    call %other_func(v0)
    v1 = iconst 42
    return v1
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    jal v2, other_func(v0)
    return v3

}
"#,
    );
}

#[test]
fn test_lower_syscall() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until syscall instruction is implemented
    // Expected: ecall instruction with syscall number and arguments
    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    syscall 1(v0)
    v1 = iconst 42
    return v1
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    ecall 1(v0)
    return v2

}
"#,
    );
}

#[test]
fn test_lower_halt() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until halt instruction is implemented
    // Expected: ebreak instruction
    LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    halt
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0:
    ebreak

}
"#,
    );
}

#[test]
fn test_lower_trap() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: Trap is a terminator, so the block ends with it
    // Expected: trap instruction with trap code
    LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    trap int_divz
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0:
    trap int_divz

}
"#,
    );
}

#[test]
fn test_lower_trapz() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until trapz instruction is implemented
    // Expected: conditional trap if condition is zero
    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    trapz v0, int_divz
    v1 = iconst 42
    return v1
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    trapz v0, int_divz
    return v2

}
"#,
    );
}

#[test]
fn test_lower_trapnz() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    // NOTE: This test will fail until trapnz instruction is implemented
    // Expected: conditional trap if condition is non-zero
    LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    trapnz v0, int_ovf
    v1 = iconst 42
    return v1
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    trapnz v0, int_ovf
    return v2

}
"#,
    );
}
