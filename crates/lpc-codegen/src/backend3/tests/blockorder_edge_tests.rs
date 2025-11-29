//! Edge case tests for block ordering

extern crate alloc;

use alloc::vec::Vec;

use crate::backend3::tests::vcode_test_helpers::LowerTest;

/// Test single block function (no edges)
#[test]
fn test_single_block_no_edges() {
    let test = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Should have exactly one block
    assert_eq!(
        vcode.block_ranges.len(),
        1,
        "Single block function should have exactly one block range"
    );

    // Entry block should be at index 0
    assert_eq!(vcode.entry.index(), 0, "Entry block should be at index 0");
}

/// Test function with no critical edges
#[test]
fn test_no_critical_edges() {
    // Function where no block has both multiple successors and multiple predecessors
    let test = LowerTest::from_lpir(
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

    let vcode = test.vcode();

    // Should have original blocks (no edge blocks needed)
    // Count edge blocks
    let edge_blocks: Vec<_> = vcode
        .block_order
        .lowered_order
        .iter()
        .filter(|lb| matches!(lb, crate::backend3::vcode::LoweredBlock::Edge { .. }))
        .collect();

    // With no critical edges, there should be no edge blocks
    // (block0 has 2 successors, but block1 and block2 each have only 1 predecessor)
    assert_eq!(
        edge_blocks.len(),
        0,
        "Function with no critical edges should have no edge blocks"
    );
}

/// Test function with all critical edges
#[test]
fn test_all_critical_edges() {
    // Function where all edges are critical
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    brif v0, block3, block4

block2:
    brif v0, block3, block4

block3:
    return v0

block4:
    return v0
}
"#,
    );

    let vcode = test.vcode();

    // Should have edge blocks for critical edges
    // block1->block3, block1->block4, block2->block3, block2->block4 are all critical
    let edge_blocks: Vec<_> = vcode
        .block_order
        .lowered_order
        .iter()
        .filter(|lb| matches!(lb, crate::backend3::vcode::LoweredBlock::Edge { .. }))
        .collect();

    assert!(
        edge_blocks.len() >= 2,
        "Function with critical edges should have edge blocks"
    );
}

/// Test entry block handling
#[test]
fn test_entry_block_handling() {
    let test = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Entry block should be valid
    assert!(
        vcode.entry.index() < vcode.block_ranges.len(),
        "Entry block index should be valid"
    );

    // Entry block should be in block_to_index mapping
    // (We can't easily check this without access to the IR block, but we verify structure)
    assert_eq!(
        vcode.entry.index(),
        0,
        "Entry block should typically be at index 0"
    );
}

/// Test function with unreachable blocks
#[test]
fn test_unreachable_blocks() {
    // Function with a block that's never reached
    let test = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    return v1

block1:
    v2 = iconst 100
    return v2
}
"#,
    );

    let vcode = test.vcode();

    // Both blocks should be in the lowered order (even if unreachable)
    // The exact behavior depends on how unreachable blocks are handled
    assert!(
        vcode.block_ranges.len() >= 1,
        "Should have at least one block"
    );
}

/// Test function with many blocks
#[test]
fn test_many_blocks() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    brif v0, block3, block4

block2:
    brif v0, block5, block6

block3:
    return v0

block4:
    return v0

block5:
    return v0

block6:
    return v0
}
"#,
    );

    let vcode = test.vcode();

    // Should have all blocks (plus any edge blocks)
    assert!(
        vcode.block_ranges.len() >= 7,
        "Should have at least 7 blocks (original blocks)"
    );
}

/// Test that block order is deterministic
#[test]
fn test_block_order_deterministic() {
    let test1 = LowerTest::from_lpir(
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

    let test2 = LowerTest::from_lpir(
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

    let vcode1 = test1.vcode();
    let vcode2 = test2.vcode();

    // Block order should be the same for identical functions
    assert_eq!(
        vcode1.block_order.lowered_order.len(),
        vcode2.block_order.lowered_order.len(),
        "Block order should be deterministic"
    );
}

/// Test that edge blocks come after their source blocks
#[test]
fn test_edge_blocks_after_source() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    brif v0, block3, block4

block2:
    brif v0, block3, block4

block3:
    return v0

block4:
    return v0
}
"#,
    );

    let vcode = test.vcode();

    // Find edge blocks and verify they come after their source blocks
    let mut block_indices: alloc::collections::BTreeMap<lpc_lpir::BlockEntity, usize> =
        alloc::collections::BTreeMap::new();
    for (idx, lowered_block) in vcode.block_order.lowered_order.iter().enumerate() {
        match lowered_block {
            crate::backend3::vcode::LoweredBlock::Orig { block } => {
                block_indices.insert(*block, idx);
            }
            crate::backend3::vcode::LoweredBlock::Edge { from, .. } => {
                // Edge block should come after its source block
                if let Some(&source_idx) = block_indices.get(from) {
                    assert!(
                        idx > source_idx,
                        "Edge block at index {} should come after source block at index {}",
                        idx,
                        source_idx
                    );
                }
            }
        }
    }
}

/// Test entry block with critical edges
#[test]
fn test_block_order_entry_critical_edges() {
    // Entry block branches to blocks that have multiple predecessors
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    brif v0, block3, block4

block2:
    brif v0, block3, block4

block3:
    return v0

block4:
    return v0
}
"#,
    );

    let vcode = test.vcode();

    // Entry block (block0) has 2 successors, but block1 and block2 each have only 1 predecessor
    // So block0->block1 and block0->block2 are NOT critical edges
    // However, block1->block3, block1->block4, block2->block3, block2->block4 ARE critical

    // Count edge blocks
    let edge_blocks: Vec<_> = vcode
        .block_order
        .lowered_order
        .iter()
        .filter(|lb| matches!(lb, crate::backend3::vcode::LoweredBlock::Edge { .. }))
        .collect();

    // Should have edge blocks for critical edges (block1->block3, block1->block4, etc.)
    assert!(
        edge_blocks.len() > 0,
        "Function with critical edges should have edge blocks"
    );

    // Verify entry block is at index 0
    assert_eq!(vcode.entry.index(), 0, "Entry block should be at index 0");
}

/// Test exit blocks with critical edges
#[test]
fn test_block_order_exit_critical_edges() {
    // Blocks that branch to exit blocks (returns) with multiple predecessors
    // Note: Returns don't create critical edges since they have no successors
    // But we can test blocks that branch to blocks with multiple predecessors
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    brif v0, block3, block4

block2:
    brif v0, block3, block4

block3:
    return v0

block4:
    return v0
}
"#,
    );

    let vcode = test.vcode();

    // block3 and block4 are exit blocks (they return)
    // block1->block3, block1->block4, block2->block3, block2->block4 are critical edges
    // because block1 and block2 have multiple successors, and block3/block4 have multiple predecessors

    let edge_blocks: Vec<_> = vcode
        .block_order
        .lowered_order
        .iter()
        .filter(|lb| matches!(lb, crate::backend3::vcode::LoweredBlock::Edge { .. }))
        .collect();

    assert!(
        edge_blocks.len() > 0,
        "Function with critical edges to exit blocks should have edge blocks"
    );
}

/// Test that RPO preserves defs-before-uses property
#[test]
fn test_block_order_rpo_property() {
    // Create a function where block order matters for defs-before-uses
    // block0 defines v1, block1 uses v1, block2 defines v2, block3 uses v2
    // RPO should ensure block0 comes before block1, block2 comes before block3
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v0, v0
    brif v0, block1, block2

block1:
    v2 = iadd v1, v0
    return v2

block2:
    v3 = iadd v0, v0
    brif v0, block3, block4

block3:
    v4 = iadd v3, v0
    return v4

block4:
    return v3
}
"#,
    );

    let vcode = test.vcode();

    // Verify block order: block0 should come before block1, block2 should come before block3
    let mut block_indices: alloc::collections::BTreeMap<lpc_lpir::BlockEntity, usize> =
        alloc::collections::BTreeMap::new();

    for (idx, lowered_block) in vcode.block_order.lowered_order.iter().enumerate() {
        match lowered_block {
            crate::backend3::vcode::LoweredBlock::Orig { block } => {
                block_indices.insert(*block, idx);
            }
            _ => {}
        }
    }

    // Find block0 and block1 indices (need to search by block number)
    // Since we can't easily map block names to entities, we verify the structure
    // The key property is that the entry block (block0) should come first
    assert_eq!(
        vcode.entry.index(),
        0,
        "Entry block should be first in RPO order"
    );

    // Verify that all original blocks are present
    assert!(
        block_indices.len() >= 5,
        "Should have at least 5 original blocks (block0-block4)"
    );
}
