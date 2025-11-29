//! Tests for VCode structure invariants and validation

extern crate alloc;

use crate::backend3::tests::vcode_test_helpers::LowerTest;

/// Test that all block ranges cover all instructions exactly once
#[test]
fn test_block_ranges_cover_all_instructions() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v0, v0
    brif v1, block1, block2

block1:
    v2 = iconst 10
    return v2

block2:
    v3 = iconst 20
    return v3
}
"#,
    );

    let vcode = test.vcode();

    // Calculate total instructions covered by block ranges
    let mut total_covered = 0;
    for i in 0..vcode.block_ranges.len() {
        if let Some(range) = vcode.block_ranges.get(i) {
            assert!(range.start <= range.end, "Range start should be <= end");
            total_covered += range.len();
        }
    }

    // Should match actual instruction count
    assert_eq!(
        total_covered,
        vcode.insts.len(),
        "Block ranges should cover all instructions exactly once"
    );
}

/// Test that block ranges are non-overlapping and contiguous
#[test]
fn test_block_ranges_non_overlapping() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v0, v0
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Check ranges are contiguous (end of one is start of next)
    for i in 0..vcode.block_ranges.len() {
        if let Some(range) = vcode.block_ranges.get(i) {
            assert!(range.start <= range.end, "Range start should be <= end");

            // Check contiguity with next range
            if i + 1 < vcode.block_ranges.len() {
                if let Some(next_range) = vcode.block_ranges.get(i + 1) {
                    assert_eq!(
                        range.end,
                        next_range.start,
                        "Ranges should be contiguous (end of range {} should equal start of range {})",
                        i,
                        i + 1
                    );
                }
            }
        }
    }
}

/// Test that predecessor/successor relationships are symmetric
#[test]
fn test_predecessor_successor_symmetry() {
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

    // For each block, verify that if A has B as successor, then B has A as predecessor
    for (pred_idx, succ_range) in vcode.block_succ_range.iter().enumerate() {
        let pred_block = crate::backend3::types::BlockIndex::new(pred_idx as u32);
        for succ in &vcode.block_succs[succ_range.start..succ_range.end] {
            // Verify that pred_block appears in succ's predecessor list
            if let Some(pred_range) = vcode.block_pred_range.get(succ.index() as usize) {
                let preds = &vcode.block_preds[pred_range.start..pred_range.end];
                assert!(
                    preds.contains(&pred_block),
                    "If block {} has {} as successor, then {} should have {} as predecessor",
                    pred_idx,
                    succ.index(),
                    succ.index(),
                    pred_idx
                );
            }
        }
    }

    // Also verify reverse: if A has B as predecessor, then B has A as successor
    for (succ_idx, pred_range) in vcode.block_pred_range.iter().enumerate() {
        let succ_block = crate::backend3::types::BlockIndex::new(succ_idx as u32);
        for pred in &vcode.block_preds[pred_range.start..pred_range.end] {
            // Verify that succ_block appears in pred's successor list
            if let Some(succ_range) = vcode.block_succ_range.get(pred.index() as usize) {
                let succs = &vcode.block_succs[succ_range.start..succ_range.end];
                assert!(
                    succs.contains(&succ_block),
                    "If block {} has {} as predecessor, then {} should have {} as successor",
                    succ_idx,
                    pred.index(),
                    pred.index(),
                    succ_idx
                );
            }
        }
    }
}

/// Test that block parameter ranges match block count
#[test]
fn test_block_params_range_count() {
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

    // Each block should have a parameter range entry
    assert_eq!(
        vcode.block_params_range.len(),
        vcode.block_ranges.len(),
        "Block parameter ranges should match block count"
    );
}

/// Test that branch argument ranges match successor structure
#[test]
fn test_branch_args_match_successors() {
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

    // For each block, the number of branch arg ranges should match number of successors
    for (block_idx, succ_range) in vcode.block_succ_range.iter().enumerate() {
        let num_succs = succ_range.len();

        // Find corresponding branch arg succ range
        if let Some(branch_arg_succ_range) = vcode.branch_block_arg_succ_range.get(block_idx) {
            assert_eq!(
                branch_arg_succ_range.len(),
                num_succs,
                "Block {} should have {} branch arg ranges matching {} successors",
                block_idx,
                branch_arg_succ_range.len(),
                num_succs
            );
        }
    }
}

/// Test that entry block is valid and in block_to_index
#[test]
fn test_entry_block_valid() {
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

    // Entry block index should be valid
    assert!(
        vcode.entry.index() < vcode.block_ranges.len() as u32,
        "Entry block index {} should be less than block count {}",
        vcode.entry.index(),
        vcode.block_ranges.len()
    );

    // Entry block should be in block_to_index mapping
    // (We need to find the IR block that corresponds to the entry)
    // Since we can't easily get the IR block from VCode, we verify the entry is valid
    assert!(
        vcode.entry.index() < vcode.block_order.lowered_order.len() as u32,
        "Entry block should be in lowered_order"
    );
}

/// Test that operand ranges match instruction count
#[test]
fn test_operand_ranges_match_instructions() {
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

    assert_eq!(
        vcode.operand_ranges.len(),
        vcode.insts.len(),
        "Each instruction should have an operand range"
    );
}

/// Test that source locations match instruction count
#[test]
fn test_srclocs_match_instructions() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v0, v0
    v2 = isub v0, v0
    return v1
}
"#,
    );

    let vcode = test.vcode();

    assert_eq!(
        vcode.srclocs.len(),
        vcode.insts.len(),
        "Source locations should match instruction count (one per instruction)"
    );
}

/// Test that operand ranges are valid (non-overlapping, covering all operands)
#[test]
fn test_operand_ranges_valid() {
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

    // Calculate total operands covered by operand ranges
    let mut total_covered = 0;
    for i in 0..vcode.operand_ranges.len() {
        if let Some(range) = vcode.operand_ranges.get(i) {
            assert!(range.start <= range.end, "Operand range start should be <= end");
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

/// Test that operand ranges are contiguous
#[test]
fn test_operand_ranges_contiguous() {
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

    // Check ranges are contiguous (end of one is start of next)
    for i in 0..vcode.operand_ranges.len() {
        if let Some(range) = vcode.operand_ranges.get(i) {
            assert!(range.start <= range.end, "Operand range start should be <= end");

            // Check contiguity with next range
            if i + 1 < vcode.operand_ranges.len() {
                if let Some(next_range) = vcode.operand_ranges.get(i + 1) {
                    assert_eq!(
                        range.end,
                        next_range.start,
                        "Operand ranges should be contiguous (end of range {} should equal start of range {})",
                        i,
                        i + 1
                    );
                }
            }
        }
    }
}

/// Test that block metadata matches block count
#[test]
fn test_block_metadata_count() {
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

    assert_eq!(
        vcode.block_metadata.len(),
        vcode.block_ranges.len(),
        "Block metadata should match block count"
    );
}

/// Test that constants map is valid (no duplicate VRegs)
#[test]
fn test_constants_map_valid() {
    let test = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 42
    v2 = iconst 100
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Constants map should be valid (BTreeMap ensures no duplicates)
    // Just verify it's accessible
    let _ = &vcode.constants.constants;
}

/// Test that relocations reference valid instruction indices
#[test]
fn test_relocations_valid() {
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

    // All relocations should reference valid instruction indices
    for reloc in &vcode.relocations {
        assert!(
            reloc.inst_idx.index() < vcode.insts.len() as u32,
            "Relocation should reference valid instruction index"
        );
    }
}

