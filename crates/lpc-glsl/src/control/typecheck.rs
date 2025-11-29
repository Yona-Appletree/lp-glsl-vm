//! Type checking for GLSL control flow constructs.

use alloc::format;

use glsl::syntax::{
    ForInitStatement, IterationStatement, JumpStatement, SelectionRestStatement, SelectionStatement,
};

use crate::{
    decl::typecheck::type_check_declaration,
    error::{GlslError, GlslResult},
    expr::typecheck::type_check_expr,
    stmt::typecheck::{type_check_statement, type_check_statement_returns},
    symbols::SymbolTable,
    types::GlslType,
};

/// Type check a selection statement (if/else).
pub fn type_check_selection_statement(
    symbols: &mut SymbolTable,
    sel: &SelectionStatement,
    expected_return: Option<GlslType>,
) -> GlslResult<()> {
    // Condition must be bool
    let cond_ty = type_check_expr(&sel.cond, symbols)?;
    if cond_ty != GlslType::Bool {
        return Err(GlslError::type_error(format!(
            "If condition must be bool, got {}",
            cond_ty
        )));
    }

    // Type check the branches
    match &sel.rest {
        SelectionRestStatement::Statement(true_stmt) => {
            type_check_statement(symbols, true_stmt, expected_return)?;
        }
        SelectionRestStatement::Else(true_stmt, false_stmt) => {
            type_check_statement(symbols, true_stmt, expected_return)?;
            type_check_statement(symbols, false_stmt, expected_return)?;
        }
    }

    Ok(())
}

/// Check if a selection statement returns (both branches return).
pub fn selection_statement_returns(
    symbols: &mut SymbolTable,
    sel: &SelectionStatement,
    expected_return: Option<GlslType>,
) -> GlslResult<bool> {
    match &sel.rest {
        SelectionRestStatement::Statement(true_stmt) => {
            // If-only: check if true branch returns
            type_check_statement_returns(symbols, true_stmt, expected_return)
        }
        SelectionRestStatement::Else(true_stmt, false_stmt) => {
            // If/else: both branches must return
            let true_returns = type_check_statement_returns(symbols, true_stmt, expected_return)?;
            let false_returns = type_check_statement_returns(symbols, false_stmt, expected_return)?;
            Ok(true_returns && false_returns)
        }
    }
}

/// Type check an iteration statement (for/while).
pub fn type_check_iteration_statement(
    symbols: &mut SymbolTable,
    iter: &IterationStatement,
    expected_return: Option<GlslType>,
) -> GlslResult<()> {
    match iter {
        IterationStatement::While(cond, body) => {
            // Push scope for loop body
            symbols.push_scope();

            // Condition must be bool
            let cond_ty = match cond {
                glsl::syntax::Condition::Expr(expr) => type_check_expr(expr, symbols)?,
                glsl::syntax::Condition::Assignment(_, _, _) => {
                    return Err(GlslError::type_error(
                        "Assignment in while condition not supported",
                    ))
                }
            };
            if cond_ty != GlslType::Bool {
                return Err(GlslError::type_error(format!(
                    "While condition must be bool, got {}",
                    cond_ty
                )));
            }

            // Type check body
            type_check_statement(symbols, body, expected_return)?;

            // Pop scope
            symbols.pop_scope();

            Ok(())
        }
        IterationStatement::DoWhile(body, cond_expr) => {
            // Push scope for loop body
            symbols.push_scope();

            // Type check body first
            type_check_statement(symbols, body, expected_return)?;

            // Condition must be bool
            let cond_ty = type_check_expr(cond_expr, symbols)?;
            if cond_ty != GlslType::Bool {
                return Err(GlslError::type_error(format!(
                    "Do-while condition must be bool, got {}",
                    cond_ty
                )));
            }

            // Pop scope
            symbols.pop_scope();

            Ok(())
        }
        IterationStatement::For(init, rest, body) => {
            // Push scope for for loop
            symbols.push_scope();

            // Type check initialization
            match init {
                ForInitStatement::Expression(expr_opt) => {
                    if let Some(expr) = expr_opt {
                        type_check_expr(expr, symbols)?;
                    }
                }
                ForInitStatement::Declaration(decl) => {
                    type_check_declaration(symbols, decl)?;
                }
            }

            // Type check condition (must be bool if present)
            if let Some(cond) = &rest.condition {
                let cond_ty = match cond {
                    glsl::syntax::Condition::Expr(expr) => type_check_expr(expr, symbols)?,
                    glsl::syntax::Condition::Assignment(_, _, _) => {
                        return Err(GlslError::type_error(
                            "Assignment in for condition not supported",
                        ))
                    }
                };
                if cond_ty != GlslType::Bool {
                    return Err(GlslError::type_error(format!(
                        "For condition must be bool, got {}",
                        cond_ty
                    )));
                }
            }

            // Type check body
            type_check_statement(symbols, body, expected_return)?;

            // Type check increment (if present)
            if let Some(post_expr) = &rest.post_expr {
                type_check_expr(post_expr, symbols)?;
            }

            // Pop scope
            symbols.pop_scope();

            Ok(())
        }
    }
}

/// Type check a jump statement (return/break/continue).
pub fn type_check_jump_statement(
    symbols: &SymbolTable,
    jump: &JumpStatement,
    expected_return: Option<GlslType>,
) -> GlslResult<()> {
    match jump {
        JumpStatement::Return(expr_opt) => {
            match (expr_opt.as_ref(), expected_return) {
                (None, None) => Ok(()), // void return
                (Some(expr), Some(expected_ty)) => {
                    let actual_ty = type_check_expr(expr, symbols)?;
                    if actual_ty != expected_ty {
                        return Err(GlslError::type_error(format!(
                            "Return type mismatch: expected {}, got {}",
                            expected_ty, actual_ty
                        )));
                    }
                    Ok(())
                }
                (None, Some(_)) => Err(GlslError::type_error(
                    "Function expects return value but none provided",
                )),
                (Some(_), None) => {
                    Err(GlslError::type_error("Void function cannot return a value"))
                }
            }
        }
        JumpStatement::Break | JumpStatement::Continue => {
            Err(GlslError::type_error("Break/continue not supported"))
        }
        JumpStatement::Discard => Err(GlslError::type_error("Discard not supported")),
    }
}

