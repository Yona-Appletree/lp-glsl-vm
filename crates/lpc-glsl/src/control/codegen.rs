//! Code generation for GLSL control flow constructs.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    vec,
    vec::Vec,
};

use glsl::syntax::{IterationStatement, JumpStatement, SelectionRestStatement, SelectionStatement};
use lpc_lpir::{Type, Value};

use crate::{
    decl::codegen::generate_declaration,
    error::{GlslError, GlslResult},
    expr::codegen::generate_expr,
    function::codegen::CodeGenContext,
    stmt::codegen::generate_statement,
    util::{find_variable_references, find_variable_references_in_statement},
};

/// Generate LPIR for a selection statement (if/else).
pub fn generate_selection_statement(
    ctx: &mut dyn CodeGenContext,
    sel: &SelectionStatement,
) -> GlslResult<()> {
    // Save variable state before the if statement
    let pre_if_vars = ctx.clone_variables();

    // Generate condition first (before getting block builder)
    let cond_value = generate_expr(ctx, &sel.cond)?;

    // Create blocks for true and false branches
    let block = ctx.current_block()?;
    let true_block = ctx.builder_mut().create_block();
    let false_block = ctx.builder_mut().create_block();

    // Now get block builder for branch instruction
    let mut block_builder = ctx.builder_mut().block_builder(block);
    block_builder.br(
        cond_value,
        true_block,
        &Vec::new(),
        false_block,
        &Vec::new(),
    );
    drop(block_builder);

    // Generate true branch
    ctx.set_current_block(true_block);
    match &sel.rest {
        SelectionRestStatement::Statement(true_stmt) => {
            generate_statement(ctx, true_stmt)?;
            // Check what block we ended up in after generating the statement
            let true_end_block = ctx.current_block()?;
            let true_ends_with_return_or_halt = ctx.block_ends_with_return_or_halt(true_end_block);

            // Save variable state after true branch
            let true_end_vars = ctx.clone_variables();

            // Restore pre-if state for false branch
            ctx.restore_variables(pre_if_vars.clone());

            // False branch is empty - no changes to variables
            let false_end_vars = pre_if_vars.clone();

            // Find variables that were modified in at least one branch
            let mut modified_vars = BTreeSet::new();
            for var_name in true_end_vars.keys() {
                if let (Some(true_val), Some(false_val)) =
                    (true_end_vars.get(var_name), false_end_vars.get(var_name))
                {
                    // Variable exists in both - check if values differ
                    if true_val != false_val {
                        modified_vars.insert(var_name.clone());
                    }
                } else if true_end_vars.contains_key(var_name) {
                    // Variable exists only in true branch (was modified)
                    modified_vars.insert(var_name.clone());
                }
            }
            for var_name in false_end_vars.keys() {
                if !true_end_vars.contains_key(var_name) {
                    // Variable exists only in false branch (was modified)
                    modified_vars.insert(var_name.clone());
                }
            }

            // Create phi nodes for modified variables
            let mut phi_params = Vec::new();
            let mut phi_param_types = Vec::new();
            let mut var_to_phi_idx: BTreeMap<String, usize> = BTreeMap::new();
            let mut phi_var_names = Vec::new();

            for var_name in &modified_vars {
                // Get the type from the pre-if value or true branch value
                let var_type = if let Some(val) = pre_if_vars.get(var_name) {
                    ctx.builder_mut()
                        .function_mut()
                        .dfg
                        .value_type(*val)
                        .unwrap_or(Type::I32)
                } else if let Some(val) = true_end_vars.get(var_name) {
                    ctx.builder_mut()
                        .function_mut()
                        .dfg
                        .value_type(*val)
                        .unwrap_or(Type::I32)
                } else {
                    Type::I32
                };

                let phi_param = ctx.builder_mut().new_value();
                phi_params.push(phi_param);
                phi_param_types.push(var_type);
                let idx = phi_var_names.len();
                phi_var_names.push(var_name.clone());
                var_to_phi_idx.insert(var_name.clone(), idx);

                // Set phi parameter type
                ctx.builder_mut()
                    .function_mut()
                    .dfg
                    .set_value_type(phi_param, var_type);
            }

            // Create merge block with phi parameters if needed
            let merge_block = if phi_params.is_empty() {
                ctx.builder_mut().create_block()
            } else {
                ctx.builder_mut().block_with_params(phi_params.clone())
            };

            // Set phi parameter types in block data
            if !phi_params.is_empty() {
                if let Some(block_data) =
                    ctx.builder_mut().function_mut().block_data_mut(merge_block)
                {
                    block_data.param_types = phi_param_types.clone();
                }
            }

            if !true_ends_with_return_or_halt {
                // Collect values from true branch for phi nodes
                // Use lazy SSA construction to get the correct value
                let mut true_values = Vec::new();
                let jump_source_block = if true_end_block != true_block {
                    true_end_block
                } else {
                    true_block
                };
                for var_name in &phi_var_names {
                    let val = ctx
                        .get_ssa_value(var_name, jump_source_block)?
                        .ok_or_else(|| {
                            GlslError::codegen(format!(
                                "Variable '{}' not found for phi node (lazy SSA)",
                                var_name
                            ))
                        })?;
                    true_values.push(val);
                }

                // Need to jump to merge block from wherever we ended up
                if true_end_block != true_block {
                    // Statement ended in a different block - jump from there
                    ctx.restore_variables(true_end_vars.clone());
                    let mut end_block_builder = ctx.builder_mut().block_builder(true_end_block);
                    if phi_params.is_empty() {
                        end_block_builder.jump(merge_block, &Vec::new());
                    } else {
                        end_block_builder.jump(merge_block, &true_values);
                    }
                } else {
                    // Statement ended in true_block - jump from there
                    ctx.restore_variables(true_end_vars.clone());
                    let mut true_block_builder = ctx.builder_mut().block_builder(true_block);
                    if phi_params.is_empty() {
                        true_block_builder.jump(merge_block, &Vec::new());
                    } else {
                        true_block_builder.jump(merge_block, &true_values);
                    }
                }
            }

            // Collect values from false branch for phi nodes
            // Use lazy SSA construction to get the correct value
            let mut false_values = Vec::new();
            for var_name in &phi_var_names {
                let val = ctx.get_ssa_value(var_name, false_block)?.ok_or_else(|| {
                    GlslError::codegen(format!(
                        "Variable '{}' not found for phi node (lazy SSA)",
                        var_name
                    ))
                })?;
                false_values.push(val);
            }

            // False branch is empty - jump directly to merge block
            ctx.restore_variables(pre_if_vars.clone());
            let mut false_block_builder = ctx.builder_mut().block_builder(false_block);
            if phi_params.is_empty() {
                false_block_builder.jump(merge_block, &Vec::new());
            } else {
                false_block_builder.jump(merge_block, &false_values);
            }
            drop(false_block_builder);

            // Update variables map to use phi parameters
            // Also record phi parameters in SSABuilder as definitions in the merge block
            for (var_name, phi_idx) in &var_to_phi_idx {
                let phi_value = phi_params[*phi_idx];
                ctx.variables_mut().insert(var_name.clone(), phi_value);
                // Record phi parameter as a definition in the merge block
                ctx.ssa_builder_mut()
                    .record_def(var_name, merge_block, phi_value);
            }

            // Continue in merge block
            ctx.set_current_block(merge_block);
        }
        SelectionRestStatement::Else(true_stmt, false_stmt) => {
            generate_statement(ctx, true_stmt)?;
            // Check what block we ended up in after generating the statement
            let true_end_block = ctx.current_block()?;
            let true_ends_with_return_or_halt = ctx.block_ends_with_return_or_halt(true_end_block);

            // Save variable state after true branch
            let true_end_vars = ctx.clone_variables();

            // Restore pre-if state for false branch
            ctx.restore_variables(pre_if_vars.clone());

            // Generate false branch
            ctx.set_current_block(false_block);
            generate_statement(ctx, false_stmt)?;
            // Check what block we ended up in after generating the statement
            let false_end_block = ctx.current_block()?;
            let false_ends_with_return_or_halt =
                ctx.block_ends_with_return_or_halt(false_end_block);

            // Save variable state after false branch
            let false_end_vars = ctx.clone_variables();

            // Find variables that were modified in at least one branch
            let mut modified_vars = BTreeSet::new();
            for var_name in true_end_vars.keys() {
                if let (Some(true_val), Some(false_val)) =
                    (true_end_vars.get(var_name), false_end_vars.get(var_name))
                {
                    // Variable exists in both - check if values differ
                    if true_val != false_val {
                        modified_vars.insert(var_name.clone());
                    }
                } else if true_end_vars.contains_key(var_name) {
                    // Variable exists only in true branch (was modified)
                    modified_vars.insert(var_name.clone());
                }
            }
            for var_name in false_end_vars.keys() {
                if !true_end_vars.contains_key(var_name) {
                    // Variable exists only in false branch (was modified)
                    modified_vars.insert(var_name.clone());
                }
            }

            // Only create merge block if at least one branch doesn't return/halt
            if !true_ends_with_return_or_halt || !false_ends_with_return_or_halt {
                // Create phi nodes for modified variables
                let mut phi_params = Vec::new();
                let mut phi_param_types = Vec::new();
                let mut var_to_phi_idx: BTreeMap<String, usize> = BTreeMap::new();
                let mut phi_var_names = Vec::new();

                for var_name in &modified_vars {
                    // Get the type from the pre-if value or true branch value
                    let var_type = if let Some(val) = pre_if_vars.get(var_name) {
                        ctx.builder_mut()
                            .function_mut()
                            .dfg
                            .value_type(*val)
                            .unwrap_or(Type::I32)
                    } else if let Some(val) = true_end_vars.get(var_name) {
                        ctx.builder_mut()
                            .function_mut()
                            .dfg
                            .value_type(*val)
                            .unwrap_or(Type::I32)
                    } else {
                        Type::I32
                    };

                    let phi_param = ctx.builder_mut().new_value();
                    phi_params.push(phi_param);
                    phi_param_types.push(var_type);
                    let idx = phi_var_names.len();
                    phi_var_names.push(var_name.clone());
                    var_to_phi_idx.insert(var_name.clone(), idx);

                    // Set phi parameter type
                    ctx.builder_mut()
                        .function_mut()
                        .dfg
                        .set_value_type(phi_param, var_type);
                }

                // Create merge block with phi parameters if needed
                let merge_block = if phi_params.is_empty() {
                    ctx.builder_mut().create_block()
                } else {
                    ctx.builder_mut().block_with_params(phi_params.clone())
                };

                // Set phi parameter types in block data
                if !phi_params.is_empty() {
                    if let Some(block_data) =
                        ctx.builder_mut().function_mut().block_data_mut(merge_block)
                    {
                        block_data.param_types = phi_param_types.clone();
                    }
                }

                if !true_ends_with_return_or_halt {
                    // Collect values from true branch for phi nodes
                    // Use SSABuilder to get values at the jump source block to ensure dominance
                    let mut true_values = Vec::new();
                    let jump_source_block = if true_end_block != true_block {
                        true_end_block
                    } else {
                        true_block
                    };
                    // Collect values using lazy SSA construction
                    for var_name in &phi_var_names {
                        let val =
                            ctx.get_ssa_value(var_name, jump_source_block)?
                                .ok_or_else(|| {
                                    GlslError::codegen(format!(
                                        "Variable '{}' not found for phi node (lazy SSA)",
                                        var_name
                                    ))
                                })?;
                        true_values.push(val);
                    }

                    // Need to jump to merge block from wherever we ended up
                    if true_end_block != true_block {
                        ctx.restore_variables(true_end_vars.clone());
                        let mut end_block_builder = ctx.builder_mut().block_builder(true_end_block);
                        if phi_params.is_empty() {
                            end_block_builder.jump(merge_block, &Vec::new());
                        } else {
                            end_block_builder.jump(merge_block, &true_values);
                        }
                    } else {
                        ctx.restore_variables(true_end_vars.clone());
                        let mut true_block_builder = ctx.builder_mut().block_builder(true_block);
                        if phi_params.is_empty() {
                            true_block_builder.jump(merge_block, &Vec::new());
                        } else {
                            true_block_builder.jump(merge_block, &true_values);
                        }
                    }
                }

                if !false_ends_with_return_or_halt {
                    // Collect values from false branch for phi nodes
                    // Use SSABuilder to get values at the jump source block to ensure dominance
                    let mut false_values = Vec::new();
                    let jump_source_block = if false_end_block != false_block {
                        false_end_block
                    } else {
                        false_block
                    };
                    // Collect values using lazy SSA construction
                    for var_name in &phi_var_names {
                        let val =
                            ctx.get_ssa_value(var_name, jump_source_block)?
                                .ok_or_else(|| {
                                    GlslError::codegen(format!(
                                        "Variable '{}' not found for phi node (lazy SSA)",
                                        var_name
                                    ))
                                })?;
                        false_values.push(val);
                    }

                    // Need to jump to merge block from wherever we ended up
                    if false_end_block != false_block {
                        ctx.restore_variables(false_end_vars.clone());
                        let mut end_block_builder =
                            ctx.builder_mut().block_builder(false_end_block);
                        if phi_params.is_empty() {
                            end_block_builder.jump(merge_block, &Vec::new());
                        } else {
                            end_block_builder.jump(merge_block, &false_values);
                        }
                    } else {
                        ctx.restore_variables(false_end_vars.clone());
                        let mut false_block_builder = ctx.builder_mut().block_builder(false_block);
                        if phi_params.is_empty() {
                            false_block_builder.jump(merge_block, &Vec::new());
                        } else {
                            false_block_builder.jump(merge_block, &false_values);
                        }
                    }
                }

                // Update variables map to use phi parameters
                // Also record phi parameters in SSABuilder as definitions in the merge block
                for (var_name, phi_idx) in &var_to_phi_idx {
                    let phi_value = phi_params[*phi_idx];
                    ctx.variables_mut().insert(var_name.clone(), phi_value);
                    // Record phi parameter as a definition in the merge block
                    ctx.ssa_builder_mut()
                        .record_def(var_name, merge_block, phi_value);
                }

                // Continue in merge block
                ctx.set_current_block(merge_block);
            }
            // If both branches return/halt, we don't create a merge block and current_block
            // is left pointing to the false_end_block (which has return/halt)
        }
    }

    Ok(())
}

/// Generate LPIR for an iteration statement (for/while).
pub fn generate_iteration_statement(
    ctx: &mut dyn CodeGenContext,
    iter: &IterationStatement,
) -> GlslResult<()> {
    match iter {
        IterationStatement::While(cond, body) => {
            // Save variable state before loop
            let pre_loop_vars = ctx.clone_variables();

            // Find variables referenced in the condition
            let cond_vars = match cond {
                glsl::syntax::Condition::Expr(expr) => find_variable_references(expr),
                glsl::syntax::Condition::Assignment(_, _, _) => {
                    return Err(GlslError::codegen(
                        "Assignment in while condition not supported",
                    ))
                }
            };

            // Create phi nodes for variables used in condition
            let mut phi_params = Vec::new();
            let mut phi_param_types = Vec::new();
            let mut var_to_phi_idx: BTreeMap<String, usize> = BTreeMap::new();
            let mut phi_var_names = Vec::new();

            for var_name in &cond_vars {
                if let Some(initial_val) = pre_loop_vars.get(var_name) {
                    // Get the type of the initial value
                    let var_type = ctx
                        .builder_mut()
                        .function_mut()
                        .dfg
                        .value_type(*initial_val)
                        .unwrap_or(Type::I32);

                    let phi_param = ctx.builder_mut().new_value();
                    phi_params.push(phi_param);
                    phi_param_types.push(var_type);
                    let idx = phi_var_names.len();
                    phi_var_names.push(var_name.clone());
                    var_to_phi_idx.insert(var_name.clone(), idx);

                    // Set phi parameter type
                    ctx.builder_mut()
                        .function_mut()
                        .dfg
                        .set_value_type(phi_param, var_type);
                }
            }

            // Create loop header block with phi parameters
            let entry_block = ctx.current_block()?;
            let loop_header = if phi_params.is_empty() {
                ctx.builder_mut().create_block()
            } else {
                ctx.builder_mut().block_with_params(phi_params.clone())
            };

            // Set phi parameter types in block data
            if !phi_params.is_empty() {
                if let Some(block_data) =
                    ctx.builder_mut().function_mut().block_data_mut(loop_header)
                {
                    block_data.param_types = phi_param_types.clone();
                }
            }

            // Update variables map to use phi parameters
            // Also record phi parameters in SSABuilder as definitions in the loop header
            for (var_name, phi_idx) in &var_to_phi_idx {
                let phi_value = phi_params[*phi_idx];
                ctx.variables_mut().insert(var_name.clone(), phi_value);
                // Record phi parameter as a definition in the loop header
                ctx.ssa_builder_mut()
                    .record_def(var_name, loop_header, phi_value);
            }

            // Jump from entry to loop header with initial values
            let mut entry_builder = ctx.builder_mut().block_builder(entry_block);
            let mut initial_values = Vec::new();
            for var_name in &phi_var_names {
                if let Some(initial_val) = pre_loop_vars.get(var_name) {
                    initial_values.push(*initial_val);
                }
            }
            if phi_params.is_empty() {
                entry_builder.jump(loop_header, &Vec::new());
            } else {
                entry_builder.jump(loop_header, &initial_values);
            }
            drop(entry_builder);

            // Generate condition in loop header
            ctx.set_current_block(loop_header);
            let body_block = ctx.builder_mut().create_block();
            let exit_block = ctx.builder_mut().create_block();

            let cond_value = match cond {
                glsl::syntax::Condition::Expr(expr) => generate_expr(ctx, expr)?,
                _ => unreachable!(),
            };

            // Branch: if condition, go to body, else exit
            let mut loop_header_builder = ctx.builder_mut().block_builder(loop_header);
            loop_header_builder.br(cond_value, body_block, &Vec::new(), exit_block, &Vec::new());
            drop(loop_header_builder);

            // Generate body
            ctx.set_current_block(body_block);
            generate_statement(ctx, body)?;

            // Check what block we ended up in after generating the body
            let body_end_block = ctx.current_block()?;

            // Check if body_end_block ends with a terminator (return, etc.)
            let has_terminator = {
                let func = ctx.builder().function();
                let insts: Vec<_> = func.block_insts(body_end_block).collect();
                if let Some(last_inst) = insts.last() {
                    if let Some(inst_data) = func.dfg.inst_data(*last_inst) {
                        inst_data.opcode.is_terminator()
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            // Jump back to loop header with updated values (only if body didn't end with terminator)
            if !has_terminator {
                // Collect updated values for phi nodes (in same order as phi_var_names)
                // Use lazy SSA construction to get the correct value at body_end_block
                let mut updated_values = Vec::new();
                for var_name in &phi_var_names {
                    // Use get_ssa_value to ensure dominance correctness
                    let source_block = if body_end_block != body_block {
                        body_end_block
                    } else {
                        body_block
                    };
                    if let Some(updated_val) = ctx.get_ssa_value(var_name, source_block)? {
                        updated_values.push(updated_val);
                    } else {
                        // Variable not found - use initial value
                        if let Some(initial_val) = pre_loop_vars.get(var_name) {
                            updated_values.push(*initial_val);
                        } else {
                            return Err(GlslError::codegen(format!(
                                "Variable '{}' not found for loop phi node",
                                var_name
                            )));
                        }
                    }
                }
                if body_end_block != body_block {
                    let mut end_block_builder = ctx.builder_mut().block_builder(body_end_block);
                    if phi_params.is_empty() {
                        end_block_builder.jump(loop_header, &Vec::new());
                    } else {
                        end_block_builder.jump(loop_header, &updated_values);
                    }
                } else {
                    let mut body_block_builder = ctx.builder_mut().block_builder(body_block);
                    if phi_params.is_empty() {
                        body_block_builder.jump(loop_header, &Vec::new());
                    } else {
                        body_block_builder.jump(loop_header, &updated_values);
                    }
                }
            }
            // If has_terminator, the body ended with a return/break/etc., so don't add jump

            // After loop, variables should use the phi node results (which are already in variables map)
            // Continue in exit block
            ctx.set_current_block(exit_block);

            Ok(())
        }
        IterationStatement::DoWhile(body, cond_expr) => {
            // Save variable state before loop
            let pre_loop_vars = ctx.clone_variables();

            // Find variables referenced in the condition
            let cond_vars = find_variable_references(cond_expr);

            // Create phi nodes for variables used in condition
            let mut phi_params = Vec::new();
            let mut phi_param_types = Vec::new();
            let mut var_to_phi_idx: BTreeMap<String, usize> = BTreeMap::new();
            let mut phi_var_names = Vec::new();

            for var_name in &cond_vars {
                if let Some(initial_val) = pre_loop_vars.get(var_name) {
                    // Get the type of the initial value
                    let var_type = ctx
                        .builder_mut()
                        .function_mut()
                        .dfg
                        .value_type(*initial_val)
                        .unwrap_or(Type::I32);

                    let phi_param = ctx.builder_mut().new_value();
                    phi_params.push(phi_param);
                    phi_param_types.push(var_type);
                    let idx = phi_var_names.len();
                    phi_var_names.push(var_name.clone());
                    var_to_phi_idx.insert(var_name.clone(), idx);

                    // Set phi parameter type
                    ctx.builder_mut()
                        .function_mut()
                        .dfg
                        .set_value_type(phi_param, var_type);
                }
            }

            // Create body block with phi parameters (loop header)
            let entry_block = ctx.current_block()?;
            let body_block = if phi_params.is_empty() {
                ctx.builder_mut().create_block()
            } else {
                ctx.builder_mut().block_with_params(phi_params.clone())
            };

            // Set phi parameter types in block data
            if !phi_params.is_empty() {
                if let Some(block_data) =
                    ctx.builder_mut().function_mut().block_data_mut(body_block)
                {
                    block_data.param_types = phi_param_types.clone();
                }
            }

            // Update variables map to use phi parameters
            // Also record phi parameters in SSABuilder as definitions in the body block
            for (var_name, phi_idx) in &var_to_phi_idx {
                let phi_value = phi_params[*phi_idx];
                ctx.variables_mut().insert(var_name.clone(), phi_value);
                // Record phi parameter as a definition in the body block
                ctx.ssa_builder_mut()
                    .record_def(var_name, body_block, phi_value);
            }

            // Collect initial values for phi nodes
            let mut initial_values = Vec::new();
            for var_name in &phi_var_names {
                if let Some(initial_val) = pre_loop_vars.get(var_name) {
                    initial_values.push(*initial_val);
                }
            }

            // Jump from entry to body with initial values
            let mut entry_builder = ctx.builder_mut().block_builder(entry_block);
            if phi_params.is_empty() {
                entry_builder.jump(body_block, &Vec::new());
            } else {
                entry_builder.jump(body_block, &initial_values);
            }
            drop(entry_builder);

            // Generate body
            ctx.set_current_block(body_block);
            generate_statement(ctx, body)?;

            // Check what block we ended up in after generating the body
            let body_end_block = ctx.current_block()?;

            // Check if body_end_block ends with a terminator (return, etc.)
            let has_terminator = {
                let func = ctx.builder().function();
                let insts: Vec<_> = func.block_insts(body_end_block).collect();
                if let Some(last_inst) = insts.last() {
                    if let Some(inst_data) = func.dfg.inst_data(*last_inst) {
                        inst_data.opcode.is_terminator()
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            // Create condition and exit blocks
            let cond_block = ctx.builder_mut().create_block();
            let exit_block = ctx.builder_mut().create_block();

            // Collect updated values for phi nodes (only if body didn't end with terminator)
            // We need these for the condition branch even if body ended with terminator
            // (but if body ended with terminator, we won't reach the condition block)
            let mut updated_values = Vec::new();
            if !has_terminator {
                // Use lazy SSA construction to get the correct value at body_end_block
                for var_name in &phi_var_names {
                    // Use get_ssa_value to ensure dominance correctness
                    let source_block = if body_end_block != body_block {
                        body_end_block
                    } else {
                        body_block
                    };
                    if let Some(updated_val) = ctx.get_ssa_value(var_name, source_block)? {
                        updated_values.push(updated_val);
                    } else {
                        // Variable not found - use initial value
                        if let Some(initial_val) = pre_loop_vars.get(var_name) {
                            updated_values.push(*initial_val);
                        } else {
                            return Err(GlslError::codegen(format!(
                                "Variable '{}' not found for loop phi node",
                                var_name
                            )));
                        }
                    }
                }
            } else {
                // Body ended with terminator - use initial values (won't be used, but needed for type)
                for var_name in &phi_var_names {
                    if let Some(initial_val) = pre_loop_vars.get(var_name) {
                        updated_values.push(*initial_val);
                    }
                }
            }

            // Jump to condition with updated values (only if body didn't end with terminator)
            if !has_terminator {
                if body_end_block != body_block {
                    let mut end_block_builder = ctx.builder_mut().block_builder(body_end_block);
                    if phi_params.is_empty() {
                        end_block_builder.jump(cond_block, &Vec::new());
                    } else {
                        end_block_builder.jump(cond_block, &updated_values);
                    }
                } else {
                    let mut body_block_builder = ctx.builder_mut().block_builder(body_block);
                    if phi_params.is_empty() {
                        body_block_builder.jump(cond_block, &Vec::new());
                    } else {
                        body_block_builder.jump(cond_block, &updated_values);
                    }
                }
            }
            // If has_terminator, the body ended with a return/break/etc., so don't add jump

            // Generate condition
            ctx.set_current_block(cond_block);
            let cond_value = generate_expr(ctx, cond_expr)?;

            // Branch: if condition, go to body, else exit
            let mut cond_builder = ctx.builder_mut().block_builder(cond_block);
            if phi_params.is_empty() {
                cond_builder.br(cond_value, body_block, &Vec::new(), exit_block, &Vec::new());
            } else {
                // For the true branch, pass updated values
                cond_builder.br(
                    cond_value,
                    body_block,
                    &updated_values,
                    exit_block,
                    &Vec::new(),
                );
            }
            drop(cond_builder);

            // Continue in exit block
            ctx.set_current_block(exit_block);

            Ok(())
        }
        IterationStatement::For(init, rest, body) => {
            // Save variable state before loop
            let pre_loop_vars = ctx.clone_variables();

            // Find variables referenced in the condition
            let cond_vars = if let Some(cond) = &rest.condition {
                match cond {
                    glsl::syntax::Condition::Expr(expr) => find_variable_references(expr),
                    glsl::syntax::Condition::Assignment(_, _, _) => {
                        return Err(GlslError::codegen(
                            "Assignment in for condition not supported",
                        ))
                    }
                }
            } else {
                BTreeSet::new()
            };

            // Generate initialization
            let entry_block = ctx.current_block()?;
        crate::debug!("[FOR] Starting for loop codegen. entry_block={:?}", entry_block);
            match init {
                glsl::syntax::ForInitStatement::Expression(expr_opt) => {
                    if let Some(expr) = expr_opt {
                        generate_expr(ctx, expr)?;
                    }
                }
                glsl::syntax::ForInitStatement::Declaration(decl) => {
        crate::debug!("[FOR] Generating declaration in init");
                    generate_declaration(ctx, decl)?;
                    // After declaration, check if 'i' is recorded
                    let (i_val, i_blocks) = {
                        let i_val_opt = ctx.variables().get("i").copied();
                        let i_blocks: Vec<_> = {
                            use crate::codegen::SSABuilder;
                            let ssa_ptr: *const SSABuilder = ctx.ssa_builder_mut() as *const SSABuilder;
                            unsafe { (*ssa_ptr).debug_get_blocks_for_var("i") }
                        };
                        (i_val_opt, i_blocks)
                    };
                    if let Some(i_val) = i_val {
        crate::debug!("[FOR] After declaration: 'i' value={:?}, recorded_in_blocks={:?}", i_val, i_blocks);
                    }
                }
            }

            // Find variables referenced in the body (for phi nodes)
            let body_vars = find_variable_references_in_statement(body);
        crate::debug!("[FOR] cond_vars={:?}, body_vars={:?}", cond_vars, body_vars);
            let mut loop_vars = cond_vars.clone();
            loop_vars.extend(body_vars.clone());

            // Also include variables declared in the initialization that are used in condition or body
            // (variables declared in init are now in ctx.variables() but may not be in cond_vars/body_vars yet)
            // Actually, if a variable is declared in init and used in condition/body, it should already
            // be in cond_vars/body_vars. But we need to make sure variables declared in init are included
            // in loop_vars even if they're not yet referenced (they will be when we generate the code).
            // For now, include all variables that are in ctx.variables() after initialization
            // and are either in cond_vars, body_vars, or were just declared.
            let init_declared_vars: BTreeSet<String> = ctx
                .variables()
                .keys()
                .filter(|name| !pre_loop_vars.contains_key(*name))
                .cloned()
                .collect();
        crate::debug!("[FOR] init_declared_vars={:?}", init_declared_vars);
            for var_name in &init_declared_vars {
                // Include if used in condition or body, or if it's a newly declared variable
                // (newly declared variables in for init are always loop variables)
                loop_vars.insert(var_name.clone());
            }
        crate::debug!("[FOR] loop_vars after init_declared_vars={:?}", loop_vars);

            // Create phi nodes for variables used in condition or body
            let mut phi_params = Vec::new();
            let mut phi_param_types = Vec::new();
            let mut var_to_phi_idx: BTreeMap<String, usize> = BTreeMap::new();
            let mut phi_var_names = Vec::new();

            // Collect initial values first
            // For variables declared in init, they should be in ctx.variables() after generate_declaration
            // For other variables, they should be in pre_loop_vars
            let mut var_values: Vec<(String, Value)> = Vec::new();
            for var_name in &loop_vars {
                // First check if variable was declared in initialization (now in ctx.variables())
                if let Some(initial_val) = ctx.variables().get(var_name) {
        crate::debug!("[FOR] Found '{}' in ctx.variables(), value={:?}", var_name, initial_val);
                    var_values.push((var_name.clone(), *initial_val));
                } else if let Some(initial_val) = pre_loop_vars.get(var_name) {
                    // Variable existed before loop - use pre-loop value
        crate::debug!("[FOR] Found '{}' in pre_loop_vars, value={:?}", var_name, initial_val);
                    var_values.push((var_name.clone(), *initial_val));
                } else {
                    // Variable should have been found - this is an error
        crate::debug!("[FOR] ERROR: '{}' not found in ctx.variables() or pre_loop_vars", var_name);
                    return Err(GlslError::codegen(format!(
                        "Variable '{}' not found for loop phi node (not in ctx.variables() or \
                         pre_loop_vars)",
                        var_name
                    )));
                }
            }
        crate::debug!("[FOR] var_values collected: {:?}", var_values.iter().map(|(n, v)| (n.clone(), format!("{:?}", v))).collect::<Vec<_>>());

            // Now get types (need mutable access to builder)
            let mut var_info: Vec<(String, Value, Type)> = Vec::new();
            for (var_name, initial_val) in &var_values {
                let var_type = ctx
                    .builder_mut()
                    .function_mut()
                    .dfg
                    .value_type(*initial_val)
                    .unwrap_or(Type::I32);
                var_info.push((var_name.clone(), *initial_val, var_type));
            }

            // Now create phi nodes
            for (var_name, _initial_val, var_type) in &var_info {
                let phi_param = ctx.builder_mut().new_value();
                phi_params.push(phi_param);
                phi_param_types.push(*var_type);
                let idx = phi_var_names.len();
                phi_var_names.push(var_name.clone());
                var_to_phi_idx.insert(var_name.clone(), idx);
        crate::debug!("[FOR] Created phi parameter for '{}': phi_param={:?}, idx={}", var_name, phi_param, idx);

                // Set phi parameter type
                ctx.builder_mut()
                    .function_mut()
                    .dfg
                    .set_value_type(phi_param, *var_type);
            }
        crate::debug!("[FOR] Created {} phi parameters. var_to_phi_idx keys: {:?}", phi_params.len(), var_to_phi_idx.keys().collect::<Vec<_>>());

            // Create condition block with phi parameters (loop header)
            let cond_block = if phi_params.is_empty() {
                ctx.builder_mut().create_block()
            } else {
                ctx.builder_mut().block_with_params(phi_params.clone())
            };
        crate::debug!("[FOR] Created cond_block={:?} with {} phi parameters. entry_block={:?}", cond_block, phi_params.len(), entry_block);

            // Set phi parameter types in block data
            if !phi_params.is_empty() {
                if let Some(block_data) =
                    ctx.builder_mut().function_mut().block_data_mut(cond_block)
                {
                    block_data.param_types = phi_param_types.clone();
                }
            }

            // Update variables map to use phi parameters
            // Also record phi parameters in SSABuilder as definitions in the condition block
        crate::debug!("[FOR] About to record phi parameters in SSABuilder. cond_block={:?}, entry_block={:?}", cond_block, entry_block);
            for (var_name, phi_idx) in &var_to_phi_idx {
                let phi_value = phi_params[*phi_idx];
        crate::debug!("[FOR] Recording phi for '{}': phi_value={:?}, cond_block={:?}", var_name, phi_value, cond_block);
                
                // Check what blocks 'i' is recorded in BEFORE recording
                if *var_name == "i" {
                    // Use a scope to get immutable access
                    let blocks_before: Vec<_> = {
                        use crate::codegen::SSABuilder;
                        let ssa_ptr: *const SSABuilder = ctx.ssa_builder_mut() as *const SSABuilder;
                        unsafe { (*ssa_ptr).debug_get_blocks_for_var("i") }
                    };
        crate::debug!("[FOR] Before recording 'i': blocks_before={:?}", blocks_before);
                }
                
                ctx.variables_mut().insert(var_name.clone(), phi_value);
                
                // Record phi parameter as a definition in the condition block
                ctx.ssa_builder_mut().record_def(var_name, cond_block, phi_value);
                
                // Verify it was recorded (especially for 'i')
                if *var_name == "i" {
                    // Use a scope to get immutable access
                    let (blocks_after, recorded) = {
                        use crate::codegen::SSABuilder;
                        use lpc_lpir::BlockEntity;
                        let ssa_ptr: *const SSABuilder = ctx.ssa_builder_mut() as *const SSABuilder;
                        unsafe {
                            let blocks_after = (*ssa_ptr).debug_get_blocks_for_var("i");
                            let recorded = (*ssa_ptr).get_value("i", cond_block);
                            (blocks_after, recorded)
                        }
                    };
        crate::debug!("[FOR] After recording 'i': blocks_after={:?}, recorded_in_cond_block={:?}", blocks_after, recorded);
                    if recorded.is_none() {
                        return Err(GlslError::codegen(format!(
                            "BUG: 'i' phi parameter not found immediately after recording. \
                             cond_block={:?}, blocks_after={:?}",
                            cond_block, blocks_after
                        )));
                    }
                }
            }
        crate::debug!("[FOR] Finished recording phi parameters");
            

            // Collect initial values for phi nodes
            // Use current variables (after initialization) for variables declared in init,
            // otherwise use pre_loop_vars
            let mut initial_values = Vec::new();
            for var_name in &phi_var_names {
                // First check if variable was declared in initialization (now in ctx.variables())
                if let Some(initial_val) = ctx.variables().get(var_name) {
                    initial_values.push(*initial_val);
                } else if let Some(initial_val) = pre_loop_vars.get(var_name) {
                    initial_values.push(*initial_val);
                } else {
                    // Variable should have been found - this is an error
                    return Err(GlslError::codegen(format!(
                        "Variable '{}' not found for loop phi node",
                        var_name
                    )));
                }
            }

            // Jump from entry to condition with initial values
            let mut entry_builder = ctx.builder_mut().block_builder(entry_block);
            if phi_params.is_empty() {
                entry_builder.jump(cond_block, &Vec::new());
            } else {
                entry_builder.jump(cond_block, &initial_values);
            }
            drop(entry_builder);

            // Generate condition
        crate::debug!("[FOR] Setting current block to cond_block={:?}", cond_block);
            ctx.set_current_block(cond_block);
            let body_block = ctx.builder_mut().create_block();
            let inc_block = ctx.builder_mut().create_block();

            // Check if condition exists - if not, exit_block is unreachable
            let has_condition = rest.condition.is_some();
            let exit_block = if has_condition {
                Some(ctx.builder_mut().create_block())
            } else {
                None
            };

        crate::debug!("[FOR] About to generate condition expression");
            let cond_value = if let Some(cond) = &rest.condition {
                match cond {
                    glsl::syntax::Condition::Expr(expr) => {
        crate::debug!("[FOR] Generating condition expr - will read variable 'i'");
                        generate_expr(ctx, expr)?
                    },
                    _ => unreachable!(),
                }
            } else {
                // No condition means always true
                let true_val = ctx.builder_mut().new_value();
                let mut cond_builder = ctx.builder_mut().block_builder(cond_block);
                cond_builder.iconst(true_val, 1);
                drop(cond_builder);
                ctx.builder_mut()
                    .function_mut()
                    .dfg
                    .set_value_type(true_val, Type::U32);
                true_val
            };

            let mut cond_builder = ctx.builder_mut().block_builder(cond_block);
            // Branch: if condition, go to body, else exit (if exit exists)
            if let Some(exit) = exit_block {
                cond_builder.br(cond_value, body_block, &Vec::new(), exit, &Vec::new());
            } else {
                // No exit block - always jump to body (condition is always true)
                cond_builder.jump(body_block, &Vec::new());
            }
            drop(cond_builder);

            // Generate body
            ctx.set_current_block(body_block);
            generate_statement(ctx, body)?;

            // If body ended in a different block (e.g., merge block from nested if),
            // we need to jump from that block to increment
            // BUT: if the body ended with a return/terminator, don't add the jump
            let body_end_block = ctx.current_block()?;
            
            // Check if body_end_block ends with a terminator (return, etc.)
            // Also check if any predecessor has a terminator (e.g., merge block after if with return)
            let has_terminator = {
                // Check if body_end_block itself has a terminator
                let block_has_terminator = ctx.block_ends_with_return_or_halt(body_end_block);
                
                // Also check if any predecessor has a terminator
                let func = ctx.builder().function();
                let cfg = lpc_lpir::ControlFlowGraph::from_function(func);
                // Build block to index map and collect blocks
                let blocks: Vec<_> = func.blocks().collect();
                let block_to_idx: BTreeMap<_, _> = blocks.iter().enumerate().map(|(i, &b)| (b, i)).collect();
                let body_end_idx = *block_to_idx.get(&body_end_block).unwrap_or(&0);
                
                let pred_has_terminator = cfg.predecessors(body_end_idx).iter().any(|&pred_idx| {
                    if let Some(&pred_block) = blocks.get(pred_idx) {
                        ctx.block_ends_with_return_or_halt(pred_block)
                    } else {
                        false
                    }
                });
                
                block_has_terminator || pred_has_terminator
            };

            // CRITICAL: Never jump from a block that has a terminator or whose predecessor has a terminator
            // This prevents dominance violations when return statements are inside control flow
            if !has_terminator {
                if body_end_block != body_block {
                    // Body ended in a merge block - jump to increment
                    // Double-check that merge block doesn't have a terminator
                    if !ctx.block_ends_with_return_or_halt(body_end_block) {
                        let mut merge_builder = ctx.builder_mut().block_builder(body_end_block);
                        merge_builder.jump(inc_block, &Vec::new());
                    }
                } else {
                    // Body ended in body_block - jump to increment
                    // Double-check that body_block doesn't have a terminator
                    if !ctx.block_ends_with_return_or_halt(body_block) {
                        let mut body_block_builder = ctx.builder_mut().block_builder(body_block);
                        body_block_builder.jump(inc_block, &Vec::new());
                    }
                }
            }
            // If has_terminator, the body ended with a return/break/etc., so don't add jump

            // Only generate increment and collect updated values if body didn't end with terminator
            if !has_terminator {
                // Generate increment
                ctx.set_current_block(inc_block);
                if let Some(post_expr) = &rest.post_expr {
                    generate_expr(ctx, post_expr)?;
                }

                // Collect updated values for phi nodes (in same order as phi_var_names)
                // Use lazy SSA construction to get the correct value at the increment block
                let mut updated_values = Vec::new();
                for var_name in &phi_var_names {
                    // Use get_ssa_value to ensure dominance correctness
                    if let Some(updated_val) = ctx.get_ssa_value(var_name, inc_block)? {
                        updated_values.push(updated_val);
                    } else {
                        // Variable not found - use initial value
                        if let Some(initial_val) = pre_loop_vars.get(var_name) {
                            updated_values.push(*initial_val);
                        } else {
                            return Err(GlslError::codegen(format!(
                                "Variable '{}' not found for loop phi node",
                                var_name
                            )));
                        }
                    }
                }

                // Jump back to condition with updated values
                let mut inc_builder = ctx.builder_mut().block_builder(inc_block);
                if phi_params.is_empty() {
                    inc_builder.jump(cond_block, &Vec::new());
                } else {
                    inc_builder.jump(cond_block, &updated_values);
                }
                drop(inc_builder);
            }
            // If has_terminator, skip increment and jump - the return already terminated the block

            // Exit block will be continued by the next statement
            if let Some(exit) = exit_block {
                ctx.set_current_block(exit);
            }

            Ok(())
        }
    }
}

/// Generate LPIR for a jump statement (return/break/continue).
pub fn generate_jump_statement(
    ctx: &mut dyn CodeGenContext,
    jump: &JumpStatement,
) -> GlslResult<()> {
    let block = ctx.current_block()?;

    match jump {
        JumpStatement::Return(expr_opt) => {
            // Before returning, store out/inout parameters back to their addresses
            let out_inout_params: Vec<_> = ctx.out_inout_params().iter().collect();
            if !out_inout_params.is_empty() {
                // Collect variable values first
                let var_values: Vec<_> = out_inout_params
                    .iter()
                    .filter_map(|(var_name, (address_param, glsl_type))| {
                        ctx.variables()
                            .get(var_name.as_str())
                            .map(|val| (*address_param, *val, glsl_type.to_lpir()))
                    })
                    .collect();

                // Then generate store instructions
                let mut block_builder = ctx.builder_mut().block_builder(block);
                for (address_param, current_value, lpir_type) in var_values {
                    block_builder.store(address_param, current_value, lpir_type);
                }
                drop(block_builder);
            }

            if let Some(expr) = expr_opt {
                // Generate expression first (creates and drops its own block builder)
                let return_value = generate_expr(ctx, expr)?;
                // Now get block builder for return instruction
                let mut block_builder = ctx.builder_mut().block_builder(block);
                block_builder.return_(&vec![return_value]);
            } else {
                let mut block_builder = ctx.builder_mut().block_builder(block);
                block_builder.return_(&Vec::new());
            }
            Ok(())
        }
        JumpStatement::Break | JumpStatement::Continue => {
            Err(GlslError::codegen("Break/continue not supported"))
        }
        JumpStatement::Discard => Err(GlslError::codegen("Discard not supported")),
    }
}
