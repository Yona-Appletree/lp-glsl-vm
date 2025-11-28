//! Type verification.

use alloc::{format, string::String, vec::Vec};

use crate::{dfg::Opcode, Function, Type, VerifierError};

/// Verify type correctness
///
/// Checks:
/// - Instruction operands have compatible types
/// - Instruction results have correct types
/// - Block arguments match parameter types
/// - Function call types match signatures (if module provided)
pub fn verify_types(
    function: &Function,
    errors: &mut Vec<VerifierError>,
    module: Option<&crate::Module>,
) {
    verify_instruction_types(function, errors);
    verify_block_argument_types(function, errors);
    if let Some(module) = module {
        verify_function_call_types(function, errors, module);
    }
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
                                            "Arithmetic operation expects i32 operands, got {:?} \
                                             and {:?}",
                                            ty1, ty2
                                        ),
                                        format!("inst{}", inst.index()),
                                    ));
                                }
                            }
                        }
                    }
                    Opcode::Icmp { .. } => {
                        // Integer comparison: both operands should be integers (i32 or u32)
                        if inst_data.args.len() >= 2 {
                            let arg1_ty = function.dfg.value_type(inst_data.args[0]);
                            let arg2_ty = function.dfg.value_type(inst_data.args[1]);

                            if let (Some(ty1), Some(ty2)) = (arg1_ty, arg2_ty) {
                                if !ty1.is_integer() || !ty2.is_integer() {
                                    errors.push(VerifierError::with_location(
                                        format!(
                                            "Icmp operation expects integer operands, got {:?} \
                                             and {:?}",
                                            ty1, ty2
                                        ),
                                        format!("inst{}", inst.index()),
                                    ));
                                }
                                if ty1 != ty2 {
                                    errors.push(VerifierError::with_location(
                                        format!(
                                            "Icmp operation expects operands of the same type, \
                                             got {:?} and {:?}",
                                            ty1, ty2
                                        ),
                                        format!("inst{}", inst.index()),
                                    ));
                                }
                            }
                        }
                    }
                    Opcode::Fcmp { .. } => {
                        // Floating point comparison: both operands should be f32
                        if inst_data.args.len() >= 2 {
                            let arg1_ty = function.dfg.value_type(inst_data.args[0]);
                            let arg2_ty = function.dfg.value_type(inst_data.args[1]);

                            if let (Some(ty1), Some(ty2)) = (arg1_ty, arg2_ty) {
                                if ty1 != Type::F32 || ty2 != Type::F32 {
                                    errors.push(VerifierError::with_location(
                                        format!(
                                            "Fcmp operation expects f32 operands, got {:?} and \
                                             {:?}",
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
fn verify_block_argument_types(function: &Function, errors: &mut Vec<VerifierError>) {
    for block in function.blocks() {
        if let Some(block_data) = function.block_data(block) {
            let expected_param_types = &block_data.param_types;

            // Check all incoming edges
            for pred_block in function.blocks() {
                for inst in function.block_insts(pred_block) {
                    if let Some(inst_data) = function.dfg.inst_data(inst) {
                        if let Some(block_args) = &inst_data.block_args {
                            for (target_block, args) in &block_args.targets {
                                if *target_block == block {
                                    // Check argument count matches
                                    if args.len() != expected_param_types.len() {
                                        continue; // This is caught by CFG validation
                                    }

                                    // Check argument types match parameter types
                                    for (i, (arg, expected_ty)) in
                                        args.iter().zip(expected_param_types.iter()).enumerate()
                                    {
                                        if let Some(arg_ty) = function.dfg.value_type(*arg) {
                                            if arg_ty != *expected_ty {
                                                errors.push(VerifierError::with_location(
                                                    format!(
                                                        "Block {} parameter {} expects type {}, \
                                                         but argument {} has type {}",
                                                        block.index(),
                                                        i,
                                                        expected_ty,
                                                        i,
                                                        arg_ty
                                                    ),
                                                    format!("inst{}", inst.index()),
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Verify function call types match function signatures
fn verify_function_call_types(
    function: &Function,
    errors: &mut Vec<VerifierError>,
    module: &crate::Module,
) {
    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                if let Opcode::Call { callee } = &inst_data.opcode {
                    if let Some(callee_func) = module.get_function(callee) {
                        // Check argument types match function signature
                        if inst_data.args.len() != callee_func.signature.params.len() {
                            errors.push(VerifierError::with_location(
                                format!(
                                    "Call to '{}' expects {} arguments, got {}",
                                    callee,
                                    callee_func.signature.params.len(),
                                    inst_data.args.len()
                                ),
                                format!("inst{}", inst.index()),
                            ));
                        } else {
                            for (i, (arg, expected_ty)) in inst_data
                                .args
                                .iter()
                                .zip(callee_func.signature.params.iter())
                                .enumerate()
                            {
                                if let Some(arg_ty) = function.dfg.value_type(*arg) {
                                    if arg_ty != *expected_ty {
                                        errors.push(VerifierError::with_location(
                                            format!(
                                                "Call to '{}' parameter {} expects type {}, but \
                                                 argument {} has type {}",
                                                callee, i, expected_ty, i, arg_ty
                                            ),
                                            format!("inst{}", inst.index()),
                                        ));
                                    }
                                }
                            }
                        }

                        // Check result types match function signature
                        if inst_data.results.len() != callee_func.signature.returns.len() {
                            errors.push(VerifierError::with_location(
                                format!(
                                    "Call to '{}' returns {} values, but {} results expected",
                                    callee,
                                    callee_func.signature.returns.len(),
                                    inst_data.results.len()
                                ),
                                format!("inst{}", inst.index()),
                            ));
                        } else {
                            for (i, (result, expected_ty)) in inst_data
                                .results
                                .iter()
                                .zip(callee_func.signature.returns.iter())
                                .enumerate()
                            {
                                if let Some(result_ty) = function.dfg.value_type(*result) {
                                    if result_ty != *expected_ty {
                                        errors.push(VerifierError::with_location(
                                            format!(
                                                "Call to '{}' result {} expects type {}, but \
                                                 result {} has type {}",
                                                callee, i, expected_ty, i, result_ty
                                            ),
                                            format!("inst{}", inst.index()),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    // Function existence is checked by entity validation
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::{dfg::InstData, signature::Signature, value::Value};

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
        verify_types(&func, &mut errors, None);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("Load instruction requires type"));
    }

    #[test]
    fn test_verify_types_block_argument_type_mismatch() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        let param = Value::new(0);
        let block1 = func.create_block_with_params(vec![param]);
        func.append_block(block0);
        func.append_block(block1);

        // Set parameter type to F32
        if let Some(block_data) = func.block_data_mut(block1) {
            block_data.param_types[0] = Type::F32;
        }

        // Pass I32 value to F32 parameter
        let v1 = Value::new(1);
        func.dfg.set_value_type(v1, Type::I32);
        let inst_data = InstData::jump(block1, vec![v1]);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block0);

        let mut errors = Vec::new();
        verify_types(&func, &mut errors, None);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("expects type f32")));
    }

    #[test]
    fn test_verify_types_function_call_type_mismatch() {
        let mut module = crate::Module::new();

        // Create callee function with i32 parameter
        let callee_sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let callee_func = Function::new(callee_sig, String::from("callee"));
        module.add_function(String::from("callee"), callee_func);

        // Create caller function
        let caller_sig = Signature::empty();
        let mut caller_func = Function::new(caller_sig, String::from("caller"));
        let block = caller_func.create_block();
        caller_func.append_block(block);

        // Call with f32 argument (wrong type)
        let v1 = Value::new(1);
        caller_func.dfg.set_value_type(v1, Type::F32);
        let v2 = Value::new(2);
        caller_func.dfg.set_value_type(v2, Type::I32);
        let inst_data = InstData::call(String::from("callee"), vec![v1], vec![v2]);
        let inst = caller_func.create_inst(inst_data);
        caller_func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_types(&caller_func, &mut errors, Some(&module));
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("expects type i32")));
    }
}
