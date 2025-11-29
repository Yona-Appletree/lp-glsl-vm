//! Tests for constant materialization (inline and large)

extern crate alloc;

use crate::backend3::constants::materialize_constant;
use crate::backend3::types::{VReg, Writable};
use crate::backend3::vcode_builder::VCodeBuilder;
use crate::isa::riscv32::backend3::inst::Riscv32MachInst;
use lpc_lpir::RelSourceLoc;

#[test]
fn test_materialize_inline_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();
    
    // Small constant that fits in 12 bits
    let vreg = materialize_constant(
        &mut vcode,
        42,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for inline constant"),
        |_rd, _rs1, _imm| panic!("Should not create ADDI for inline constant"),
    );
    
    // Build VCode to check constants
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let built_vcode = vcode.build(entry, block_order, abi);
    // Should have recorded the constant
    assert!(built_vcode.constants.constants.contains_key(&vreg));
    // Should not have emitted any instructions for inline constants
    assert_eq!(built_vcode.insts.len(), 0);
}

#[test]
fn test_materialize_large_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();
    
    // Large constant that doesn't fit in 12 bits
    let vreg = materialize_constant(
        &mut vcode,
        50000, // > 2047, requires LUI+ADDI
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
    );
    
    // Should have emitted LUI + ADDI instructions
    // Build VCode to check instructions
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let vcode = vcode.build(entry, block_order, abi);
    assert_eq!(vcode.insts.len(), 2);
    
    // Check that first instruction is LUI
    match &vcode.insts[0] {
        Riscv32MachInst::Lui { .. } => {}
        _ => panic!("First instruction should be LUI"),
    }
    
    // Check that second instruction is ADDI
    match &vcode.insts[1] {
        Riscv32MachInst::Addi { .. } => {}
        _ => panic!("Second instruction should be ADDI"),
    }
}

#[test]
fn test_materialize_negative_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();
    
    // Negative constant that fits in 12 bits (inline)
    let vreg = materialize_constant(
        &mut vcode,
        -100,
        srcloc,
        |_rd, _imm| panic!("Should not create LUI for inline constant"),
        |_rd, _rs1, _imm| panic!("Should not create ADDI for inline constant"),
    );
    
    // Build VCode to check constants
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let built_vcode = vcode.build(entry, block_order, abi);
    // Should have recorded the constant
    assert!(built_vcode.constants.constants.contains_key(&vreg));
    // Should not have emitted any instructions for inline constants
    assert_eq!(built_vcode.insts.len(), 0);
}

#[test]
fn test_materialize_large_negative_constant() {
    let mut vcode = VCodeBuilder::<Riscv32MachInst>::new();
    let srcloc = RelSourceLoc::default();
    
    // Large negative constant that doesn't fit in 12 bits
    let vreg = materialize_constant(
        &mut vcode,
        -50000, // < -2048, requires LUI+ADDI
        srcloc,
        |rd, imm| Riscv32MachInst::Lui { rd, imm },
        |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
    );
    
    // Should have emitted LUI + ADDI instructions
    // Build VCode to check instructions
    let entry = crate::backend3::types::BlockIndex::new(0);
    let block_order = crate::backend3::vcode::BlockLoweringOrder {
        lowered_order: alloc::vec::Vec::new(),
        lowered_succs: alloc::vec::Vec::new(),
        block_to_index: alloc::collections::BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };
    let abi = crate::backend3::vcode::Callee {
        abi: crate::isa::riscv32::backend3::inst::Riscv32ABI,
    };
    let vcode = vcode.build(entry, block_order, abi);
    assert_eq!(vcode.insts.len(), 2);
}

