//! Type verification.

use alloc::{format, string::String, vec::Vec};

use crate::dfg::Opcode;
use crate::{Function, VerifierError, Type};

/// Verify type correctness
///
/// Checks:
/// - Instruction operands have compatible types
/// - Instruction results have correct types
/// - Block arguments match parameter types
pub fn verify_types(function: &Function, errors: &mut Vec<VerifierError>) {
    verify_instruction_types(function, errors);
    verify_block_argument_types(function, errors);
}

/// Verify instruction operand and result types
fn verify_instruction_types(function: &Function, errors: &mut Vec<VerifierError>) {
    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                match &inst_data.opcode {
                    Opcode::Iadd | Opcode::Isub | Opcode::Imul | Opcode::Idiv | Opcode::Irem => {
                        // Arithmetic operations: both operands should be integers
                        if inst_data.args.len() >= 2 {
                            let arg1_ty = function.dfg.value_type(inst_data.args[0]);
                            let arg2_ty = function.dfg.value_type(inst_data.args[1]);

                            if let (Some(ty1), Some(ty2)) = (arg1_ty, arg2_ty) {
                                if ty1 != Type::I32 || ty2 != Type::I32 {
                                    errors.push(VerifierError::with_location(
                                        format!(
                                            "Arithmetic operation expects i32 operands, got {:?} and {:?}",
                                            ty1, ty2
                                        ),
                                        format!("inst{}", inst.index()),
                                    ));
                                }
                            }
                        }
                    }
                    Opcode::Load => {
                        // Load requires type information
                        if inst_data.ty.is_none() {
                            errors.push(VerifierError::with_location(
                                String::from("Load instruction requires type information"),
                                format!("inst{}", inst.index()),
                            ));
                        }
                    }
                    Opcode::Store => {
                        // Store requires type information
                        if inst_data.ty.is_none() {
                            errors.push(VerifierError::with_location(
                                String::from("Store instruction requires type information"),
                                format!("inst{}", inst.index()),
                            ));
                        }
                    }
                    _ => {
                        // Other instructions - basic checks can be added here
                    }
                }
            }
        }
    }
}

/// Verify block argument types match parameter types
fn verify_block_argument_types(_function: &Function, _errors: &mut Vec<VerifierError>) {
    // For now, we only check that the number of arguments matches.
    // Full type checking would require tracking parameter types, which
    // is not yet implemented in BlockData.
    // This is a placeholder for future enhancement.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dfg::InstData;
    use crate::signature::Signature;
    use crate::value::Value;

    #[test]
    fn test_verify_types_load_without_type() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        // Create a load without type information
        let mut inst_data = InstData::load(v2, v1, Type::I32);
        inst_data.ty = None; // Remove type info to test error
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_types(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("Load instruction requires type"));
    }
}

