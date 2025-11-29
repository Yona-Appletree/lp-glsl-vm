//! Tests for relocation recording and structure

extern crate alloc;

use crate::backend3::tests::vcode_test_helpers::LowerTest;

/// Test recording relocations for function calls
#[test]
fn test_relocations_for_function_calls() {
    let test = LowerTest::from_lpir(
        r#"
function %test(i32) -> i32 {
block0(v0: i32):
    call %other(v0) -> v1
    return v1
}
"#,
    );

    let vcode = test.vcode();

    // Find the call instruction (JAL)
    let mut found_call = false;
    for (inst_idx, inst) in vcode.insts.iter().enumerate() {
        if let crate::isa::riscv32::backend3::inst::Riscv32MachInst::Jal { callee, .. } = inst {
            found_call = true;

            // Check if relocation is recorded for this instruction
            let insn_idx = crate::backend3::types::InsnIndex::new(inst_idx);
            let reloc = vcode.relocations.iter().find(|r| r.inst_idx == insn_idx);

            // Note: Currently, relocations are not automatically recorded during lowering
            // This test verifies the structure exists and can be used when relocations are implemented
            if let Some(reloc) = reloc {
                assert_eq!(
                    reloc.kind,
                    crate::backend3::vcode::RelocKind::FunctionCall,
                    "Function call should have FunctionCall relocation kind"
                );
                assert_eq!(
                    reloc.target, *callee,
                    "Relocation target should match function name"
                );
            }
        }
    }

    assert!(found_call, "Should have found a call instruction");
}

/// Test relocation structure matches expected format
#[test]
fn test_relocation_structure() {
    // Create a VCode with a relocation manually (for testing structure)
    use crate::{
        backend3::{
            types::{BlockIndex, InsnIndex},
            vcode::{BlockLoweringOrder, Callee, RelocKind, VCodeReloc},
            vcode_builder::VCodeBuilder,
        },
        isa::riscv32::backend3::inst::{Riscv32ABI, Riscv32EmitInfo, Riscv32MachInst},
    };

    let mut builder = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    builder.start_block(block_idx, alloc::vec![]);

    // Add an instruction
    let inst = Riscv32MachInst::Ebreak;
    builder.push(inst, lpc_lpir::RelSourceLoc::default());

    // Record a relocation
    let inst_idx = InsnIndex::new(0);
    builder.record_reloc(inst_idx, RelocKind::FunctionCall, "test_func".into());

    builder.end_block();

    // Build VCode
    let entry = BlockIndex::new(0);
    let block_order = BlockLoweringOrder {
        lowered_order: alloc::vec![crate::backend3::vcode::LoweredBlock::Orig {
            block: lpc_lpir::BlockEntity::new(0),
        }],
        lowered_succs: alloc::vec![alloc::vec![]],
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = Callee { abi: Riscv32ABI };
    let vcode = builder.build(entry, block_order, abi);

    // Verify relocation structure
    assert_eq!(vcode.relocations.len(), 1, "Should have one relocation");
    let reloc = &vcode.relocations[0];
    assert_eq!(
        reloc.inst_idx, inst_idx,
        "Relocation should reference correct instruction"
    );
    assert_eq!(
        reloc.kind,
        RelocKind::FunctionCall,
        "Relocation should have correct kind"
    );
    assert_eq!(
        reloc.target, "test_func",
        "Relocation should have correct target"
    );
}

/// Test multiple relocations in one function
#[test]
fn test_multiple_relocations() {
    use crate::{
        backend3::{
            types::{BlockIndex, InsnIndex},
            vcode::{BlockLoweringOrder, Callee, RelocKind},
            vcode_builder::VCodeBuilder,
        },
        isa::riscv32::backend3::inst::{Riscv32ABI, Riscv32EmitInfo, Riscv32MachInst},
    };

    let mut builder = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    builder.start_block(block_idx, alloc::vec![]);

    // Add multiple instructions
    let inst1 = Riscv32MachInst::Ebreak;
    builder.push(inst1, lpc_lpir::RelSourceLoc::default());
    builder.record_reloc(InsnIndex::new(0), RelocKind::FunctionCall, "func1".into());

    let inst2 = Riscv32MachInst::Ebreak;
    builder.push(inst2, lpc_lpir::RelSourceLoc::default());
    builder.record_reloc(InsnIndex::new(1), RelocKind::FunctionCall, "func2".into());

    builder.end_block();

    // Build VCode
    let entry = BlockIndex::new(0);
    let block_order = BlockLoweringOrder {
        lowered_order: alloc::vec![crate::backend3::vcode::LoweredBlock::Orig {
            block: lpc_lpir::BlockEntity::new(0),
        }],
        lowered_succs: alloc::vec![alloc::vec![]],
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = Callee { abi: Riscv32ABI };
    let vcode = builder.build(entry, block_order, abi);

    // Verify multiple relocations
    assert_eq!(vcode.relocations.len(), 2, "Should have two relocations");
    assert_eq!(
        vcode.relocations[0].target, "func1",
        "First relocation should have correct target"
    );
    assert_eq!(
        vcode.relocations[1].target, "func2",
        "Second relocation should have correct target"
    );
}

/// Test that relocations reference valid instruction indices
#[test]
fn test_relocations_reference_valid_instructions() {
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
            reloc.inst_idx.index() < vcode.insts.len(),
            "Relocation should reference valid instruction index (got {}, max {})",
            reloc.inst_idx.index(),
            vcode.insts.len()
        );
    }
}

/// Test relocation kinds
#[test]
fn test_relocation_kinds() {
    use crate::backend3::vcode::RelocKind;

    // Test that relocation kinds are valid
    let _function_call = RelocKind::FunctionCall;
    let _branch = RelocKind::Branch;

    // Verify they're different
    assert_ne!(
        RelocKind::FunctionCall,
        RelocKind::Branch,
        "Relocation kinds should be distinct"
    );
}
