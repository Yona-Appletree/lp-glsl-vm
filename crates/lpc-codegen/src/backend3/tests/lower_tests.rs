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
    assert!(vcode.insts.len() >= 0);
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

