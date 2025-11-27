//! Validation for parsed IR functions.

use alloc::{collections::BTreeSet, string::String, vec::Vec};

use crate::{function::Function, inst::Inst, value::Value};

/// Validate that block indices in jumps and branches are valid.
pub fn validate_block_indices(func: &Function) -> Result<(), String> {
    let num_blocks = func.blocks.len();

    for (_block_idx, block) in func.blocks.iter().enumerate() {
        for inst in &block.insts {
            match inst {
                Inst::Jump { target, .. } => {
                    if *target as usize >= num_blocks {
                        return Err(alloc::format!(
                            "Jump to block{} but function only has {} blocks",
                            target,
                            num_blocks
                        ));
                    }
                }
                Inst::Br {
                    target_true,
                    target_false,
                    ..
                } => {
                    if *target_true as usize >= num_blocks {
                        return Err(alloc::format!(
                            "Branch to block{} but function only has {} blocks",
                            target_true,
                            num_blocks
                        ));
                    }
                    if *target_false as usize >= num_blocks {
                        return Err(alloc::format!(
                            "Branch to block{} but function only has {} blocks",
                            target_false,
                            num_blocks
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Validate that jump/branch arguments match target block parameter counts.
pub fn validate_block_parameters(func: &Function) -> Result<(), String> {
    for (_block_idx, block) in func.blocks.iter().enumerate() {
        for inst in &block.insts {
            match inst {
                Inst::Jump { target, args } => {
                    let target_block = &func.blocks[*target as usize];
                    if args.len() != target_block.params.len() {
                        return Err(alloc::format!(
                            "Jump to block{} expects {} parameters, but {} arguments provided",
                            target,
                            target_block.params.len(),
                            args.len()
                        ));
                    }
                }
                Inst::Br {
                    target_true,
                    args_true,
                    target_false,
                    args_false,
                    ..
                } => {
                    let target_true_block = &func.blocks[*target_true as usize];
                    if args_true.len() != target_true_block.params.len() {
                        return Err(alloc::format!(
                            "Branch to block{} expects {} parameters, but {} arguments provided",
                            target_true,
                            target_true_block.params.len(),
                            args_true.len()
                        ));
                    }

                    let target_false_block = &func.blocks[*target_false as usize];
                    if args_false.len() != target_false_block.params.len() {
                        return Err(alloc::format!(
                            "Branch to block{} expects {} parameters, but {} arguments provided",
                            target_false,
                            target_false_block.params.len(),
                            args_false.len()
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Validate that return instructions match function signature return count.
pub fn validate_return_values(func: &Function) -> Result<(), String> {
    let expected_return_count = func.signature.returns.len();

    for (block_idx, block) in func.blocks.iter().enumerate() {
        for inst in &block.insts {
            if let Inst::Return { values } = inst {
                if values.len() != expected_return_count {
                    return Err(alloc::format!(
                        "Return instruction in block{} returns {} values, but function signature \
                         expects {}",
                        block_idx,
                        values.len(),
                        expected_return_count
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Validate that blocks end with terminating instructions.
pub fn validate_terminating_instructions(func: &Function) -> Result<(), String> {
    for (block_idx, block) in func.blocks.iter().enumerate() {
        if block.insts.is_empty() {
            return Err(alloc::format!(
                "Block{} is empty (must have at least one terminating instruction)",
                block_idx
            ));
        }

        let last_inst = &block.insts[block.insts.len() - 1];
        match last_inst {
            Inst::Return { .. } | Inst::Jump { .. } | Inst::Br { .. } | Inst::Halt => {
                // Valid terminator
            }
            _ => {
                return Err(alloc::format!(
                    "Block{} does not end with a terminating instruction (return/jump/branch/halt)",
                    block_idx
                ));
            }
        }
    }

    Ok(())
}

/// Validate that entry block parameters match function signature.
pub fn validate_entry_block(func: &Function) -> Result<(), String> {
    if let Some(entry_block) = func.blocks.first() {
        let expected_param_count = func.signature.params.len();
        let actual_param_count = entry_block.params.len();

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

/// Validate that values are only used within their defining block or passed as parameters.
pub fn validate_value_scoping(func: &Function) -> Result<(), String> {
    // Track which values are defined in which blocks
    // Map: value -> block index where it's defined
    let mut value_definitions: BTreeSet<(usize, Value)> = BTreeSet::new();

    // Track which values are available in each block
    // Map: block_idx -> set of available values
    let mut available_values: Vec<BTreeSet<Value>> = Vec::new();

    // First pass: collect definitions and initialize available values
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Initialize available values for this block
        let mut block_available = BTreeSet::new();
        let mut block_definitions = BTreeSet::new();

        // Block parameters are available in this block
        for param in &block.params {
            if !block_definitions.insert(*param) {
                return Err(alloc::format!(
                    "Value {} defined multiple times in block{} (SSA violation)",
                    param.index(),
                    block_idx
                ));
            }
            block_available.insert(*param);
            value_definitions.insert((block_idx, *param));
        }

        // Track values defined in this block
        // Note: Return instructions don't produce results - they consume values
        for inst in &block.insts {
            match inst {
                crate::inst::Inst::Return { .. } => {
                    // Return instructions don't produce results, skip them
                }
                _ => {
                    for result in inst.results() {
                        if !block_definitions.insert(result) {
                            return Err(alloc::format!(
                                "Value {} defined multiple times in block{} (SSA violation)",
                                result.index(),
                                block_idx
                            ));
                        }
                        block_available.insert(result);
                        value_definitions.insert((block_idx, result));
                    }
                }
            }
        }

        available_values.push(block_available);
    }

    // Second pass: validate that all used values are available
    for (block_idx, block) in func.blocks.iter().enumerate() {
        let available = &available_values[block_idx];

        for inst in &block.insts {
            // Check all argument values used by this instruction
            for arg_value in inst.args() {
                if !available.contains(&arg_value) {
                    // Check if this value is defined in another block
                    // Note: We need to find the FIRST definition (in case of duplicates, though SSA shouldn't have them)
                    let defined_in = value_definitions
                        .iter()
                        .find(|(_, v)| *v == arg_value)
                        .map(|(bid, _)| *bid);

                    if let Some(def_block) = defined_in {
                        // Value is defined in a different block - this is an error
                        // (If it were defined in the same block, it would be in available)
                        return Err(alloc::format!(
                            "Value {} used in block{} but defined in block{}. Values must be \
                             passed as block parameters.",
                            arg_value.index(),
                            block_idx,
                            def_block
                        ));
                    } else {
                        // Value not defined anywhere - this is also an error
                        return Err(alloc::format!(
                            "Value {} used in block{} but not defined anywhere",
                            arg_value.index(),
                            block_idx
                        ));
                    }
                }
            }

            // For jump/branch instructions, validate that args are available
            match inst {
                Inst::Jump { args, .. } => {
                    for arg_value in args {
                        if !available.contains(arg_value) {
                            let defined_in = value_definitions
                                .iter()
                                .find(|(_, v)| *v == *arg_value)
                                .map(|(bid, _)| *bid);

                            if let Some(def_block) = defined_in {
                                return Err(alloc::format!(
                                    "Value {} passed to jump in block{} but defined in block{}. \
                                     Values must be available in the current block.",
                                    arg_value.index(),
                                    block_idx,
                                    def_block
                                ));
                            } else {
                                return Err(alloc::format!(
                                    "Value {} passed to jump in block{} but not defined anywhere",
                                    arg_value.index(),
                                    block_idx
                                ));
                            }
                        }
                    }
                }
                Inst::Br {
                    args_true,
                    args_false,
                    ..
                } => {
                    for arg_value in args_true.iter().chain(args_false.iter()) {
                        if !available.contains(arg_value) {
                            let defined_in = value_definitions
                                .iter()
                                .find(|(_, v)| *v == *arg_value)
                                .map(|(bid, _)| *bid);

                            if let Some(def_block) = defined_in {
                                return Err(alloc::format!(
                                    "Value {} passed to branch in block{} but defined in block{}. \
                                     Values must be available in the current block.",
                                    arg_value.index(),
                                    block_idx,
                                    def_block
                                ));
                            } else {
                                return Err(alloc::format!(
                                    "Value {} passed to branch in block{} but not defined anywhere",
                                    arg_value.index(),
                                    block_idx
                                ));
                            }
                        }
                    }
                }
                _ => {}
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
        use alloc::vec;

        use crate::{
            block::Block, function::Function, inst::Inst, signature::Signature, types::Type,
            value::Value,
        };

        let mut func = Function::new(Signature {
            params: vec![Type::I32],
            returns: vec![Type::I32],
        });

        // block0: defines v1
        let mut block0 = Block::new();
        block0.params.push(Value::new(0)); // v0 parameter
        block0.push_inst(Inst::Iconst {
            result: Value::new(1),
            value: 42,
        });
        block0.push_inst(Inst::Br {
            condition: Value::new(0),
            target_true: 1,
            args_true: Vec::new(),
            target_false: 2,
            args_false: Vec::new(),
        });
        func.add_block(block0);

        // block1: uses v1 (defined in block0) - should fail
        let mut block1 = Block::new();
        block1.push_inst(Inst::Return {
            values: vec![Value::new(1)],
        });
        func.add_block(block1);

        // block2: uses v1 (defined in block0) - should fail
        let mut block2 = Block::new();
        block2.push_inst(Inst::Return {
            values: vec![Value::new(1)],
        });
        func.add_block(block2);

        let result = validate_value_scoping(&func);
        assert!(result.is_err(), "Should fail validation, got: {:?}", result);
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("Value 1 used in block") && err_msg.contains("but defined in block0"),
            "Error should mention cross-block usage: {}",
            err_msg
        );
    }

    #[test]
    fn test_validate_cross_block_usage() {
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
        // If validation fails, parse_function will return an error
        let result = parse_function(input.trim());
        assert!(
            result.is_err(),
            "parse_function should fail due to validation error"
        );
        let err = result.unwrap_err();
        assert!(
            err.message.contains("Value 1 used in block")
                && err.message.contains("but defined in block0"),
            "Error should mention cross-block usage: {}",
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
