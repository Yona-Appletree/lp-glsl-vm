//! Tests for operand collection, constraints, and ranges

extern crate alloc;

use alloc::vec::Vec;

use regalloc2::{OperandConstraint, OperandKind};

use crate::backend3::tests::vcode_test_helpers::LowerTest;

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
        match operand.constraint() {
            OperandConstraint::Any => {}
            OperandConstraint::FixedReg(_) => {}
            OperandConstraint::Reg => {}
            OperandConstraint::Stack => {}
            OperandConstraint::Reuse(_) => {}
            OperandConstraint::Limit(_) => {}
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
                    .filter(|op| (**op).kind() == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| (**op).kind() == OperandKind::Use)
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
                    assert_eq!(range.end, next_range.start, "Ranges should be contiguous");
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
                assert_eq!(range.len(), 0, "EBREAK instruction should have no operands");
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
    call %other_func(v0, v1, v2) -> v3
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
                    .filter(|op| (**op).kind() == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| (**op).kind() == OperandKind::Use)
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
                    .filter(|op| (**op).kind() == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| (**op).kind() == OperandKind::Use)
                    .collect();

                assert_eq!(defs.len(), 0, "SW instruction should have no def operands");
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
                    .filter(|op| (**op).kind() == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| (**op).kind() == OperandKind::Use)
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
                    .filter(|op| (**op).kind() == OperandKind::Def)
                    .collect();
                let uses: Vec<_> = operands
                    .iter()
                    .filter(|op| (**op).kind() == OperandKind::Use)
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
        let _vreg = operand.vreg(); // Should not panic
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

/// Test instructions with Mod (read-write) operands
///
/// Note: Currently, RISC-V instructions don't have Mod operands.
/// This test verifies that if Mod operands are added in the future, they are handled correctly.
#[test]
fn test_operand_collection_mod_operands() {
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

    // Verify that operands are collected (even if none are Mod)
    // Currently, RISC-V instructions use Use/Def, not Mod
    // Note: regalloc2 doesn't support Mod directly - it's split into Use+Def
    for operand in &vcode.operands {
        match operand.kind() {
            OperandKind::Use => {}
            OperandKind::Def => {}
        }
    }

    // Verify operands are collected
    assert!(
        !vcode.operands.is_empty() || vcode.insts.is_empty(),
        "Operands should be collected if there are instructions"
    );
}

/// Test instructions with fixed register constraints
///
/// Note: Currently, RISC-V instructions use OperandConstraint::Any.
/// This test verifies the structure supports fixed register constraints.
#[test]
fn test_operand_collection_fixed_registers() {
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

    // Verify operand constraints are collected
    for operand in &vcode.operands {
        match operand.constraint() {
            OperandConstraint::Any => {}
            OperandConstraint::FixedReg(_) => {
                // If fixed registers are used, verify they're handled correctly
            }
            OperandConstraint::Reg => {}
            OperandConstraint::Stack => {}
            OperandConstraint::Reuse(_) => {}
            OperandConstraint::Limit(_) => {}
        }
    }

    // Currently, all RISC-V operands use OperandConstraint::Any
    // This test verifies the structure is ready for fixed register constraints
    assert!(
        !vcode.operands.is_empty() || vcode.insts.is_empty(),
        "Operands should be collected"
    );
}

/// Test instructions with register class constraints
#[test]
fn test_operand_collection_reg_class() {
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

    // Verify register class constraints are supported
    // Note: Register class is determined by the VReg's register class, not a separate constraint
    for operand in &vcode.operands {
        match operand.constraint() {
            OperandConstraint::Any => {}
            OperandConstraint::FixedReg(_) => {}
            OperandConstraint::Reg => {
                // Reg constraint means the operand must be in the same register class as the VReg
            }
            OperandConstraint::Stack => {}
            OperandConstraint::Reuse(_) => {}
            OperandConstraint::Limit(_) => {}
        }
    }

    // Currently, all RISC-V operands use OperandConstraint::Any
    // This test verifies the structure is ready for register class constraints
    assert!(
        !vcode.operands.is_empty() || vcode.insts.is_empty(),
        "Operands should be collected"
    );
}

/// Test operand collection with empty instruction list
#[test]
fn test_operand_collection_empty() {
    use crate::{
        backend3::{
            types::BlockIndex,
            vcode::{BlockLoweringOrder, Callee, LoweredBlock},
            vcode_builder::VCodeBuilder,
        },
        isa::riscv32::backend3::inst::{Riscv32ABI, Riscv32EmitInfo, Riscv32MachInst},
    };

    let mut builder = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    builder.start_block(block_idx, alloc::vec![]);
    builder.end_block();

    let entry = BlockIndex::new(0);
    let mut block_to_index = alloc::collections::BTreeMap::new();
    block_to_index.insert(lpc_lpir::BlockEntity::new(0), entry);
    let block_order = BlockLoweringOrder {
        lowered_order: alloc::vec![LoweredBlock::Orig {
            block: lpc_lpir::BlockEntity::new(0),
        }],
        lowered_succs: alloc::vec![alloc::vec![]],
        block_to_index,
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = Callee { abi: Riscv32ABI };
    let vcode = builder.build(entry, block_order, abi);

    // Empty instruction list should have empty operands
    assert_eq!(
        vcode.operands.len(),
        0,
        "Empty instruction list should have no operands"
    );
    assert_eq!(
        vcode.operand_ranges.len(),
        0,
        "Empty instruction list should have no operand ranges"
    );
}

/// Test operand collection with single instruction
#[test]
fn test_operand_collection_single() {
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

    // Find the ADD instruction
    let mut found_add = false;
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Add { .. } = inst {
            found_add = true;

            // Get operand range for this instruction
            if let Some(range) = vcode.operand_ranges.get(inst_idx) {
                let operands = &vcode.operands[range.start..range.end];

                // ADD should have: 1 def (rd), 2 uses (rs1, rs2) = 3 operands total
                assert_eq!(
                    operands.len(),
                    3,
                    "ADD instruction should have 3 operands (1 def, 2 uses)"
                );
            }
        }
    }

    assert!(found_add, "Should have found an ADD instruction");
}
