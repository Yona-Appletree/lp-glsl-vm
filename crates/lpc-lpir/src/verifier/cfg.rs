//! CFG (Control Flow Graph) verification.

use alloc::{collections::BTreeSet, format, vec::Vec};

use crate::{Function, VerifierError};

/// Verify CFG integrity
///
/// Checks:
/// - All block references in control flow instructions are valid
/// - Block parameters match arguments passed to them
pub fn verify_cfg(function: &Function, errors: &mut Vec<VerifierError>) {
    verify_block_references(function, errors);
    verify_block_arguments(function, errors);
}

/// Verify that all block references in control flow instructions are valid
fn verify_block_references(function: &Function, errors: &mut Vec<VerifierError>) {
    let valid_blocks: BTreeSet<_> = function.blocks().collect();

    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                if let Some(block_args) = &inst_data.block_args {
                    for (target_block, _args) in &block_args.targets {
                        if !valid_blocks.contains(target_block) {
                            errors.push(VerifierError::with_location(
                                format!("Instruction references invalid block {}", target_block),
                                format!("inst{}", inst.index()),
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// Verify that block arguments match block parameters
fn verify_block_arguments(function: &Function, errors: &mut Vec<VerifierError>) {
    for block in function.blocks() {
        let block_data = function.block_data(block);
        let expected_param_count = block_data.map(|b| b.params.len()).unwrap_or(0);

        // Check all incoming edges
        for pred_block in function.blocks() {
            for inst in function.block_insts(pred_block) {
                if let Some(inst_data) = function.dfg.inst_data(inst) {
                    if let Some(block_args) = &inst_data.block_args {
                        for (target_block, args) in &block_args.targets {
                            if *target_block == block {
                                if args.len() != expected_param_count {
                                    errors.push(VerifierError::with_location(
                                        format!(
                                            "Block {} expects {} parameters, but {} arguments \
                                             provided",
                                            block,
                                            expected_param_count,
                                            args.len()
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

#[cfg(test)]
mod tests {
    use alloc::{string::String, vec, vec::Vec};

    use super::*;
    use crate::{dfg::InstData, entity::Block, signature::Signature, value::Value};

    #[test]
    fn test_verify_cfg_valid() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        let block1 = func.create_block();
        func.append_block(block0);
        func.append_block(block1);

        let inst_data = InstData::jump(block1, Vec::new());
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block0);

        let mut errors = Vec::new();
        verify_cfg(&func, &mut errors);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_verify_cfg_invalid_block_reference() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        func.append_block(block0);

        // Reference a block that doesn't exist
        let invalid_block = Block::new(999);
        let inst_data = InstData::jump(invalid_block, Vec::new());
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block0);

        let mut errors = Vec::new();
        verify_cfg(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("invalid block"));
    }

    #[test]
    fn test_verify_cfg_block_argument_mismatch() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        let block1 = func.create_block_with_params(vec![Value::new(0)]);
        func.append_block(block0);
        func.append_block(block1);

        // Jump with wrong number of arguments (0 instead of 1)
        let inst_data = InstData::jump(block1, Vec::new());
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block0);

        let mut errors = Vec::new();
        verify_cfg(&func, &mut errors);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("expects"));
    }
}
