//! Type checking for GLSL statements.

use alloc::{boxed::Box, vec::Vec};

use glsl::syntax::{CompoundStatement, JumpStatement, SimpleStatement, Statement};

use crate::{
    control::typecheck::{
        type_check_iteration_statement, type_check_jump_statement, type_check_selection_statement,
    },
    decl::typecheck::type_check_declaration,
    error::{GlslError, GlslResult},
    expr::typecheck::type_check_expr,
    symbols::SymbolTable,
    types::GlslType,
};

/// Type check a statement.
///
/// `expected_return` is the expected return type for return statements
/// (None for void functions).
pub fn type_check_statement(
    symbols: &mut SymbolTable,
    stmt: &Statement,
    expected_return: Option<GlslType>,
) -> GlslResult<()> {
    match stmt {
        Statement::Simple(simple) => type_check_simple_statement(symbols, simple, expected_return),
        Statement::Compound(compound) => {
            type_check_compound_statement(symbols, compound, expected_return)
        }
    }
}

/// Type check a compound statement (block).
pub fn type_check_compound_statement(
    symbols: &mut SymbolTable,
    compound: &CompoundStatement,
    expected_return: Option<GlslType>,
) -> GlslResult<()> {
    // Push new scope for the compound statement
    symbols.push_scope();

    // Type check each statement
    // Track if we've encountered a return statement (which makes subsequent code unreachable)
    let mut has_return = false;
    for stmt in &compound.statement_list {
        if has_return {
            // Unreachable code after return - warn but don't error (GLSL allows this)
            // We could add a warning here in the future
        }
        let returns = type_check_statement_returns(symbols, stmt, expected_return)?;
        if returns {
            has_return = true;
        }
    }

    // Pop scope
    symbols.pop_scope();

    Ok(())
}

/// Type check a statement and return whether it returns (makes subsequent code unreachable).
pub fn type_check_statement_returns(
    symbols: &mut SymbolTable,
    stmt: &Statement,
    expected_return: Option<GlslType>,
) -> GlslResult<bool> {
    match stmt {
        Statement::Simple(simple) => {
            type_check_simple_statement(symbols, simple, expected_return)?;
            // Check if this is a return statement or an if/else where both branches return
            match simple.as_ref() {
                SimpleStatement::Jump(JumpStatement::Return(_)) => Ok(true),
                SimpleStatement::Selection(sel) => {
                    crate::control::typecheck::selection_statement_returns(
                        symbols,
                        sel,
                        expected_return,
                    )
                }
                _ => Ok(false),
            }
        }
        Statement::Compound(compound) => {
            // Push scope for the compound statement
            symbols.push_scope();

            // Type check each statement and track if any returns
            let mut has_return = false;
            for stmt in &compound.statement_list {
                let returns = type_check_statement_returns(symbols, stmt, expected_return)?;
                if returns {
                    has_return = true;
                }
            }

            // Pop scope
            symbols.pop_scope();

            Ok(has_return)
        }
    }
}

/// Type check a simple statement.
pub fn type_check_simple_statement(
    symbols: &mut SymbolTable,
    simple: &SimpleStatement,
    expected_return: Option<GlslType>,
) -> GlslResult<()> {
    match simple {
        SimpleStatement::Declaration(decl) => type_check_declaration(symbols, decl),
        SimpleStatement::Expression(expr_stmt) => {
            if let Some(expr) = expr_stmt {
                // Expression statements allow void expressions (function calls that return void)
                // So we check the expression but allow void function calls
                match type_check_expr(expr, symbols) {
                    Ok(_) => {
                        // Expression has a value - that's fine
                    }
                    Err(e) => {
                        // Check if it's a void function call error - if so, allow it
                        if matches!(&e, GlslError::VoidFunctionCall(_)) {
                            // Void function call in expression statement - this is allowed
                            // Don't return error
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
            Ok(())
        }
        SimpleStatement::Selection(sel) => {
            type_check_selection_statement(symbols, sel, expected_return)
        }
        SimpleStatement::Iteration(iter) => {
            type_check_iteration_statement(symbols, iter, expected_return)
        }
        SimpleStatement::Jump(jump) => type_check_jump_statement(symbols, jump, expected_return),
        SimpleStatement::Switch(_) => Err(GlslError::type_error("Switch not supported")),
        SimpleStatement::CaseLabel(_) => Err(GlslError::type_error("Case labels not supported")),
    }
}
