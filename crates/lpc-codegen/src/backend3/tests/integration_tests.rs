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
