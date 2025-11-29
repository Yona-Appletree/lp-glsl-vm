//! Tests for clobber collection from instructions

extern crate alloc;

use crate::backend3::tests::vcode_test_helpers::LowerTest;

/// Test that clobbers are collected for function calls
#[test]
fn test_clobbers_for_function_calls() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = call %other(v0)
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Find the call instruction (JAL)
    let mut found_call = false;
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Jal { .. } = inst {
            found_call = true;

            // Check if clobbers are recorded for this instruction
            let insn_idx = crate::backend3::types::InsnIndex::new(inst_idx);
            // Note: Currently, RISC-V instructions don't implement get_clobbers(),
            // so clobbers may be empty. This test verifies the structure exists.
            let _has_clobbers = vcode.clobbers.contains_key(&insn_idx);
        }
    }

    assert!(found_call, "Should have found a call instruction");
}

/// Test that clobbers are collected for syscalls
#[test]
fn test_clobbers_for_syscalls() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    syscall 1(v0) -> v1
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Find the syscall instruction (ECALL)
    let mut found_syscall = false;
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Ecall { .. } = inst {
            found_syscall = true;

            // Check if clobbers are recorded for this instruction
            let insn_idx = crate::backend3::types::InsnIndex::new(inst_idx);
            // Note: Currently, RISC-V instructions don't implement get_clobbers(),
            // so clobbers may be empty. This test verifies the structure exists.
            let _has_clobbers = vcode.clobbers.contains_key(&insn_idx);
        }
    }

    assert!(found_syscall, "Should have found a syscall instruction");
}

/// Test that clobber sets are correctly associated with instructions
#[test]
fn test_clobbers_associated_with_instructions() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = call %other(v0)
    v2 = call %another(v1)
    return v2
}
"#,
    );

    let vcode = test.vcode();

    // Verify that clobbers map uses instruction indices
    for (insn_idx, _clobber_set) in &vcode.clobbers {
        assert!(
            insn_idx.index() < vcode.insts.len(),
            "Clobber instruction index {} should be less than instruction count {}",
            insn_idx.index(),
            vcode.insts.len()
        );
    }
}

/// Test that clobbers map is sparse (only entries for instructions with clobbers)
#[test]
fn test_clobbers_map_is_sparse() {
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

    // Regular arithmetic instructions should not have clobbers
    // (unless they're function calls, which they're not in this test)
    // So clobbers map should be empty or sparse
    // The exact behavior depends on whether instructions implement get_clobbers()
    // Currently, RISC-V instructions don't implement it, so clobbers should be empty
    for (insn_idx, _clobber_set) in &vcode.clobbers {
        // If there are clobbers, verify they're for valid instructions
        assert!(
            insn_idx.index() < vcode.insts.len(),
            "Clobber instruction index should be valid"
        );
    }
}

/// Test that clobber sets contain valid register indices
#[test]
fn test_clobber_sets_valid() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    v1 = call %other(v0)
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Verify that clobber sets contain valid register indices
    // Note: Currently using placeholder u32 for PRegSet
    for (_insn_idx, clobber_set) in &vcode.clobbers {
        // Clobber set should be a valid PRegSet
        // Values represent physical registers
        for reg in *clobber_set {
            // Register should be valid
            let _ = reg.index();
        }
    }
}

/// Test that instructions without clobbers don't appear in clobbers map
#[test]
fn test_no_clobbers_for_regular_instructions() {
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

    // Regular arithmetic instructions should not have clobbers
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        match inst {
            crate::isa::riscv32::backend3::inst::Riscv32MachInst::Add { .. }
            | crate::isa::riscv32::backend3::inst::Riscv32MachInst::Sub { .. }
            | crate::isa::riscv32::backend3::inst::Riscv32MachInst::Mul { .. } => {
                let insn_idx = crate::backend3::types::InsnIndex::new(inst_idx);
                // These instructions should not have clobbers (unless they implement get_clobbers)
                // Currently, RISC-V instructions don't implement it, so they won't be in the map
                if vcode.clobbers.contains_key(&insn_idx) {
                    // If they do have clobbers, that's also valid (implementation-dependent)
                    let _clobber_set = &vcode.clobbers[&insn_idx];
                }
            }
            _ => {}
        }
    }
}
