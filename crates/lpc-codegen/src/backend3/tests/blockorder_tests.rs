//! Tests for block ordering and critical edge detection

extern crate alloc;

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

