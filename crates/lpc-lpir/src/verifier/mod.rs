//! IR verifier.

use alloc::{format, string::String, vec::Vec};

use crate::Function;

mod cfg;
mod dominance;
mod entities;
mod format;
mod ssa;
mod types;

pub use cfg::verify_cfg;
pub use dominance::verify_dominance;
pub use entities::verify_entities;
pub use format::verify_format;
pub use ssa::verify_ssa;
pub use types::verify_types;

// verify_module is defined below

/// Verifier error
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierError {
    /// Error message describing what's wrong
    pub message: String,
    /// Optional location information (e.g., "block0", "inst5")
    pub location: Option<String>,
}

impl VerifierError {
    /// Create a new verifier error
    pub fn new(message: String) -> Self {
        Self {
            message,
            location: None,
        }
    }

    /// Create a new verifier error with location
    pub fn with_location(message: String, location: String) -> Self {
        Self {
            message,
            location: Some(location),
        }
    }
}

/// Verify a function is well-formed
///
/// This runs all verification checks and returns a list of errors if any are found.
/// Returns `Ok(())` if the function is valid, or `Err(errors)` with a list of
/// verification errors.
///
/// # Arguments
///
/// * `function` - The function to verify
/// * `module` - Optional module context for function call validation
pub fn verify(
    function: &Function,
    module: Option<&crate::Module>,
) -> Result<(), Vec<VerifierError>> {
    let mut errors = Vec::new();

    // Run all checks
    verify_format(function, &mut errors);
    verify_entities(function, &mut errors, module);
    verify_cfg(function, &mut errors);
    verify_ssa(function, &mut errors);
    verify_dominance(function, &mut errors);
    verify_types(function, &mut errors, module);
    verify_terminators(function, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Verify a module (cross-function checks)
///
/// This performs verification checks that require module-level context,
/// such as verifying that call instructions match callee signatures.
pub fn verify_module(module: &crate::Module) -> Result<(), Vec<VerifierError>> {
    let mut errors = Vec::new();

    // Verify each function
    for func in module.functions.values() {
        if let Err(func_errors) = verify(func, Some(module)) {
            errors.extend(func_errors);
        }
    }

    // Verify call instructions match callee signatures
    for func in module.functions.values() {
        for block in func.blocks() {
            for inst in func.block_insts(block) {
                if let Some(inst_data) = func.dfg.inst_data(inst) {
                    if let crate::dfg::Opcode::Call { callee } = &inst_data.opcode {
                        let callee_func = match module.get_function(callee) {
                            Some(f) => f,
                            None => {
                                // Function existence is checked by entity validation
                                continue;
                            }
                        };

                        // Verify argument count
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
                        }

                        // Verify result count
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
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Verify that all blocks have proper terminators
fn verify_terminators(function: &Function, errors: &mut Vec<VerifierError>) {
    for block in function.blocks() {
        let has_terminator = function.block_insts(block).any(|inst| {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                matches!(
                    inst_data.opcode,
                    crate::dfg::Opcode::Jump
                        | crate::dfg::Opcode::Br
                        | crate::dfg::Opcode::Return
                        | crate::dfg::Opcode::Halt
                )
            } else {
                false
            }
        });

        if !has_terminator {
            errors.push(VerifierError::with_location(
                format!("Block {} has no terminator", block),
                format!("block{}", block.index()),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::{dfg::InstData, signature::Signature, types::Type, value::Value};

    #[test]
    fn test_verify_valid_function() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let inst_data = InstData::halt();
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let result = verify(&func, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_missing_terminator() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        // Block has no instructions, so no terminator
        let result = verify(&func, None);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("no terminator"));
    }

    #[test]
    fn test_verifier_error_creation() {
        let error = VerifierError::new(String::from("test error"));
        assert_eq!(error.message, "test error");
        assert_eq!(error.location, None);

        let error_with_loc =
            VerifierError::with_location(String::from("test error"), String::from("block0"));
        assert_eq!(error_with_loc.message, "test error");
        assert_eq!(error_with_loc.location, Some(String::from("block0")));
    }

    #[test]
    fn test_verify_module_call_correct_signature() {
        let mut module = crate::Module::new();

        // Create callee function with terminator and entry block
        let callee_sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let mut callee_func = Function::new(callee_sig, String::from("callee"));
        let callee_entry = callee_func.create_block_with_params(vec![Value::new(0)]);
        callee_func.append_block(callee_entry);
        if let Some(block_data) = callee_func.block_data_mut(callee_entry) {
            block_data.param_types = vec![Type::I32];
        }
        callee_func.dfg.set_value_type(Value::new(0), Type::I32);
        let callee_return = InstData::return_(vec![Value::new(0)]);
        let callee_return_inst = callee_func.create_inst(callee_return);
        callee_func.append_inst(callee_return_inst, callee_entry);
        module.add_function(String::from("callee"), callee_func);

        // Create caller function with entry block
        let caller_sig = Signature::empty();
        let mut caller_func = Function::new(caller_sig, String::from("caller"));
        let block = caller_func.create_block();
        caller_func.append_block(block);

        // Define values before using them
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        caller_func.dfg.set_value_type(v1, Type::I32);
        caller_func.dfg.set_value_type(v2, Type::I32);
        // Create a constant to define v1
        let const_inst_data = InstData::constant(v1, crate::dfg::Immediate::I64(42));
        let const_inst = caller_func.create_inst(const_inst_data);
        caller_func.append_inst(const_inst, block);
        // v2 will be the result of the call
        let inst_data = InstData::call(String::from("callee"), vec![v1], vec![v2]);
        let inst = caller_func.create_inst(inst_data);
        caller_func.append_inst(inst, block);
        let halt_inst = caller_func.create_inst(InstData::halt());
        caller_func.append_inst(halt_inst, block);

        module.add_function(String::from("caller"), caller_func);

        let result = verify_module(&module);
        assert!(result.is_ok(), "Call with correct signature should pass");
    }

    #[test]
    fn test_verify_module_call_wrong_arg_count() {
        let mut module = crate::Module::new();

        // Create callee function with 2 args and terminator and entry block
        let callee_sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
        let mut callee_func = Function::new(callee_sig, String::from("callee"));
        let callee_entry = callee_func.create_block_with_params(vec![Value::new(0), Value::new(1)]);
        callee_func.append_block(callee_entry);
        if let Some(block_data) = callee_func.block_data_mut(callee_entry) {
            block_data.param_types = vec![Type::I32, Type::I32];
        }
        callee_func.dfg.set_value_type(Value::new(0), Type::I32);
        let callee_return = InstData::return_(vec![Value::new(0)]);
        let callee_return_inst = callee_func.create_inst(callee_return);
        callee_func.append_inst(callee_return_inst, callee_entry);
        module.add_function(String::from("callee"), callee_func);

        // Create caller function with wrong arg count
        let caller_sig = Signature::empty();
        let mut caller_func = Function::new(caller_sig, String::from("caller"));
        let block = caller_func.create_block();
        caller_func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        caller_func.dfg.set_value_type(v1, Type::I32);
        caller_func.dfg.set_value_type(v2, Type::I32);
        // Call with only 1 arg instead of 2
        let inst_data = InstData::call(String::from("callee"), vec![v1], vec![v2]);
        let inst = caller_func.create_inst(inst_data);
        caller_func.append_inst(inst, block);
        let halt_inst = caller_func.create_inst(InstData::halt());
        caller_func.append_inst(halt_inst, block);

        module.add_function(String::from("caller"), caller_func);

        let result = verify_module(&module);
        assert!(result.is_err(), "Call with wrong arg count should fail");
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("expects 2 arguments")));
    }

    #[test]
    fn test_verify_module_call_wrong_result_count() {
        let mut module = crate::Module::new();

        // Create callee function with 2 results and terminator and entry block
        let callee_sig = Signature::new(vec![Type::I32], vec![Type::I32, Type::F32]);
        let mut callee_func = Function::new(callee_sig, String::from("callee"));
        let callee_entry = callee_func.create_block_with_params(vec![Value::new(0)]);
        callee_func.append_block(callee_entry);
        if let Some(block_data) = callee_func.block_data_mut(callee_entry) {
            block_data.param_types = vec![Type::I32];
        }
        let callee_ret1 = Value::new(1);
        let callee_ret2 = Value::new(2);
        callee_func.dfg.set_value_type(callee_ret1, Type::I32);
        callee_func.dfg.set_value_type(callee_ret2, Type::F32);
        let callee_return = InstData::return_(vec![callee_ret1, callee_ret2]);
        let callee_return_inst = callee_func.create_inst(callee_return);
        callee_func.append_inst(callee_return_inst, callee_entry);
        module.add_function(String::from("callee"), callee_func);

        // Create caller function with wrong result count
        let caller_sig = Signature::empty();
        let mut caller_func = Function::new(caller_sig, String::from("caller"));
        let block = caller_func.create_block();
        caller_func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        caller_func.dfg.set_value_type(v1, Type::I32);
        caller_func.dfg.set_value_type(v2, Type::I32);
        // Call with only 1 result instead of 2
        let inst_data = InstData::call(String::from("callee"), vec![v1], vec![v2]);
        let inst = caller_func.create_inst(inst_data);
        caller_func.append_inst(inst, block);
        let halt_inst = caller_func.create_inst(InstData::halt());
        caller_func.append_inst(halt_inst, block);

        module.add_function(String::from("caller"), caller_func);

        let result = verify_module(&module);
        assert!(result.is_err(), "Call with wrong result count should fail");
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("returns 2 values")));
    }

    #[test]
    fn test_verify_module_call_multi_return() {
        let mut module = crate::Module::new();

        // Create callee function with 3+ returns and terminator and entry block
        let callee_sig = Signature::new(vec![Type::I32], vec![Type::I32, Type::F32, Type::I32]);
        let mut callee_func = Function::new(callee_sig, String::from("callee"));
        let callee_entry = callee_func.create_block_with_params(vec![Value::new(0)]);
        callee_func.append_block(callee_entry);
        if let Some(block_data) = callee_func.block_data_mut(callee_entry) {
            block_data.param_types = vec![Type::I32];
        }
        // Define return values before returning them
        let callee_ret1 = Value::new(1);
        let callee_ret2 = Value::new(2);
        let callee_ret3 = Value::new(3);
        callee_func.dfg.set_value_type(callee_ret1, Type::I32);
        callee_func.dfg.set_value_type(callee_ret2, Type::F32);
        callee_func.dfg.set_value_type(callee_ret3, Type::I32);
        // Create constants to define the return values
        let const1 = InstData::constant(callee_ret1, crate::dfg::Immediate::I64(1));
        let const1_inst = callee_func.create_inst(const1);
        callee_func.append_inst(const1_inst, callee_entry);
        let const2 = InstData::constant(callee_ret2, crate::dfg::Immediate::F32Bits(0x40000000)); // 2.0
        let const2_inst = callee_func.create_inst(const2);
        callee_func.append_inst(const2_inst, callee_entry);
        let const3 = InstData::constant(callee_ret3, crate::dfg::Immediate::I64(3));
        let const3_inst = callee_func.create_inst(const3);
        callee_func.append_inst(const3_inst, callee_entry);
        let callee_return = InstData::return_(vec![callee_ret1, callee_ret2, callee_ret3]);
        let callee_return_inst = callee_func.create_inst(callee_return);
        callee_func.append_inst(callee_return_inst, callee_entry);
        module.add_function(String::from("callee"), callee_func);

        // Create caller function with entry block
        let caller_sig = Signature::empty();
        let mut caller_func = Function::new(caller_sig, String::from("caller"));
        let block = caller_func.create_block();
        caller_func.append_block(block);

        // Define values before using them
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let v4 = Value::new(4);
        caller_func.dfg.set_value_type(v1, Type::I32);
        caller_func.dfg.set_value_type(v2, Type::I32);
        caller_func.dfg.set_value_type(v3, Type::F32);
        caller_func.dfg.set_value_type(v4, Type::I32);
        // Create a constant to define v1
        let const_inst_data = InstData::constant(v1, crate::dfg::Immediate::I64(42));
        let const_inst = caller_func.create_inst(const_inst_data);
        caller_func.append_inst(const_inst, block);
        // v2, v3, v4 will be the results of the call
        let inst_data = InstData::call(String::from("callee"), vec![v1], vec![v2, v3, v4]);
        let inst = caller_func.create_inst(inst_data);
        caller_func.append_inst(inst, block);
        let halt_inst = caller_func.create_inst(InstData::halt());
        caller_func.append_inst(halt_inst, block);

        module.add_function(String::from("caller"), caller_func);

        let result = verify_module(&module);
        assert!(
            result.is_ok(),
            "Call with 3+ results (multi-return) should pass"
        );
    }

    #[test]
    fn test_verify_module_multiple_functions() {
        let mut module = crate::Module::new();

        // Create multiple functions with terminators and entry blocks
        let func1_sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let mut func1 = Function::new(func1_sig, String::from("func1"));
        let entry1 = func1.create_block_with_params(vec![Value::new(0)]);
        func1.append_block(entry1);
        if let Some(block_data) = func1.block_data_mut(entry1) {
            block_data.param_types = vec![Type::I32];
        }
        func1.dfg.set_value_type(Value::new(0), Type::I32);
        let return1 = InstData::return_(vec![Value::new(0)]);
        let return1_inst = func1.create_inst(return1);
        func1.append_inst(return1_inst, entry1);
        module.add_function(String::from("func1"), func1);

        let func2_sig = Signature::new(vec![Type::F32], vec![Type::F32]);
        let mut func2 = Function::new(func2_sig, String::from("func2"));
        let entry2 = func2.create_block_with_params(vec![Value::new(0)]);
        func2.append_block(entry2);
        if let Some(block_data) = func2.block_data_mut(entry2) {
            block_data.param_types = vec![Type::F32];
        }
        func2.dfg.set_value_type(Value::new(0), Type::F32);
        let return2 = InstData::return_(vec![Value::new(0)]);
        let return2_inst = func2.create_inst(return2);
        func2.append_inst(return2_inst, entry2);
        module.add_function(String::from("func2"), func2);

        let result = verify_module(&module);
        assert!(result.is_ok(), "Module with multiple functions should pass");
    }
}
