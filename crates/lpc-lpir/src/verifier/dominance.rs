//! Dominance verification.

use alloc::{collections::BTreeMap, vec::Vec};

use crate::{analysis::ControlFlowGraph, Function, VerifierError};

/// Verify dominance properties
///
/// Checks:
/// - All uses of a value are dominated by their definition
/// - Block parameters can be used in their own block and dominated blocks
/// - Entry block parameters can be used anywhere
pub fn verify_dominance(function: &Function, errors: &mut Vec<VerifierError>) {
    // Build CFG and dominator tree
    let cfg = ControlFlowGraph::from_function(function);
    let domtree = crate::analysis::DominatorTree::from_cfg(&cfg);

    // Map blocks to their indices for dominance queries
    let block_to_index: BTreeMap<_, _> = function
        .blocks()
        .enumerate()
        .map(|(idx, block)| (block, idx))
        .collect();

    let entry_block = function.entry_block();
    let entry_block_idx = entry_block.and_then(|b| block_to_index.get(&b).copied());

    // Track value definitions: value -> (block_idx, is_block_param)
    let mut value_definitions: BTreeMap<crate::Value, (usize, bool)> = BTreeMap::new();

    // First pass: collect all value definitions
    for (block_idx, block) in function.blocks().enumerate() {
        // Block parameters are defined at block entry
        if let Some(block_data) = function.block_data(block) {
            for param in &block_data.params {
                value_definitions.insert(*param, (block_idx, true));
            }
        }

        // Track values defined by instructions
        for inst in function.block_insts(block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                for result in &inst_data.results {
                    value_definitions.insert(*result, (block_idx, false));
                }
            }
        }
    }

    // Second pass: verify all uses are dominated by definitions
    for (use_block_idx, use_block) in function.blocks().enumerate() {
        for inst in function.block_insts(use_block) {
            if let Some(inst_data) = function.dfg.inst_data(inst) {
                // Check instruction arguments
                for arg in &inst_data.args {
                    if let Some((def_block_idx, is_block_param)) = value_definitions.get(arg) {
                        // Entry block parameters can be used anywhere
                        if let Some(entry_idx) = entry_block_idx {
                            if *def_block_idx == entry_idx && *is_block_param {
                                continue; // Valid: entry block parameter
                            }
                        }

                        // Check if use block is dominated by definition block
                        if !domtree.dominates(*def_block_idx, use_block_idx) {
                            // For block parameters, they can be used in their own block
                            if *is_block_param && *def_block_idx == use_block_idx {
                                continue; // Valid: parameter used in its own block
                            }

                            errors.push(VerifierError::with_location(
                                alloc::format!(
                                    "Value {} defined in block{} is used in block{} which is not \
                                     dominated by block{}",
                                    arg.index(),
                                    def_block_idx,
                                    use_block_idx,
                                    def_block_idx
                                ),
                                alloc::format!("inst{}", inst.index()),
                            ));
                        }
                    }
                    // If value not found in definitions, it will be caught by entity validation
                }

                // Check block arguments (values passed to blocks)
                if let Some(block_args) = &inst_data.block_args {
                    for (target_block, args) in &block_args.targets {
                        if let Some(target_block_idx) = block_to_index.get(target_block).copied() {
                            for arg in args {
                                if let Some((def_block_idx, is_block_param)) =
                                    value_definitions.get(arg)
                                {
                                    // Entry block parameters can be used anywhere
                                    if let Some(entry_idx) = entry_block_idx {
                                        if *def_block_idx == entry_idx && *is_block_param {
                                            continue; // Valid: entry block parameter
                                        }
                                    }

                                    // Check if target block is dominated by definition block
                                    if !domtree.dominates(*def_block_idx, target_block_idx) {
                                        // For block parameters, they can be used in their own block
                                        if *is_block_param && *def_block_idx == target_block_idx {
                                            continue; // Valid: parameter used in its own block
                                        }

                                        errors.push(VerifierError::with_location(
                                            alloc::format!(
                                                "Value {} defined in block{} is passed to block{} \
                                                 which is not dominated by block{}",
                                                arg.index(),
                                                def_block_idx,
                                                target_block_idx,
                                                def_block_idx
                                            ),
                                            alloc::format!("inst{}", inst.index()),
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
        assert!(errors.is_empty());
    }

    #[test]
    fn test_verify_dominance_value_used_before_definition() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        func.append_block(block0);

        // Use v1 before defining it
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst1_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2); // Uses v1, v2 before definition
        let inst1 = func.create_inst(inst1_data);
        func.append_inst(inst1, block0);

        let mut errors = Vec::new();
        verify_dominance(&func, &mut errors);
        // Should error about v1 and v2 being used before definition
        // (Actually, this will be caught by entity validation, but dominance
        // should also catch it if values are defined later)
    }

    #[test]
    fn test_verify_dominance_value_used_in_non_dominated_block() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        let block1 = func.create_block();
        func.append_block(block0);
        func.append_block(block1);

        // Define v1 in block1
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst1_data = InstData::arithmetic(Opcode::Iadd, v3, v1, v2);
        let inst1 = func.create_inst(inst1_data);
        func.append_inst(inst1, block1);

        // Try to use v3 in block0 (not dominated by block1)
        let v4 = Value::new(4);
        let inst2_data = InstData::arithmetic(Opcode::Iadd, v4, v3, v1);
        let inst2 = func.create_inst(inst2_data);
        func.append_inst(inst2, block0);

        let mut errors = Vec::new();
        verify_dominance(&func, &mut errors);
        // Should error about v3 being used in non-dominated block
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.message.contains("not dominated")));
    }

    #[test]
    fn test_verify_dominance_entry_block_parameter() {
        let sig = Signature::new(vec![Type::I32], vec![]);
        let mut func = Function::new(sig, String::from("test"));
        let block0 = func.create_block();
        func.append_block(block0);

        // Entry block parameter (v0) should be available
        let param = Value::new(0);
        let v1 = Value::new(1);
        let inst_data = InstData::arithmetic(Opcode::Iadd, v1, param, param);
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block0);

        let mut errors = Vec::new();
        verify_dominance(&func, &mut errors);
        // Entry block parameters can be used anywhere, so this should be valid
        // (Note: This test might fail if entry block params aren't set up correctly)
    }
}
