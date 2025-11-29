//! Code generation for GLSL statements.

use alloc::{collections::BTreeSet, vec::Vec};

use glsl::syntax::{CompoundStatement, SimpleStatement, Statement};

use crate::{
    control::codegen::{
        generate_iteration_statement, generate_jump_statement, generate_selection_statement,
    },
    decl::codegen::generate_declaration,
    error::{GlslError, GlslResult},
    expr::codegen::generate_expr,
    function::codegen::CodeGenContext,
};

/// Generate LPIR for a statement.
pub fn generate_statement(ctx: &mut dyn CodeGenContext, stmt: &Statement) -> GlslResult<()> {
    match stmt {
        Statement::Simple(simple) => generate_simple_statement(ctx, simple),
        Statement::Compound(compound) => generate_compound_statement(ctx, compound),
    }
}

/// Generate LPIR for a compound statement (block).
pub fn generate_compound_statement(
    ctx: &mut dyn CodeGenContext,
    compound: &CompoundStatement,
) -> GlslResult<()> {
    // Push new scope (both old and new for migration compatibility)
    ctx.scope_stack_mut().push(BTreeSet::new());
    ctx.scope_stack_new_mut().push();

    // Generate each statement
    // Skip unreachable code after a terminator (return, etc.)
    for stmt in &compound.statement_list {
        // Check if current block has a terminator before generating next statement
        let current_block = ctx.current_block()?;
        let has_terminator = {
            let func = ctx.builder().function();
            let insts: Vec<_> = func.block_insts(current_block).collect();
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
        
        if has_terminator {
            // Current block ends with a terminator - subsequent statements are unreachable
            // Skip them (GLSL allows unreachable code, but we don't generate it)
            break;
        }
        
        generate_statement(ctx, stmt)?;
    }

    // Pop legacy scope: remove only variables declared in this scope
    if let Some(scope_vars) = ctx.scope_stack_mut().pop() {
        for var_name in scope_vars {
            ctx.variables_mut().remove(&var_name);
        }
    }

    // Pop new scope stack
    ctx.scope_stack_new_mut().pop();

    Ok(())
}

/// Generate LPIR for a simple statement.
pub fn generate_simple_statement(
    ctx: &mut dyn CodeGenContext,
    simple: &SimpleStatement,
) -> GlslResult<()> {
    match simple {
        SimpleStatement::Declaration(decl) => {
            generate_declaration(ctx, decl)?;
            Ok(())
        }
        SimpleStatement::Expression(expr_stmt) => {
            if let Some(expr) = expr_stmt {
                // Generate expression - void function calls are allowed here
                // They don't return a value, but that's OK for expression statements
                match generate_expr(ctx, expr) {
                    Ok(_) => {
                        // Expression has a value - that's fine
                    }
                    Err(e) => {
                        // Check if it's a void function call error - if so, allow it
                        if matches!(&e, GlslError::VoidFunctionCall(_)) {
                            // Void function call - this is allowed in expression statements
                            // The call was already generated, we just don't use the return value
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
            Ok(())
        }
        SimpleStatement::Selection(sel) => generate_selection_statement(ctx, sel),
        SimpleStatement::Iteration(iter) => generate_iteration_statement(ctx, iter),
        SimpleStatement::Jump(jump) => generate_jump_statement(ctx, jump),
        SimpleStatement::Switch(_) => Err(GlslError::codegen("Switch not supported")),
        SimpleStatement::CaseLabel(_) => Err(GlslError::codegen("Case labels not supported")),
    }
}
