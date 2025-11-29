//! Tests for VCode structure and VCodeBuilder

extern crate alloc;

use alloc::{collections::BTreeMap, collections::BTreeSet, vec::Vec};
use crate::backend3::types::{BlockIndex, VReg};
use crate::backend3::vcode::{BlockLoweringOrder, Callee};
use crate::backend3::vcode_builder::VCodeBuilder;
use crate::isa::riscv32::backend3::inst::{Riscv32ABI, Riscv32MachInst};

#[test]
fn test_vcode_builder_new() {
    let _builder = VCodeBuilder::<Riscv32MachInst>::new();
    // Builder created successfully
}

#[test]
fn test_vcode_builder_alloc_vreg() {
    let mut builder = VCodeBuilder::<Riscv32MachInst>::new();
    let vreg1 = builder.alloc_vreg();
    let vreg2 = builder.alloc_vreg();
    assert_eq!(vreg1.index(), 0);
    assert_eq!(vreg2.index(), 1);
}

#[test]
fn test_vcode_builder_start_block() {
    let mut builder = VCodeBuilder::<Riscv32MachInst>::new();
    let block_idx = BlockIndex::new(0);
    let params = alloc::vec![VReg::new(1), VReg::new(2)];
    builder.start_block(block_idx, params.clone());
    builder.end_block();
    
    // Build and check that block parameters were recorded
    let entry = BlockIndex::new(0);
    let block_order = BlockLoweringOrder {
        lowered_order: Vec::new(),
        lowered_succs: Vec::new(),
        block_to_index: BTreeMap::new(),
        cold_blocks: BTreeSet::new(),
        indirect_targets: BTreeSet::new(),
    };
    let abi = Callee { abi: Riscv32ABI };
    let vcode = builder.build(entry, block_order, abi);
    assert_eq!(vcode.block_params.len(), 2);
}

#[test]
fn test_vcode_builder_build() {
    let mut builder = VCodeBuilder::<Riscv32MachInst>::new();
    let entry = BlockIndex::new(0);
    let block_order = BlockLoweringOrder {
        lowered_order: Vec::new(),
        lowered_succs: Vec::new(),
        block_to_index: BTreeMap::new(),
        cold_blocks: BTreeSet::new(),
        indirect_targets: BTreeSet::new(),
    };
    let abi = Callee {
        abi: Riscv32ABI,
    };
    
    let vcode = builder.build(entry, block_order, abi);
    assert_eq!(vcode.insts.len(), 0);
    assert_eq!(vcode.entry, entry);
}

