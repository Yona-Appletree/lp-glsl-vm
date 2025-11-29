//! Tests for VCode structure and VCodeBuilder

extern crate alloc;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec,
    vec::Vec,
};

use crate::{
    backend3::{
        types::{BlockIndex, VReg},
        vcode::{BlockLoweringOrder, Callee},
        vcode_builder::VCodeBuilder,
    },
    isa::riscv32::backend3::inst::{Riscv32ABI, Riscv32EmitInfo, Riscv32MachInst},
};

#[test]
fn test_vcode_builder_new() {
    let _builder = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    // Builder created successfully
}

#[test]
fn test_vcode_builder_alloc_vreg() {
    let mut builder = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let vreg1 = builder.alloc_vreg();
    let vreg2 = builder.alloc_vreg();
    assert_eq!(vreg1.index(), 0);
    assert_eq!(vreg2.index(), 1);
}

#[test]
fn test_vcode_builder_start_block() {
    let mut builder = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    let params = alloc::vec![VReg::new(1), VReg::new(2)];
    builder.start_block(block_idx, params.clone());
    builder.end_block();

    // Build and check that block parameters were recorded
    use crate::backend3::vcode::LoweredBlock;

    let entry = BlockIndex::new(0);
    let mut block_to_index = BTreeMap::new();
    block_to_index.insert(lpc_lpir::BlockEntity::new(0), entry);
    let block_order = BlockLoweringOrder {
        lowered_order: alloc::vec![LoweredBlock::Orig {
            block: lpc_lpir::BlockEntity::new(0),
        }],
        lowered_succs: alloc::vec![Vec::new()],
        block_to_index,
        cold_blocks: BTreeSet::new(),
        indirect_targets: BTreeSet::new(),
    };
    let abi = Callee { abi: Riscv32ABI };
    let vcode = builder.build(entry, block_order, abi);
    assert_eq!(vcode.block_params.len(), 2);
}

#[test]
fn test_vcode_builder_build() {
    use crate::backend3::vcode::LoweredBlock;

    let mut builder = VCodeBuilder::<Riscv32MachInst>::new(Riscv32EmitInfo);
    let block_idx = BlockIndex::new(0);
    builder.start_block(block_idx, Vec::new());
    builder.end_block();

    let entry = BlockIndex::new(0);
    let mut block_to_index = BTreeMap::new();
    block_to_index.insert(lpc_lpir::BlockEntity::new(0), entry);
    let block_order = BlockLoweringOrder {
        lowered_order: alloc::vec![LoweredBlock::Orig {
            block: lpc_lpir::BlockEntity::new(0),
        }],
        lowered_succs: alloc::vec![Vec::new()],
        block_to_index,
        cold_blocks: BTreeSet::new(),
        indirect_targets: BTreeSet::new(),
    };
    let abi = Callee { abi: Riscv32ABI };

    let vcode = builder.build(entry, block_order, abi);
    assert_eq!(vcode.insts.len(), 0);
    assert_eq!(vcode.entry, entry);
}
