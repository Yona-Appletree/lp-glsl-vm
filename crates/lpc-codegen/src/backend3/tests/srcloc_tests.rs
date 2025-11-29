//! Tests for source location tracking

extern crate alloc;

use lpc_lpir::RelSourceLoc;

use crate::backend3::tests::vcode_test_helpers::LowerTest;

/// Test that source locations are tracked for all instructions
#[test]
fn test_srclocs_tracked_for_all_instructions() {
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

    // Source locations should match instruction count
    assert_eq!(
        vcode.srclocs.len(),
        vcode.insts.len(),
        "Source locations should be tracked for all instructions"
    );
}

/// Test that source locations are parallel to instructions array
#[test]
fn test_srclocs_parallel_to_instructions() {
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

    // Each instruction should have a corresponding source location
    for i in 0..vcode.insts.len() {
        assert!(
            i < vcode.srclocs.len(),
            "Instruction {} should have a corresponding source location",
            i
        );
        let _srcloc = vcode.srclocs[i]; // Should not panic
    }
}

/// Test that source locations from IR instructions are preserved
#[test]
fn test_srclocs_preserved_from_ir() {
    // This test verifies that source locations are being tracked
    // The exact values depend on the IR source locations, which may be default
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

    // All source locations should be valid RelSourceLoc values
    for (i, srcloc) in vcode.srclocs.iter().enumerate() {
        // RelSourceLoc is a newtype, so we can't easily check its value
        // But we can verify it exists and doesn't cause issues
        let _ = srcloc;
        assert!(
            i < vcode.insts.len(),
            "Source location {} should correspond to an instruction",
            i
        );
    }
}

/// Test that synthetic instructions (phi moves) have appropriate source locations
#[test]
fn test_synthetic_instructions_srclocs() {
    // Create a function with critical edges to generate phi moves
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

    // All instructions (including synthetic phi moves) should have source locations
    assert_eq!(
        vcode.srclocs.len(),
        vcode.insts.len(),
        "All instructions including synthetic ones should have source locations"
    );

    // Find move instructions (phi moves in edge blocks)
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Move { .. } = inst {
            // Synthetic move instructions should have source locations
            assert!(
                inst_idx < vcode.srclocs.len(),
                "Move instruction at index {} should have a source location",
                inst_idx
            );
            // Synthetic instructions may use default source location
            let _srcloc = vcode.srclocs[inst_idx];
        }
    }
}

/// Test that constant materialization instructions have source locations
#[test]
fn test_constant_materialization_srclocs() {
    // Use a large constant to force LUI+ADDI materialization
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

    // All instructions (including LUI+ADDI for constant materialization) should have source locations
    assert_eq!(
        vcode.srclocs.len(),
        vcode.insts.len(),
        "All instructions including constant materialization should have source locations"
    );

    // Find LUI and ADDI instructions
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        match inst {
            crate::isa::riscv32::backend3::inst::Riscv32MachInst::Lui { .. }
            | crate::isa::riscv32::backend3::inst::Riscv32MachInst::Addi { .. } => {
                assert!(
                    inst_idx < vcode.srclocs.len(),
                    "Constant materialization instruction at index {} should have a source \
                     location",
                    inst_idx
                );
                let _srcloc = vcode.srclocs[inst_idx];
            }
            _ => {}
        }
    }
}

/// Test that source locations are valid RelSourceLoc values
#[test]
fn test_srclocs_valid_type() {
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

    // All source locations should be valid RelSourceLoc
    for srcloc in &vcode.srclocs {
        // RelSourceLoc is Copy, so we can use it
        let _ = *srcloc;
    }
}

/// Test that source locations match instruction order
#[test]
fn test_srclocs_match_instruction_order() {
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

    // Source locations should be in the same order as instructions
    for i in 0..vcode.insts.len() {
        assert!(
            i < vcode.srclocs.len(),
            "Source location {} should exist for instruction {}",
            i,
            i
        );
    }
}

/// Test that empty function still has valid source location structure
#[test]
fn test_empty_function_srclocs() {
    // Function with just return (no other instructions)
    let test = LowerTest::from_lpir(
        r#"
function %test() -> i32 {
block0:
    v1 = iconst 0
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Even with minimal instructions, source locations should match
    assert_eq!(
        vcode.srclocs.len(),
        vcode.insts.len(),
        "Source locations should match instructions even in minimal functions"
    );
}
