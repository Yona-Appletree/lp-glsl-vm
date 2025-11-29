//! Integration tests for lowering complete functions to VCode

extern crate alloc;

use alloc::vec::Vec;
use crate::backend3::lower::lower_function;
use crate::backend3::vcode::Callee;
use crate::isa::riscv32::backend3::{inst::Riscv32ABI, Riscv32LowerBackend};
use lpc_lpir::{Function, Immediate, InstData, Opcode, Signature, Type, Value};

#[test]
fn test_lower_simple_add_function() {
    // Function: fn add(a: i32, b: i32) -> i32 { a + b }
    let sig = Signature::new(alloc::vec![Type::I32, Type::I32], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("add"));
    
    // Create block with parameters matching function signature
    let v0 = Value::new(0); // a
    let v1 = Value::new(1); // b
    func.dfg.set_value_type(v0, Type::I32);
    func.dfg.set_value_type(v1, Type::I32);
    let block = func.create_block_with_params(alloc::vec![v0, v1]);
    func.append_block(block);
    
    // Create iadd instruction: v2 = v0 + v1
    let v2 = Value::new(2);
    func.dfg.set_value_type(v2, Type::I32);
    let inst_data = InstData::arithmetic(Opcode::Iadd, v2, v0, v1);
    let inst = func.create_inst(inst_data);
    func.append_inst(inst, block);
    
    // Add return: return v2
    let return_inst_data = InstData::return_(Vec::from([v2]));
    let return_inst = func.create_inst(return_inst_data);
    func.append_inst(return_inst, block);
    
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    // Verify VCode structure
    assert_eq!(vcode.entry.index(), 0);
    assert!(vcode.insts.len() >= 1); // Should have at least the ADD instruction
    assert_eq!(vcode.block_ranges.len(), 1); // One block
}

#[test]
fn test_lower_function_with_constants() {
    // Function: fn test() -> i32 { 10 + 20 }
    let sig = Signature::new(alloc::vec![], alloc::vec![Type::I32]);
    let mut func = Function::new(sig, alloc::string::String::from("test"));
    let block = func.create_block();
    func.append_block(block);
    
    // Create iconst 10
    let v1 = Value::new(1);
    func.dfg.set_value_type(v1, Type::I32);
    let inst1_data = InstData::constant(v1, Immediate::I32(10));
    let inst1 = func.create_inst(inst1_data);
    func.append_inst(inst1, block);
    
    // Create iconst 20
    let v2 = Value::new(2);
    func.dfg.set_value_type(v2, Type::I32);
    let inst2_data = InstData::constant(v2, Immediate::I32(20));
    let inst2 = func.create_inst(inst2_data);
    func.append_inst(inst2, block);
    
    // Create iadd: v3 = v1 + v2
    let v3 = Value::new(3);
    func.dfg.set_value_type(v3, Type::I32);
    let inst3_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
    let inst3 = func.create_inst(inst3_data);
    func.append_inst(inst3, block);
    
    // Add return
    let return_inst_data = InstData::return_(Vec::from([v3]));
    let return_inst = func.create_inst(return_inst_data);
    func.append_inst(return_inst, block);
    
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    // Verify VCode structure
    assert_eq!(vcode.entry.index(), 0);
    // Should have at least the ADD instruction, plus any constant materialization
    assert!(vcode.insts.len() >= 1);
}

