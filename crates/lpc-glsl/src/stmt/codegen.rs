//! Code generation for GLSL statements.

use alloc::{collections::BTreeSet, vec::Vec};

use glsl::syntax::{CompoundStatement, SimpleStatement, Statement};

use crate::{
    control::codegen::{generate_iteration_statement, generate_jump_statement, generate_selection_statement},
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
    // Push new scope - need access to scope_stack
    // This requires extending CodeGenContext or accessing CodeGen directly
    // For now, we'll need to add scope_stack access to the trait
    // Actually, let's make this work by having CodeGen pass itself
    // But that won't work with the trait...
    
    // Let me check how scope_stack is used - it's only in CodeGen
    // So we need to either:
    // 1. Add scope_stack methods to CodeGenContext
    // 2. Make generate_compound_statement take &mut CodeGen directly
    
    // For now, let's add scope_stack to the trait
    // Actually, let me check the actual usage pattern first
    
    // Looking at the code, scope_stack is only used in compound statements
    // So I can add it to CodeGenContext trait
    
    // Push new scope
    ctx.scope_stack_mut().push(BTreeSet::new());

    // Generate each statement
    for stmt in &compound.statement_list {
        generate_statement(ctx, stmt)?;
    }

    // Pop scope: remove only variables declared in this scope
    if let Some(scope_vars) = ctx.scope_stack_mut().pop() {
        for var_name in scope_vars {
            ctx.variables_mut().remove(&var_name);
        }
    }

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

