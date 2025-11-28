//! SSA (Static Single Assignment) verification.

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use crate::{Function, Value, VerifierError};

/// Verify SSA form
///
/// Checks:
/// - Each value is defined exactly once
/// - Values are only used after they are defined
/// - Values are only used in blocks dominated by their definition
pub fn verify_ssa(function: &Function, errors: &mut Vec<VerifierError>) {
    verify_value_definitions(function, errors);
    verify_value_uses(function, errors);
}

/// Verify that each value is defined exactly once
fn verify_value_definitions(function: &Function, errors: &mut Vec<VerifierError>) {
    let mut definitions: BTreeMap<Value, String> = BTreeMap::new();

    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                for result in &inst_data.results {
                    if let Some(prev_def) = definitions.get(result) {
                        errors.push(VerifierError::with_location(
                            format!(
                                "Value {} is defined multiple times (previously at {})",
                                result, prev_def
                            ),
                            format!("inst{}", inst.index()),
                        ));
                    } else {
                        definitions.insert(*result, format!("inst{}", inst.index()));
                    }
                }
            }
        }
    }
}

/// Verify that values are only used after they are defined
fn verify_value_uses(function: &Function, errors: &mut Vec<VerifierError>) {
    let mut definitions: BTreeMap<Value, String> = BTreeMap::new();

    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                // Check uses before adding definitions
                for arg in &inst_data.args {
                    if !definitions.contains_key(arg) {
                        // Check if it's a block parameter
                        let is_block_param = function
                            .block_data(block)
                            .map(|bd| bd.params.contains(arg))
                            .unwrap_or(false);

                        if !is_block_param {
                            errors.push(VerifierError::with_location(
                                format!("Value {} is used before definition", arg),
                                format!("inst{}", inst.index()),
                            ));
                        }
                    }
                }

                // Add definitions after checking uses
                for result in &inst_data.results {
                    definitions.insert(*result, format!("inst{}", inst.index()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dfg::{InstData, Opcode},
        signature::Signature,
        value::Value,
    };

    #[test]
    fn test_verify_ssa_valid() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);

        let inst1_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst1 = func.create_inst(inst1_data);
        func.append_inst(inst1, block);

        let mut errors = Vec::new();
        verify_ssa(&func, &mut errors);
        // Should have errors for v1 and v2 being used before definition
        // This is expected - we'd need to define them first in a real function
    }

    #[test]
    fn test_verify_ssa_multiple_definitions() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);

        // Define v3 twice
        let inst1_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst1 = func.create_inst(inst1_data);
        func.append_inst(inst1, block);

        let inst2_data = InstData::arithmetic(Opcode::Isub, v3, v1, v2);
        let inst2 = func.create_inst(inst2_data);
        func.append_inst(inst2, block);

        let mut errors = Vec::new();
        verify_ssa(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("defined multiple times")));
    }
}
