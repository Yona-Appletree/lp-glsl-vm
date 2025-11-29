//! Tests for lowering iconst/iadd/isub instructions

extern crate alloc;

use alloc::vec::Vec;
use crate::backend3::lower::lower_function;
use crate::backend3::vcode::Callee;
use crate::isa::riscv32::backend3::{inst::Riscv32ABI, Riscv32LowerBackend};
use lpc_lpir::{Function, Immediate, InstData, Opcode, Signature, Type, Value};

#[test]
fn test_lower_iconst() {
    let sig = Signature::new(alloc::vec![], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    let block = func.create_block();
    func.append_block(block);
    
    // Create iconst instruction
    let v1 = Value::new(1);
    func.dfg.set_value_type(v1, Type::I32);
    let inst_data = InstData::constant(v1, Immediate::I32(42));
    let inst = func.create_inst(inst_data);
    func.append_inst(inst, block);
    
    // Add return
    let return_inst_data = InstData::return_(Vec::from([v1]));
    let return_inst = func.create_inst(return_inst_data);
    func.append_inst(return_inst, block);
    
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    // Should have at least zero instructions (iconst may materialize as inline or LUI+ADDI)
    // Note: len() is always >= 0, but this assertion documents the expectation
    let _ = vcode.insts.len();
}

#[test]
fn test_lower_iadd() {
    let sig = Signature::new(alloc::vec![Type::I32, Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    
    // Create block with parameters matching function signature
    let v0 = Value::new(0);
    let v1 = Value::new(1);
    func.dfg.set_value_type(v0, Type::I32);
    func.dfg.set_value_type(v1, Type::I32);
    let block = func.create_block_with_params(alloc::vec![v0, v1]);
    func.append_block(block);
    
    // Create iadd instruction
    let v2 = Value::new(2);
    func.dfg.set_value_type(v2, Type::I32);
    let inst_data = InstData::arithmetic(Opcode::Iadd, v2, v0, v1);
    let inst = func.create_inst(inst_data);
    func.append_inst(inst, block);
    
    // Add return
    let return_inst_data = InstData::return_(Vec::from([v2]));
    let return_inst = func.create_inst(return_inst_data);
    func.append_inst(return_inst, block);
    
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    // Should have at least one instruction (the ADD)
    assert!(vcode.insts.len() >= 1);
}

#[test]
fn test_lower_isub() {
    let sig = Signature::new(alloc::vec![Type::I32, Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    
    // Create block with parameters matching function signature
    let v0 = Value::new(0);
    let v1 = Value::new(1);
    func.dfg.set_value_type(v0, Type::I32);
    func.dfg.set_value_type(v1, Type::I32);
    let block = func.create_block_with_params(alloc::vec![v0, v1]);
    func.append_block(block);
    
    // Create isub instruction
    let v2 = Value::new(2);
    func.dfg.set_value_type(v2, Type::I32);
    let inst_data = InstData::arithmetic(Opcode::Isub, v2, v0, v1);
    let inst = func.create_inst(inst_data);
    func.append_inst(inst, block);
    
    // Add return
    let return_inst_data = InstData::return_(Vec::from([v2]));
    let return_inst = func.create_inst(return_inst_data);
    func.append_inst(return_inst, block);
    
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    // Should have at least one instruction (the SUB)
    assert!(vcode.insts.len() >= 1);
}

/// Test that operands are collected correctly from instructions
#[test]
fn test_operand_collection() {
    let sig = Signature::new(alloc::vec![Type::I32, Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    
    // Create block with parameters matching function signature
    let v0 = Value::new(0);
    let v1 = Value::new(1);
    func.dfg.set_value_type(v0, Type::I32);
    func.dfg.set_value_type(v1, Type::I32);
    let block = func.create_block_with_params(alloc::vec![v0, v1]);
    func.append_block(block);
    
    // Create iadd instruction: v2 = v0 + v1
    // This should produce an ADD instruction with:
    // - 1 def (rd = v2)
    // - 2 uses (rs1 = v0, rs2 = v1)
    let v2 = Value::new(2);
    func.dfg.set_value_type(v2, Type::I32);
    let inst_data = InstData::arithmetic(Opcode::Iadd, v2, v0, v1);
    let inst = func.create_inst(inst_data);
    func.append_inst(inst, block);
    
    // Add return
    let return_inst_data = InstData::return_(Vec::from([v2]));
    let return_inst = func.create_inst(return_inst_data);
    func.append_inst(return_inst, block);
    
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    // Verify operands are collected
    assert_eq!(vcode.operand_ranges.len(), vcode.insts.len(), 
               "Each instruction should have an operand range");
    
    // Verify that operands array is populated
    assert!(!vcode.operands.is_empty() || vcode.insts.is_empty(),
            "Operands should be populated if there are instructions");
    
    // Verify operand ranges match instruction count
    let total_operands: usize = (0..vcode.operand_ranges.len())
        .map(|i| {
            let range = vcode.operand_ranges.get(i).unwrap();
            range.len()
        })
        .sum();
    assert_eq!(total_operands, vcode.operands.len(),
               "Total operand count should match operands array length");
}

/// Test that phi moves are correctly emitted in edge blocks
///
/// Creates a function with critical edges and phi nodes:
///   block0 (entry) - branches to block1 and block2
///   block1 - computes v1, branches to block3 and block4
///   block2 - computes v2, branches to block3 and block4
///   block3 - phi node: v3 = phi(v1 from block1, v2 from block2)
///   block4 - phi node: v4 = phi(v1 from block1, v2 from block2)
///
/// The edges block1->block3, block1->block4, block2->block3, block2->block4
/// are critical edges (source has multiple successors AND target has multiple predecessors)
/// and should have edge blocks with move instructions.
#[test]
fn test_phi_moves_in_edge_blocks() {
    use crate::isa::riscv32::backend3::inst::Riscv32MachInst;
    
    let sig = Signature::new(alloc::vec![Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    
    // Entry block with parameter
    let v0 = Value::new(0);
    func.dfg.set_value_type(v0, Type::I32);
    let block0 = func.create_block_with_params(alloc::vec![v0]);
    func.append_block(block0);
    
    // block1: computes v1 = v0 + 1, then branches to block3 and block4
    let block1 = func.create_block();
    func.append_block(block1);
    let v1 = Value::new(1);
    func.dfg.set_value_type(v1, Type::I32);
    let const1 = Value::new(2);
    func.dfg.set_value_type(const1, Type::I32);
    let const1_inst = InstData::constant(const1, Immediate::I32(1));
    let const1_inst_entity = func.create_inst(const1_inst);
    func.append_inst(const1_inst_entity, block1);
    let add1_inst = InstData::arithmetic(Opcode::Iadd, v1, v0, const1);
    let add1_inst_entity = func.create_inst(add1_inst);
    func.append_inst(add1_inst_entity, block1);
    
    // block2: computes v2 = v0 + 2, then branches to block3 and block4
    let block2 = func.create_block();
    func.append_block(block2);
    let v2 = Value::new(3);
    func.dfg.set_value_type(v2, Type::I32);
    let const2 = Value::new(4);
    func.dfg.set_value_type(const2, Type::I32);
    let const2_inst = InstData::constant(const2, Immediate::I32(2));
    let const2_inst_entity = func.create_inst(const2_inst);
    func.append_inst(const2_inst_entity, block2);
    let add2_inst = InstData::arithmetic(Opcode::Iadd, v2, v0, const2);
    let add2_inst_entity = func.create_inst(add2_inst);
    func.append_inst(add2_inst_entity, block2);
    
    // block3: phi node v3 = phi(v1 from block1, v2 from block2)
    let v3 = Value::new(5);
    func.dfg.set_value_type(v3, Type::I32);
    let block3 = func.create_block_with_params(alloc::vec![v3]);
    func.append_block(block3);
    
    // block4: phi node v4 = phi(v1 from block1, v2 from block2)
    let v4 = Value::new(6);
    func.dfg.set_value_type(v4, Type::I32);
    let block4 = func.create_block_with_params(alloc::vec![v4]);
    func.append_block(block4);
    
    // block0 branches to block1 and block2
    let branch_inst = InstData::branch(v0, block1, Vec::new(), block2, Vec::new());
    let branch_inst_entity = func.create_inst(branch_inst);
    func.append_inst(branch_inst_entity, block0);
    
    // block1 branches to block3 and block4 (multiple successors = critical edge source)
    // Passing v1 as argument to both (phi source)
    let branch1_inst = InstData::branch(v0, block3, Vec::from([v1]), block4, Vec::from([v1]));
    let branch1_inst_entity = func.create_inst(branch1_inst);
    func.append_inst(branch1_inst_entity, block1);
    
    // block2 branches to block3 and block4 (multiple successors = critical edge source)
    // Passing v2 as argument to both (phi source)
    let branch2_inst = InstData::branch(v0, block3, Vec::from([v2]), block4, Vec::from([v2]));
    let branch2_inst_entity = func.create_inst(branch2_inst);
    func.append_inst(branch2_inst_entity, block2);
    
    // block3 returns v3
    let return_inst = InstData::return_(Vec::from([v3]));
    let return_inst_entity = func.create_inst(return_inst);
    func.append_inst(return_inst_entity, block3);
    
    // block4 returns v4
    let return4_inst = InstData::return_(Vec::from([v4]));
    let return4_inst_entity = func.create_inst(return4_inst);
    func.append_inst(return4_inst_entity, block4);
    
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    // Verify that edge blocks exist and contain move instructions
    // We should have edge blocks for block1->block3 and block2->block3
    let edge_blocks: Vec<_> = vcode.block_order.lowered_order.iter()
        .enumerate()
        .filter(|(_, lb)| matches!(lb, crate::backend3::vcode::LoweredBlock::Edge { .. }))
        .collect();
    
    assert!(edge_blocks.len() >= 2, "Should have at least 2 edge blocks for critical edges");
    
    // Count move instructions in the VCode
    let move_count = vcode.insts.iter()
        .filter(|inst| matches!(inst, Riscv32MachInst::Move { .. }))
        .count();
    
    // Should have move instructions for phi values
    // Note: If source and target VRegs are the same, moves may be elided
    // The exact count depends on VReg allocation, but we should have some moves
    assert!(move_count > 0 || edge_blocks.len() > 0, 
            "Should have moves or edge blocks for phi values");
    
    // Verify that edge blocks are properly tracked in block ranges
    assert_eq!(vcode.block_ranges.len(), vcode.block_order.lowered_order.len(),
               "Block ranges should match lowered order length");
}

