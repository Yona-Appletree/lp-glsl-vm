//! Validation for parsed IR functions.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec,
    vec::Vec,
};

use crate::{
    analysis::{ControlFlowGraph, DominatorTree},
    dfg::Opcode,
    entity::Block,
    function::Function,
    value::Value,
};

/// Validate that block indices in jumps and branches are valid.
pub fn validate_block_indices(func: &Function) -> Result<(), String> {
    let valid_blocks: BTreeSet<Block> = func.blocks().collect();
    let num_blocks = valid_blocks.len();

    for block in func.blocks() {
        for inst in func.block_insts(block) {
            if let Some(inst_data) = func.dfg.inst_data(inst) {
                if let Some(block_args) = &inst_data.block_args {
                    for (target_block, _args) in &block_args.targets {
                        if !valid_blocks.contains(target_block) {
                            return Err(alloc::format!(
                                "Jump/branch to block{} but function only has {} blocks",
                                target_block.index(),
                                num_blocks
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Validate that jump/branch arguments match target block parameter counts.
pub fn validate_block_parameters(func: &Function) -> Result<(), String> {
    for block in func.blocks() {
        for inst in func.block_insts(block) {
            if let Some(inst_data) = func.dfg.inst_data(inst) {
                if let Some(block_args) = &inst_data.block_args {
                    for (target_block, args) in &block_args.targets {
                        let expected_param_count = func
                            .block_data(*target_block)
                            .map(|bd| bd.params.len())
                            .unwrap_or(0);
                        if args.len() != expected_param_count {
                            return Err(alloc::format!(
                                "Jump/branch to block{} expects {} parameters, but {} arguments \
                                 provided",
                                target_block.index(),
                                expected_param_count,
                                args.len()
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Validate that return instructions match function signature return count.
pub fn validate_return_values(func: &Function) -> Result<(), String> {
    let expected_return_count = func.signature.returns.len();

    for block in func.blocks() {
        for inst in func.block_insts(block) {
            if let Some(inst_data) = func.dfg.inst_data(inst) {
                if inst_data.opcode == Opcode::Return {
                    if inst_data.args.len() != expected_return_count {
                        return Err(alloc::format!(
                            "Return instruction in block{} returns {} values, but function \
                             signature expects {}",
                            block.index(),
                            inst_data.args.len(),
                            expected_return_count
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Validate that blocks end with terminating instructions.
pub fn validate_terminating_instructions(func: &Function) -> Result<(), String> {
    for block in func.blocks() {
        let insts: Vec<_> = func.block_insts(block).collect();
        if insts.is_empty() {
            return Err(alloc::format!(
                "Block{} is empty (must have at least one terminating instruction)",
                block.index()
            ));
        }

        if let Some(last_inst) = insts.last() {
            if let Some(inst_data) = func.dfg.inst_data(*last_inst) {
                match inst_data.opcode {
                    Opcode::Return | Opcode::Jump | Opcode::Br | Opcode::Halt => {
                        // Valid terminator
                    }
                    _ => {
                        return Err(alloc::format!(
                            "Block{} does not end with a terminating instruction \
                             (return/jump/branch/halt)",
                            block.index()
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Validate that entry block parameters match function signature.
pub fn validate_entry_block(func: &Function) -> Result<(), String> {
    if let Some(entry_block) = func.entry_block() {
        let expected_param_count = func.signature.params.len();
        let actual_param_count = func
            .block_data(entry_block)
            .map(|bd| bd.params.len())
            .unwrap_or(0);

        if actual_param_count != expected_param_count {
            return Err(alloc::format!(
                "Entry block has {} parameters, but function signature expects {}",
                actual_param_count,
                expected_param_count
            ));
        }
    }

    Ok(())
}

/// Validate that values are only used where they're dominated by their definition.
///
/// This implements CLIF-style dominance-based value scoping: values can be used
/// anywhere they're dominated by their definition, not just within their defining block.
pub fn validate_value_scoping(func: &Function) -> Result<(), String> {
    // Build CFG and compute dominance
    let cfg = ControlFlowGraph::from_function(func);
    let domtree = DominatorTree::from_cfg(&cfg);

    // Note: CFG/domtree use usize indices, which we get from enumerate() below

    // Track value definitions: value -> (block_idx, inst_idx)
    let mut value_definitions: BTreeMap<Value, (usize, usize)> = BTreeMap::new();

    // Track values defined in each block (for SSA property check)
    let mut block_definitions: Vec<BTreeSet<Value>> = vec![BTreeSet::new(); func.block_count()];

    // First pass: collect all value definitions
    for (block_idx, block) in func.blocks().enumerate() {
        // Block parameters are defined at block entry (inst 0)
        if let Some(block_data) = func.block_data(block) {
            for param in &block_data.params {
                if !block_definitions[block_idx].insert(*param) {
                    return Err(alloc::format!(
                        "Value {} defined multiple times in block{} (SSA violation)",
                        param.index(),
                        block_idx
                    ));
                }
                value_definitions.insert(*param, (block_idx, 0));
            }
        }

        // Track values defined by instructions
        for (inst_idx, inst) in func.block_insts(block).enumerate() {
            if let Some(inst_data) = func.dfg.inst_data(inst) {
                // Return instructions don't produce results (they use results field for return values)
                if inst_data.opcode != Opcode::Return {
                    for result in &inst_data.results {
                        if !block_definitions[block_idx].insert(*result) {
                            return Err(alloc::format!(
                                "Value {} defined multiple times in block{} (SSA violation)",
                                result.index(),
                                block_idx
                            ));
                        }
                        // inst_idx + 1 because 0 is reserved for block entry
                        value_definitions.insert(*result, (block_idx, inst_idx + 1));
                    }
                }
            }
        }
    }

    // Second pass: validate that all value uses are dominated by their definitions
    for (use_block_idx, block) in func.blocks().enumerate() {
        // Skip unreachable blocks (they can't execute anyway)
        if !cfg.is_reachable(use_block_idx) {
            continue;
        }

        for (inst_idx, inst) in func.block_insts(block).enumerate() {
            if let Some(inst_data) = func.dfg.inst_data(inst) {
                // Check all argument values used by this instruction
                for arg_value in &inst_data.args {
                    if let Some((def_block_idx, def_inst_idx)) = value_definitions.get(arg_value) {
                        // Check if value is defined in an unreachable block
                        if !cfg.is_reachable(*def_block_idx) {
                            return Err(alloc::format!(
                                "Value {} used in block{} but defined in unreachable block{}",
                                arg_value.index(),
                                use_block_idx,
                                def_block_idx
                            ));
                        }

                        // Check dominance: def_block must dominate use_block
                        if !domtree.dominates(*def_block_idx, use_block_idx) {
                            return Err(alloc::format!(
                                "Value {} used in block{} but defined in block{}. Value must be \
                                 dominated by its definition.",
                                arg_value.index(),
                                use_block_idx,
                                def_block_idx
                            ));
                        }

                        // Check that definition comes before use (within same block)
                        if *def_block_idx == use_block_idx && *def_inst_idx >= inst_idx + 1 {
                            return Err(alloc::format!(
                                "Value {} used before definition in block{} (instruction {})",
                                arg_value.index(),
                                use_block_idx,
                                inst_idx
                            ));
                        }
                    } else {
                        // Value not defined anywhere
                        return Err(alloc::format!(
                            "Value {} used in block{} but not defined anywhere",
                            arg_value.index(),
                            use_block_idx
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_function;

    #[test]
    fn test_validate_cross_block_usage_direct() {
        // Test validation directly with a manually constructed function
        // This should now PASS because block0 dominates block1 and block2
        use alloc::vec;

        use crate::{function::Function, signature::Signature, types::Type, value::Value};

        let mut func = Function::new(
            Signature {
                params: vec![Type::I32],
                returns: vec![Type::I32],
            },
            String::from("test"),
        );

        // block0: defines v1
        let block0 = func.create_block_with_params(vec![Value::new(0)]); // v0 parameter
        func.append_block(block0);
        let v1 = Value::new(1);
        let v0 = Value::new(0);
        let inst0 = func.create_inst(crate::dfg::InstData::constant(
            v1,
            crate::dfg::Immediate::I64(42),
        ));
        func.append_inst(inst0, block0);
        let block1_entity = func.create_block();
        func.append_block(block1_entity);
        let block2_entity = func.create_block();
        func.append_block(block2_entity);
        let inst1 = func.create_inst(crate::dfg::InstData::branch(
            v0,
            block1_entity,
            Vec::new(),
            block2_entity,
            Vec::new(),
        ));
        func.append_inst(inst1, block0);

        // block1: uses v1 (defined in block0) - should PASS (block0 dominates block1)
        let inst2 = func.create_inst(crate::dfg::InstData::return_(vec![v1]));
        func.append_inst(inst2, block1_entity);

        // block2: uses v1 (defined in block0) - should PASS (block0 dominates block2)
        let inst3 = func.create_inst(crate::dfg::InstData::return_(vec![v1]));
        func.append_inst(inst3, block2_entity);

        let result = validate_value_scoping(&func);
        assert!(
            result.is_ok(),
            "Should pass validation (block0 dominates block1 and block2), got: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_cross_block_usage() {
        // This should now PASS - CLIF-style cross-block usage is valid
        let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    brif v0, block1, block2

block1:
    return v1

block2:
    return v1
}"#;
        // Note: parse_function will call validate_value_scoping internally
        // This should now pass because block0 dominates block1 and block2
        let result = parse_function(input.trim());
        assert!(
            result.is_ok(),
            "Should pass validation (CLIF-style cross-block usage), got: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_dominance_violation() {
        // Test actual dominance violation: value defined in block that doesn't dominate use
        let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 42
    jump block3

block2:
    jump block3

block3:
    return v1
}"#;
        // block1 doesn't dominate block3 (path through block2 doesn't go through block1)
        let result = parse_function(input.trim());
        assert!(
            result.is_err(),
            "Should fail validation (dominance violation)"
        );
        let err = result.unwrap_err();
        assert!(
            err.message.contains("Value 1 used in block")
                && err.message.contains("but defined in block")
                && err.message.contains("dominated"),
            "Error should mention dominance violation: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_block_param_count_mismatch() {
        // Test that jump with wrong arg count fails
        let input = r#"function %test() -> i32 {
block0:
    v0 = iconst 42
    jump block1(v0)

block1(v1: i32, v2: i32):
    return v1
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Jump with wrong arg count should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("expects 2 parameters")
                && err.message.contains("1 arguments provided"),
            "Error should mention parameter count mismatch: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_block_index_invalid() {
        // Test that jump to non-existent block fails
        let input = r#"function %test() -> i32 {
block0:
    v0 = iconst 42
    jump block5
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Jump to non-existent block should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("block5") && err.message.contains("only has 1 blocks"),
            "Error should mention invalid block index: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_return_count_mismatch() {
        // Test that return with wrong value count fails
        let input = r#"function %test() -> i32, i32 {
block0:
    v0 = iconst 42
    return v0
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Return with wrong value count should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("returns 1 values") && err.message.contains("expects 2"),
            "Error should mention return count mismatch: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_value_from_unreachable_block() {
        // Test that values defined in unreachable blocks cannot be used in reachable blocks
        let input = r#"function %test() -> i32 {
block0:
    return v0

block1:
    v0 = iconst 42
    return v0
}"#;
        let result = parse_function(input.trim());
        assert!(
            result.is_err(),
            "Using value from unreachable block should fail"
        );
        let err = result.unwrap_err();
        assert!(
            err.message.contains("unreachable block")
                || (err.message.contains("Value 0") && err.message.contains("not defined")),
            "Error should mention unreachable block or undefined value: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_value_from_unreachable_block_explicit() {
        // Test explicit case: value defined in unreachable block, used in reachable block
        use alloc::vec;

        use crate::{function::Function, signature::Signature, types::Type, value::Value};

        let mut func = Function::new(
            Signature {
                params: Vec::new(),
                returns: vec![Type::I32],
            },
            String::from("test"),
        );

        // block0: reachable, uses v1
        let block0 = func.create_block();
        func.append_block(block0);
        let v1 = Value::new(1);
        let inst0 = func.create_inst(crate::dfg::InstData::return_(vec![v1]));
        func.append_inst(inst0, block0);

        // block1: unreachable, defines v1
        let block1 = func.create_block();
        func.append_block(block1);
        let inst1 = func.create_inst(crate::dfg::InstData::constant(
            v1,
            crate::dfg::Immediate::I64(42),
        ));
        func.append_inst(inst1, block1);
        let inst2 = func.create_inst(crate::dfg::InstData::return_(vec![v1]));
        func.append_inst(inst2, block1);

        let result = validate_value_scoping(&func);
        assert!(
            result.is_err(),
            "Using value from unreachable block should fail"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("unreachable block"),
            "Error should mention unreachable block: {}",
            err
        );
    }

    #[test]
    fn test_validate_duplicate_value_definition() {
        // Test that same value defined twice fails
        let input = r#"function %test() -> i32 {
block0:
    v0 = iconst 42
    v0 = iconst 100
    return v0
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Duplicate value definition should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("Value 0 defined multiple times")
                && err.message.contains("SSA violation"),
            "Error should mention duplicate definition: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_missing_terminator() {
        // Test that block without terminator fails
        let input = r#"function %test() -> i32 {
block0:
    v0 = iconst 42
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Block without terminator should fail");
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("does not end with a terminating instruction"),
            "Error should mention missing terminator: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_entry_block_params() {
        // Test that entry block param mismatch fails
        let input = r#"function %test(i32, i32) -> i32 {
block0(v0: i32):
    return v0
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Entry block param mismatch should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("Entry block has 1 parameters")
                && err.message.contains("expects 2"),
            "Error should mention entry block param mismatch: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_branch_param_count_mismatch() {
        // Test that branch with wrong arg count fails
        let input = r#"function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 42
    brif v0, block1(v1), block2

block1(v2: i32, v3: i32):
    return v2

block2:
    v4 = iconst 0
    return v4
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Branch with wrong arg count should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("expects 2 parameters")
                && err.message.contains("1 arguments provided"),
            "Error should mention branch parameter count mismatch: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_empty_block() {
        // Test that empty block fails
        let input = r#"function %test() -> i32 {
block0:
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Empty block should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("is empty")
                || err
                    .message
                    .contains("does not end with a terminating instruction"),
            "Error should mention empty block: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_branch_missing_args() {
        // Test that branch to block with parameters but no args provided fails
        let input = r#"function %test() -> i32 {
block0:
    v0 = iconst 1
    brif v0, block1, block2

block1:
    v1 = iconst 42
    return v1

block2(v2: i32):
    return v2
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Branch with missing args should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("expects 1 parameters")
                && err.message.contains("0 arguments provided"),
            "Error should mention missing arguments: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_all_validations_pass() {
        // Test that valid IR passes all validations
        let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    brif v0, block1(v1), block2(v1)

block1(v2: i32):
    return v2

block2(v3: i32):
    return v3
}"#;
        let result = parse_function(input.trim());
        assert!(
            result.is_ok(),
            "Valid IR should pass all validations: {:?}",
            result
        );
    }
}
