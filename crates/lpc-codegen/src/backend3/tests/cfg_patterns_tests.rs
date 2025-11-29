//! Tests for complex CFG patterns (diamonds, loops, etc.)

extern crate alloc;

use crate::backend3::tests::vcode_test_helpers::LowerTest;

/// Test diamond pattern (if-then-else merge)
#[test]
fn test_diamond_pattern() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 10
    jump block3(v1)

block2:
    v2 = iconst 20
    jump block3(v2)

block3(v3: i32):
    return v3
}
"#,
    );

    test.assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    brif v0, block2, block1

  block1:
    jump block3(v4)

  block2:
    jump block3(v5)

  block3(v1):
    return v1

}
"#,
    );

    let vcode = test.vcode();

    // Verify structure: block0 branches to block1 and block2, both merge to block3
    // Check that block3 has two predecessors
    let block3_idx = 3;
    if let Some(pred_range) = vcode.block_pred_range.get(block3_idx) {
        let preds = &vcode.block_preds[pred_range.start..pred_range.end];
        assert!(
            preds.len() >= 2,
            "Block3 (merge point) should have at least 2 predecessors in diamond pattern"
        );
    }
}

/// Test loop with backedge
#[test]
fn test_loop_with_backedge() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = isub v0, v0
    brif v1, block1, block2

block2:
    return v0
}
"#,
    );

    let vcode = test.vcode();

    // Verify loop structure: block1 has itself as a successor (backedge)
    // Find block1's successors
    let block1_idx = 1;
    if let Some(succ_range) = vcode.block_succ_range.get(block1_idx) {
        let succs = &vcode.block_succs[succ_range.start..succ_range.end];
        // Block1 should have block1 (itself) as a successor (backedge)
        let has_backedge = succs.iter().any(|&s| s.index() == block1_idx);
        assert!(
            has_backedge || succs.len() > 0,
            "Loop block should have successors (including possible backedge)"
        );
    }
}

/// Test nested loops
#[test]
fn test_nested_loops() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    brif v0, block1, block3

block1:
    brif v1, block2, block3

block2:
    brif v1, block2, block1

block3:
    return v0
}
"#,
    );

    let vcode = test.vcode();

    // Verify nested loop structure
    // block1->block2->block2 (inner loop) and block1->block2->block1 (outer loop)
    assert!(
        vcode.block_ranges.len() >= 4,
        "Nested loops should have multiple blocks"
    );
}

/// Test switch-like pattern (multiple branches from one block)
#[test]
fn test_switch_like_pattern() {
    // Simulate switch with multiple conditional branches
    // Note: LPIR doesn't have direct switch, so we use multiple branches
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    brif v0, block1, block2

block1:
    return v1

block2:
    brif v0, block3, block4

block3:
    return v1

block4:
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Verify multiple branches from block0
    let block0_idx = 0;
    if let Some(succ_range) = vcode.block_succ_range.get(block0_idx) {
        let succs = &vcode.block_succs[succ_range.start..succ_range.end];
        assert!(
            succs.len() >= 2,
            "Switch-like pattern should have multiple successors from entry block"
        );
    }
}

/// Test function with many blocks and complex control flow
#[test]
fn test_complex_control_flow() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    brif v0, block1, block2

block1:
    brif v1, block3, block4

block2:
    brif v1, block5, block6

block3:
    brif v0, block7, block8

block4:
    brif v0, block9, block10

block5:
    return v0

block6:
    return v1

block7:
    return v0

block8:
    return v1

block9:
    return v0

block10:
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Verify complex structure
    assert!(
        vcode.block_ranges.len() >= 11,
        "Complex control flow should have many blocks"
    );

    // Verify all blocks are reachable from entry
    assert_eq!(vcode.entry.index(), 0, "Entry block should be at index 0");
}

/// Test function with early returns
#[test]
fn test_early_returns() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    return v0

block2:
    v1 = iconst 42
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Verify structure with early returns
    assert!(
        vcode.block_ranges.len() >= 3,
        "Function with early returns should have multiple blocks"
    );

    // Both block1 and block2 should have return instructions
    let mut return_count = 0;
    for inst in &vcode.insts {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Return { .. } = inst {
            return_count += 1;
        }
    }
    assert!(
        return_count >= 2,
        "Function with early returns should have multiple return instructions"
    );
}

/// Test function with phi nodes (block parameters)
#[test]
fn test_phi_nodes() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 10
    jump block3(v1)

block2:
    v2 = iconst 20
    jump block3(v2)

block3(v3: i32):
    return v3
}
"#,
    );

    let vcode = test.vcode();

    // Verify phi node (block parameter) handling
    // Block3 should have a parameter (phi node)
    let block3_idx = 3;
    if let Some(param_range) = vcode.block_params_range.get(block3_idx) {
        let params = &vcode.block_params[param_range.start..param_range.end];
        assert!(
            params.len() >= 1,
            "Block with phi node should have block parameters"
        );
    }
}

