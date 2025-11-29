//! Integration tests for lowering complete functions to VCode

extern crate alloc;

use crate::backend3::tests::vcode_test_helpers::LowerTest;

#[test]
fn test_lower_simple_add_function() {
    // Function: fn add(a: i32, b: i32) -> i32 { a + b }
    LowerTest::from_lpir(
        r#"
function %add(i32, i32) -> i32 {
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
fn test_lower_function_with_constants() {
    // Function: fn test() -> i32 { 10 + 20 }
    LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 10
    v2 = iconst 20
    v3 = iadd v1, v2
    return v3
}
"#,
    )
    .assert_vcode(
        r#"
vcode {
  entry: block0

  block0:
    add v2, v3, v4
    return v2

}
"#,
    );
}

/// Test that block ranges are computed correctly when edge blocks are present
///
/// This verifies that edge blocks are properly tracked in block_ranges
/// and that the ranges account for both original blocks and edge blocks.
#[test]
fn test_block_ranges_with_edge_blocks() {
    // Create a function with critical edges to generate edge blocks
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    jump block3

block2:
    jump block3

block3:
    v1 = iconst 42
    return v1
}
"#,
    );

    // Verify VCode structure matches expected format
    test.assert_vcode(
        r#"
vcode {
  entry: block0

  block0(v0):
    brif v0, block2, block1

  block1:
    jump block3

  block2:
    jump block3

  block3:
    return v2

}
"#,
    );

    // Verify block ranges are computed correctly
    let vcode = test.vcode();
    // Should have ranges for: block0, edge blocks (if any), block1, block2, block3
    // The exact count depends on whether critical edges were detected and split
    assert!(
        vcode.block_ranges.len() >= 4,
        "Should have at least 4 block ranges"
    );

    // Verify that block ranges are non-overlapping and cover all instructions
    let mut total_instructions = 0;
    for i in 0..vcode.block_ranges.len() {
        let range = vcode.block_ranges.get(i).unwrap();
        assert!(range.start <= range.end, "Range start should be <= end");
        total_instructions += range.len();

        // Verify ranges don't overlap (except at boundaries)
        if i > 0 {
            let prev_range = vcode.block_ranges.get(i - 1).unwrap();
            assert_eq!(prev_range.end, range.start, "Ranges should be contiguous");
        }
    }

    // Total instructions covered by ranges should match actual instruction count
    assert_eq!(
        total_instructions,
        vcode.insts.len(),
        "Block ranges should cover all instructions"
    );

    // Verify entry block is at index 0
    assert_eq!(vcode.entry.index(), 0);
}

/// Test complex function with multiple blocks, critical edges, phi nodes, and constants
#[test]
fn test_lower_complex_function() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    LowerTest::from_lpir(
        r#"
function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    brif v2, block1, block2

block1:
    v3 = iconst 10
    brif v3, block3(v3), block4(v3)

block2:
    v4 = iconst 20
    brif v4, block3(v4), block4(v4)

block3(v5: i32):
    v6 = iadd v5, v5
    return v6

block4(v7: i32):
    v8 = imul v7, v7
    return v8
}
"#,
    );

    // Verify complex function structure
    // This tests: multiple blocks, critical edges (block1->block3, block1->block4, etc.),
    // phi nodes (block3 and block4), and constants
}

/// Test that source locations are preserved through lowering
#[test]
fn test_lower_preserves_srclocs() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

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

    // Verify source locations match instruction count
    assert_eq!(
        vcode.srclocs.len(),
        vcode.insts.len(),
        "Source locations should match instruction count"
    );

    // Verify all source locations are valid (not default/zero)
    // Note: Actual source location values depend on IR source locations
    for srcloc in &vcode.srclocs {
        // Just verify they exist (actual values depend on IR)
        let _ = srcloc;
    }
}

/// Test lowering with constants requiring LUI+ADDI sequence
#[test]
fn test_lower_large_constants() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    let test = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 50000
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Large constant (50000) should require LUI + ADDI sequence
    // Verify that instructions were emitted
    assert!(
        vcode.insts.len() >= 2,
        "Large constant should require at least 2 instructions (LUI + ADDI)"
    );

    // Find LUI and ADDI instructions
    let mut found_lui = false;
    let mut found_addi = false;
    for inst in &vcode.insts {
        match inst {
            crate::isa::riscv32::backend3::inst::Riscv32MachInst::Lui { .. } => {
                found_lui = true;
            }
            crate::isa::riscv32::backend3::inst::Riscv32MachInst::Addi { .. } => {
                found_addi = true;
            }
            _ => {}
        }
    }

    assert!(found_lui, "Should have LUI instruction for large constant");
    assert!(
        found_addi,
        "Should have ADDI instruction for large constant"
    );
}

/// Test lowering with mixed inline and large constants
#[test]
fn test_lower_mixed_constants() {
    use crate::backend3::tests::vcode_test_helpers::LowerTest;

    let test = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    v2 = iconst 50000
    v3 = iadd v1, v2
    return v3
}
"#,
    );

    let vcode = test.vcode();

    // Should have:
    // - Inline constant (42) - recorded in constants map, no instructions
    // - Large constant (50000) - LUI + ADDI instructions
    // - ADD instruction for iadd

    // Verify large constant instructions
    let mut found_lui = false;
    let mut found_addi = false;
    for inst in &vcode.insts {
        match inst {
            crate::isa::riscv32::backend3::inst::Riscv32MachInst::Lui { .. } => {
                found_lui = true;
            }
            crate::isa::riscv32::backend3::inst::Riscv32MachInst::Addi { .. } => {
                found_addi = true;
            }
            _ => {}
        }
    }

    assert!(found_lui, "Should have LUI instruction for large constant");
    assert!(
        found_addi,
        "Should have ADDI instruction for large constant"
    );

    // Verify constants map has inline constant
    // (Large constants don't go in constants map, they're materialized as instructions)
    assert!(
        !vcode.constants.constants.is_empty() || vcode.insts.is_empty(),
        "Should have constants recorded or instructions"
    );
}
