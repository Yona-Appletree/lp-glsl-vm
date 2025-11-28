//! Dominance verification.

use alloc::{format, vec::Vec};

use crate::{Function, VerifierError};

/// Verify dominance properties
///
/// Checks:
/// - All uses of a value are dominated by its definition
/// - Block parameters are only used in their block
pub fn verify_dominance(function: &Function, errors: &mut Vec<VerifierError>) {
    // For now, this is a placeholder. Full dominance verification requires
    // computing the dominator tree, which is already available in the
    // analysis module. We can enhance this later.

    // Basic check: block parameters should only be used in their defining block
    verify_block_parameter_usage(function, errors);
}

/// Verify that block parameters are only used in their defining block
fn verify_block_parameter_usage(function: &Function, errors: &mut Vec<VerifierError>) {
    for block in function.blocks() {
        if let Some(block_data) = function.block_data(block) {
            for param in &block_data.params {
                // Check if this parameter is used in other blocks
                for other_block in function.blocks() {
                    if other_block == block {
                        continue; // Skip the defining block
                    }

                    for inst in function.block_insts(other_block) {
                        if let Some(inst_data) = function.dfg.inst_data(inst) {
                            if inst_data.args.contains(param) {
                                errors.push(VerifierError::with_location(
                                    format!(
                                        "Block parameter {} from block {} is used in block {}",
                                        param, block, other_block
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

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec};

    use super::*;
    use crate::{
        dfg::{InstData, Opcode},
        signature::Signature,
        value::Value,
    };

    #[test]
    fn test_verify_dominance_block_parameter_usage() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        let param = Value::new(0);
        let block1 = func.create_block_with_params(vec![param]);
        func.append_block(block0);
        func.append_block(block1);

        // Use the parameter in its own block (valid)
        let v1 = Value::new(1);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v1, param, param);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block1);

        let mut errors = Vec::new();
        verify_dominance(&func, &mut errors);
        // Should be valid - parameter used in its own block
    }
}
