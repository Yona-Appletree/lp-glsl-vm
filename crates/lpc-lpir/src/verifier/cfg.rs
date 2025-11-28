//! CFG (Control Flow Graph) verification.

use alloc::{collections::BTreeSet, format, vec::Vec};

use crate::{analysis::ControlFlowGraph, Function, VerifierError};

/// Verify CFG integrity
///
/// Checks:
/// - All block references in control flow instructions are valid
/// - Block parameters match arguments passed to them
/// - Entry block cannot be branched to (no predecessors)
/// - Instruction-block consistency (inst_block matches block_insts iterator)
/// - CFG-predecessor consistency (all CFG predecessors have branches)
/// - Branch-CFG consistency (all branches are in CFG)
pub fn verify_cfg(function: &Function, errors: &mut Vec<VerifierError>) {
    verify_block_references(function, errors);
    verify_block_arguments(function, errors);
    verify_entry_block(function, errors);
    verify_instruction_block_consistency(function, errors);
    verify_cfg_consistency(function, errors);
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

/// Verify entry block cannot be branched to
fn verify_entry_block(function: &Function, errors: &mut Vec<VerifierError>) {
    if let Some(entry_block) = function.entry_block() {
        // Check all blocks for branches to entry block
        for block in function.blocks() {
            if block == entry_block {
                continue; // Skip entry block itself
            }

            for inst in function.block_insts(block) {
                if let Some(inst_data) = function.dfg.inst_data(inst) {
                    if let Some(block_args) = &inst_data.block_args {
                        for (target_block, _args) in &block_args.targets {
                            if *target_block == entry_block {
                                errors.push(VerifierError::with_location(
                                    format!(
                                        "Entry block {} cannot be branched to",
                                        entry_block.index()
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

/// Verify instruction-block consistency
///
/// Checks that inst_block() matches block_insts() iterator:
/// - All instructions in block_insts(block) have inst_block() == block
fn verify_instruction_block_consistency(function: &Function, errors: &mut Vec<VerifierError>) {
    for block in function.blocks() {
        for inst in function.block_insts(block) {
            if let Some(inst_block) = function.layout.inst_block(inst) {
                if inst_block != block {
                    errors.push(VerifierError::with_location(
                        format!(
                            "Instruction {} is in block_insts({}) but inst_block() returns {}",
                            inst.index(),
                            block.index(),
                            inst_block.index()
                        ),
                        format!("inst{}", inst.index()),
                    ));
                }
            } else {
                errors.push(VerifierError::with_location(
                    format!(
                        "Instruction {} is in block_insts({}) but inst_block() returns None",
                        inst.index(),
                        block.index()
                    ),
                    format!("inst{}", inst.index()),
                ));
            }
        }
    }
}

/// Verify CFG consistency
///
/// Checks:
/// - All CFG predecessors have branches to the block
/// - All branches are present in CFG
fn verify_cfg_consistency(function: &Function, errors: &mut Vec<VerifierError>) {
    let cfg = ControlFlowGraph::from_function(function);

    // Map blocks to indices
    let block_to_index: BTreeSet<_> = function.blocks().enumerate().collect();

    // Check that all CFG predecessors have actual branches
    for (block_idx, block) in function.blocks().enumerate() {
        let cfg_preds = cfg.predecessors(block_idx);
        let mut actual_preds = BTreeSet::new();

        // Find actual predecessors by examining branches
        for pred_block in function.blocks() {
            for inst in function.block_insts(pred_block) {
                if let Some(inst_data) = function.dfg.inst_data(inst) {
                    if let Some(block_args) = &inst_data.block_args {
                        for (target_block, _args) in &block_args.targets {
                            if *target_block == block {
                                if let Some(&pred_idx) = block_to_index
                                    .iter()
                                    .find(|(_, b)| *b == pred_block)
                                    .map(|(idx, _)| idx)
                                {
                                    actual_preds.insert(pred_idx);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check that CFG predecessors match actual branches
        for &cfg_pred_idx in cfg_preds {
            if !actual_preds.contains(&cfg_pred_idx) {
                errors.push(VerifierError::with_location(
                    format!(
                        "CFG shows block{} as predecessor of block{}, but no branch found",
                        cfg_pred_idx, block_idx
                    ),
                    format!("block{}", block.index()),
                ));
            }
        }

        // Check that actual branches are in CFG
        for &actual_pred_idx in &actual_preds {
            if !cfg_preds.contains(&actual_pred_idx) {
                errors.push(VerifierError::with_location(
                    format!(
                        "Branch from block{} to block{} exists but not in CFG",
                        actual_pred_idx, block_idx
                    ),
                    format!("block{}", block.index()),
                ));
            }
        }
    }
}
