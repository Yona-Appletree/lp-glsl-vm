//! Tests for block ordering and critical edge detection

extern crate alloc;

use alloc::vec::Vec;
use crate::backend3::blockorder::compute_block_order;
use lpc_lpir::{Function, InstData, Signature, Type};

#[test]
fn test_block_order_single_block() {
    let sig = Signature::new(alloc::vec![Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    let block = func.create_block();
    func.append_block(block);
    
    let cfg = lpc_lpir::ControlFlowGraph::from_function(&func);
    let domtree = lpc_lpir::DominatorTree::from_cfg(&cfg);
    
    let block_order = compute_block_order(&func, &cfg, &domtree);
    assert_eq!(block_order.lowered_order.len(), 1);
}

#[test]
fn test_block_order_two_blocks() {
    let sig = Signature::new(alloc::vec![Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    let block0 = func.create_block();
    let block1 = func.create_block();
    func.append_block(block0);
    func.append_block(block1);
    
    // Add a jump from block0 to block1
    let inst_data = InstData::jump(block1, alloc::vec::Vec::new());
    let inst = func.create_inst(inst_data);
    func.append_inst(inst, block0);
    
    let cfg = lpc_lpir::ControlFlowGraph::from_function(&func);
    let domtree = lpc_lpir::DominatorTree::from_cfg(&cfg);
    
    let block_order = compute_block_order(&func, &cfg, &domtree);
    assert_eq!(block_order.lowered_order.len(), 2);
}

/// Test critical edge detection and splitting
/// 
/// Creates a CFG with critical edges:
///   block0 (entry) - branches to block1 and block2
///   block1 - branches to block3 and block4
///   block2 - branches to block3 and block4
///   block3 - (merge point with 2 predecessors)
///   block4 - (merge point with 2 predecessors)
///
/// The edges block1->block3, block1->block4, block2->block3, block2->block4
/// are critical edges (source has multiple successors AND target has multiple predecessors)
/// and should be split, creating edge blocks.
#[test]
fn test_block_order_critical_edges() {
    let sig = Signature::new(alloc::vec![Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    
    let v0 = lpc_lpir::Value::new(0);
    func.dfg.set_value_type(v0, Type::I32);
    let block0 = func.create_block_with_params(alloc::vec![v0]);
    let block1 = func.create_block();
    let block2 = func.create_block();
    let block3 = func.create_block();
    let block4 = func.create_block();
    
    func.append_block(block0);
    func.append_block(block1);
    func.append_block(block2);
    func.append_block(block3);
    func.append_block(block4);
    
    // block0 branches to block1 and block2 (conditional)
    let inst0 = InstData::branch(v0, block1, Vec::new(), block2, Vec::new());
    let inst0_entity = func.create_inst(inst0);
    func.append_inst(inst0_entity, block0);
    
    // block1 branches to block3 and block4 (conditional) - has multiple successors
    let v1 = lpc_lpir::Value::new(1);
    func.dfg.set_value_type(v1, Type::I32);
    let inst1 = InstData::branch(v1, block3, Vec::new(), block4, Vec::new());
    let inst1_entity = func.create_inst(inst1);
    func.append_inst(inst1_entity, block1);
    
    // block2 branches to block3 and block4 (conditional) - has multiple successors
    let inst2 = InstData::branch(v1, block3, Vec::new(), block4, Vec::new());
    let inst2_entity = func.create_inst(inst2);
    func.append_inst(inst2_entity, block2);
    
    let cfg = lpc_lpir::ControlFlowGraph::from_function(&func);
    let domtree = lpc_lpir::DominatorTree::from_cfg(&cfg);
    
    let block_order = compute_block_order(&func, &cfg, &domtree);
    
    // Should have 5 original blocks + edge blocks
    assert!(block_order.lowered_order.len() >= 5);
    
    // Verify edge blocks are present
    // block1->block3, block1->block4, block2->block3, block2->block4 are all critical
    let edge_blocks: Vec<_> = block_order.lowered_order.iter()
        .filter(|lb| matches!(lb, crate::backend3::vcode::LoweredBlock::Edge { .. }))
        .collect();
    assert!(edge_blocks.len() >= 2, "Should have at least 2 edge blocks for critical edges");
    
    // Verify edge blocks come after their source blocks in RPO order
    let mut found_block1 = false;
    let mut found_block2 = false;
    for lowered_block in &block_order.lowered_order {
        match lowered_block {
            crate::backend3::vcode::LoweredBlock::Orig { block } => {
                if *block == block1 {
                    found_block1 = true;
                }
                if *block == block2 {
                    found_block2 = true;
                }
            }
            crate::backend3::vcode::LoweredBlock::Edge { from, .. } => {
                // Edge blocks should come after their source blocks
                if *from == block1 {
                    assert!(found_block1, "Edge block from block1 should come after block1");
                }
                if *from == block2 {
                    assert!(found_block2, "Edge block from block2 should come after block2");
                }
            }
        }
    }
}

/// Test that entry block is correctly identified and mapped
#[test]
fn test_entry_block_mapping() {
    let sig = Signature::new(alloc::vec![Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    
    let v0 = lpc_lpir::Value::new(0);
    func.dfg.set_value_type(v0, Type::I32);
    let entry_block = func.create_block_with_params(alloc::vec![v0]);
    let other_block = func.create_block();
    
    func.append_block(entry_block);
    func.append_block(other_block);
    
    let cfg = lpc_lpir::ControlFlowGraph::from_function(&func);
    let domtree = lpc_lpir::DominatorTree::from_cfg(&cfg);
    
    let block_order = compute_block_order(&func, &cfg, &domtree);
    
    // Entry block should be in block_to_index mapping
    assert!(block_order.block_to_index.contains_key(&entry_block));
    
    // Entry block should be at index 0 in lowered_order
    let entry_idx = block_order.block_to_index.get(&entry_block).unwrap();
    assert_eq!(entry_idx.index(), 0, "Entry block should be at index 0");
    
    // Verify the lowered block at index 0 is the entry block
    match &block_order.lowered_order[0] {
        crate::backend3::vcode::LoweredBlock::Orig { block } => {
            assert_eq!(*block, entry_block, "First lowered block should be entry block");
        }
        _ => panic!("Entry block should be an Orig block"),
    }
}

/// Test that predecessors are computed correctly from successors
#[test]
fn test_predecessor_computation() {
    let sig = Signature::new(alloc::vec![Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    
    let v0 = lpc_lpir::Value::new(0);
    func.dfg.set_value_type(v0, Type::I32);
    let block0 = func.create_block_with_params(alloc::vec![v0]);
    let block1 = func.create_block();
    let block2 = func.create_block();
    
    func.append_block(block0);
    func.append_block(block1);
    func.append_block(block2);
    
    // block0 branches to block1 and block2
    let inst0 = InstData::branch(v0, block1, Vec::new(), block2, Vec::new());
    let inst0_entity = func.create_inst(inst0);
    func.append_inst(inst0_entity, block0);
    
    // Lower the function to get VCode with populated predecessors
    use crate::backend3::lower::lower_function;
    use crate::backend3::vcode::Callee;
    use crate::isa::riscv32::backend3::{inst::Riscv32ABI, Riscv32LowerBackend};
    
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    // Verify predecessors are computed
    assert_eq!(vcode.block_pred_range.len(), vcode.block_ranges.len(),
               "Each block should have a predecessor range");
    
    // Verify predecessor computation: block1 and block2 should have block0 as predecessor
    // (assuming block0 is at index 0, block1 at index 1, block2 at index 2)
    let block0_idx = crate::backend3::types::BlockIndex::new(0);
    
    // Check block1 (index 1) has block0 as predecessor
    if let Some(pred_range) = vcode.block_pred_range.get(1) {
        let preds: Vec<_> = vcode.block_preds[pred_range.start..pred_range.end].iter().collect();
        assert!(preds.contains(&&block0_idx), 
                "Block1 should have block0 as predecessor");
    }
    
    // Check block2 (index 2) has block0 as predecessor
    if let Some(pred_range) = vcode.block_pred_range.get(2) {
        let preds: Vec<_> = vcode.block_preds[pred_range.start..pred_range.end].iter().collect();
        assert!(preds.contains(&&block0_idx), 
                "Block2 should have block0 as predecessor");
    }
    
    // Verify that for each successor relationship, there's a corresponding predecessor
    for (block_idx, succ_range) in vcode.block_succ_range.iter().enumerate() {
        let pred_block = crate::backend3::types::BlockIndex::new(block_idx as u32);
        for succ in &vcode.block_succs[succ_range.start..succ_range.end] {
            // Verify that pred_block appears in succ's predecessor list
            if let Some(pred_range) = vcode.block_pred_range.get(succ.index() as usize) {
                let preds = &vcode.block_preds[pred_range.start..pred_range.end];
                assert!(preds.contains(&pred_block),
                       "If block {} has {} as successor, then {} should have {} as predecessor",
                       block_idx, succ.index(), succ.index(), block_idx);
            }
        }
    }
}

