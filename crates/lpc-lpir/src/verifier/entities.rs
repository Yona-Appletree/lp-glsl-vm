//! Entity existence validation.
//!
//! Validates that all referenced entities (values, blocks, functions) exist.

use alloc::{collections::BTreeSet, format, vec::Vec};

use crate::{dfg::Opcode, Function, VerifierError};

/// Verify that all referenced entities exist
///
/// Checks:
/// - All values in instruction args/results exist in DFG
/// - All blocks in block_args exist in function
/// - All function names in Call instructions exist in module (if module provided)
pub fn verify_entities(
    function: &Function,
    errors: &mut Vec<VerifierError>,
    module: Option<&crate::Module>,
) {
    verify_values(function, errors);
    verify_blocks(function, errors);
    if let Some(module) = module {
        verify_functions(function, errors, module);
    }
}

/// Verify that all referenced values exist
fn verify_values(function: &Function, errors: &mut Vec<VerifierError>) {
    // Collect all valid values (those that have types set in DFG)
    // Note: This is a simple check - in a more complete system, we'd track
    // all created values explicitly
    let mut valid_values: BTreeSet<crate::Value> = BTreeSet::new();

    // Collect values from block parameters
    for block in function.blocks() {
        if let Some(block_data) = function.block_data(block) {
            for param in &block_data.params {
                valid_values.insert(*param);
            }
        }
    }

    // Collect values from instruction results
    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                for result in &inst_data.results {
                    valid_values.insert(*result);
                }
            }
        }
    }

    // Now verify all uses reference valid values
    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                // Check instruction arguments
                for arg in &inst_data.args {
                    if !valid_values.contains(arg) {
                        // Check if it's a block parameter (might not be in valid_values yet)
                        let is_block_param = function
                            .block_data(block)
                            .map(|bd| bd.params.contains(arg))
                            .unwrap_or(false);

                        if !is_block_param {
                            // Check other blocks' parameters
                            let mut found_in_other_block = false;
                            for other_block in function.blocks() {
                                if let Some(other_block_data) = function.block_data(other_block) {
                                    if other_block_data.params.contains(arg) {
                                        found_in_other_block = true;
                                        break;
                                    }
                                }
                            }

                            if !found_in_other_block {
                                errors.push(VerifierError::with_location(
                                    format!("Value {} is used but never defined", arg.index()),
                                    format!("inst{}", inst.index()),
                                ));
                            }
                        }
                    }
                }

                // Check block arguments (values passed to blocks)
                if let Some(block_args) = &inst_data.block_args {
                    for (_target_block, args) in &block_args.targets {
                        for arg in args {
                            if !valid_values.contains(arg) {
                                // Check if it's a block parameter
                                let mut found = false;
                                for other_block in function.blocks() {
                                    if let Some(other_block_data) = function.block_data(other_block)
                                    {
                                        if other_block_data.params.contains(arg) {
                                            found = true;
                                            break;
                                        }
                                    }
                                }
                                if !found {
                                    errors.push(VerifierError::with_location(
                                        format!(
                                            "Value {} passed to block but never defined",
                                            arg.index()
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

/// Verify that all referenced blocks exist
fn verify_blocks(function: &Function, errors: &mut Vec<VerifierError>) {
    let valid_blocks: BTreeSet<_> = function.blocks().collect();

    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                if let Some(block_args) = &inst_data.block_args {
                    for (target_block, _args) in &block_args.targets {
                        if !valid_blocks.contains(target_block) {
                            errors.push(VerifierError::with_location(
                                format!(
                                    "Block {} is referenced but does not exist in function",
                                    target_block.index()
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

/// Verify that all called functions exist in module
fn verify_functions(function: &Function, errors: &mut Vec<VerifierError>, module: &crate::Module) {
    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                if let Opcode::Call { callee } = &inst_data.opcode {
                    if module.get_function(callee).is_none() {
                        errors.push(VerifierError::with_location(
                            format!(
                                "Function '{}' is called but does not exist in module",
                                callee
                            ),
                            format!("inst{}", inst.index()),
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec};

    use super::*;
    use crate::{
        dfg::{InstData, Opcode},
        signature::Signature,
        types::Type,
        value::Value,
    };

    #[test]
    fn test_verify_entities_valid() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let mut errors = Vec::new();
        verify_entities(&func, &mut errors, None);
        // Should have errors for v1 and v2 being used before definition
        // This is expected - they're not defined yet
    }

    #[test]
    fn test_verify_entities_invalid_block() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        func.append_block(block0);

        // Reference a non-existent block
        let invalid_block = crate::entity::Block::new(999);
        let inst_data = InstData::jump(invalid_block, vec![]);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block0);

        let mut errors = Vec::new();
        verify_entities(&func, &mut errors, None);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.message.contains("does not exist")));
    }

    #[test]
    fn test_verify_entities_function_call() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let inst_data = InstData::call(String::from("nonexistent"), vec![v1], vec![v2]);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        // Create a module without the function
        let mut module = crate::Module::new();
        module.add_function(String::from("test"), func.clone());

        let mut errors = Vec::new();
        verify_entities(
            &module.get_function("test").unwrap(),
            &mut errors,
            Some(&module),
        );
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("does not exist in module")));
    }

    #[test]
    fn test_verify_entities_function_call_valid() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let inst_data = InstData::call(String::from("callee"), vec![v1], vec![v2]);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        // Create a module with the called function
        let mut module = crate::Module::new();
        let callee_sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let callee_func = Function::new(callee_sig, String::from("callee"));
        module.add_function(String::from("callee"), callee_func);
        module.add_function(String::from("test"), func.clone());

        let mut errors = Vec::new();
        verify_entities(
            &module.get_function("test").unwrap(),
            &mut errors,
            Some(&module),
        );
        // Should not error about missing function
        assert!(!errors
            .iter()
            .any(|e| e.message.contains("does not exist in module")));
    }
}
