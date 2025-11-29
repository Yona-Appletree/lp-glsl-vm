//! Utilities for finding variable references in expressions and statements.

use alloc::{collections::BTreeSet, string::String};

use glsl::syntax::{
    Expr, ForInitStatement, IterationStatement, SelectionRestStatement, SimpleStatement, Statement,
};

/// Find all variable names referenced in an expression.
pub fn find_variable_references(expr: &Expr) -> BTreeSet<String> {
    let mut vars = BTreeSet::new();
    match expr {
        Expr::Variable(ident) => {
            vars.insert(ident.0.clone());
        }
        Expr::Unary(_, operand) => {
            vars.extend(find_variable_references(operand));
        }
        Expr::Binary(_, left, right) => {
            vars.extend(find_variable_references(left));
            vars.extend(find_variable_references(right));
        }
        Expr::Assignment(lhs, _, rhs) => {
            vars.extend(find_variable_references(lhs));
            vars.extend(find_variable_references(rhs));
        }
        Expr::FunCall(_, args) => {
            for arg in args {
                vars.extend(find_variable_references(arg));
            }
        }
        Expr::Ternary(cond, true_expr, false_expr) => {
            vars.extend(find_variable_references(cond));
            vars.extend(find_variable_references(true_expr));
            vars.extend(find_variable_references(false_expr));
        }
        Expr::Bracket(base, _index_spec) => {
            // Array indexing not supported, but we can still find variables in base
            vars.extend(find_variable_references(base));
        }
        Expr::Dot(base, _) => {
            vars.extend(find_variable_references(base));
        }
        Expr::PostInc(operand) | Expr::PostDec(operand) => {
            vars.extend(find_variable_references(operand));
        }
        Expr::Comma(left, right) => {
            vars.extend(find_variable_references(left));
            vars.extend(find_variable_references(right));
        }
        _ => {
            // Literals and other expressions don't reference variables
        }
    }
    vars
}

/// Find all variable names referenced in a statement.
pub fn find_variable_references_in_statement(stmt: &Statement) -> BTreeSet<String> {
    let mut vars = BTreeSet::new();
    match stmt {
        Statement::Simple(simple) => {
            match simple.as_ref() {
                SimpleStatement::Expression(expr_opt) => {
                    if let Some(expr) = expr_opt {
                        vars.extend(find_variable_references(expr));
                    }
                }
                SimpleStatement::Selection(sel) => {
                    vars.extend(find_variable_references(&sel.cond));
                    match &sel.rest {
                        SelectionRestStatement::Statement(true_stmt) => {
                            vars.extend(find_variable_references_in_statement(true_stmt));
                        }
                        SelectionRestStatement::Else(true_stmt, false_stmt) => {
                            vars.extend(find_variable_references_in_statement(true_stmt));
                            vars.extend(find_variable_references_in_statement(false_stmt));
                        }
                    }
                }
                SimpleStatement::Iteration(iter) => {
                    match iter {
                        IterationStatement::While(cond, body) => {
                            match cond {
                                glsl::syntax::Condition::Expr(expr) => {
                                    vars.extend(find_variable_references(expr));
                                }
                                _ => {}
                            }
                            vars.extend(find_variable_references_in_statement(body));
                        }
                        IterationStatement::DoWhile(body, cond_expr) => {
                            vars.extend(find_variable_references(cond_expr));
                            vars.extend(find_variable_references_in_statement(body));
                        }
                        IterationStatement::For(init, rest, body) => {
                            match init {
                                ForInitStatement::Expression(expr_opt) => {
                                    if let Some(expr) = expr_opt {
                                        vars.extend(find_variable_references(expr));
                                    }
                                }
                                ForInitStatement::Declaration(_decl) => {
                                    // Declaration doesn't reference variables (it declares them)
                                }
                            }
                            if let Some(cond) = &rest.condition {
                                match cond {
                                    glsl::syntax::Condition::Expr(expr) => {
                                        vars.extend(find_variable_references(expr));
                                    }
                                    _ => {}
                                }
                            }
                            if let Some(post_expr) = &rest.post_expr {
                                vars.extend(find_variable_references(post_expr));
                            }
                            vars.extend(find_variable_references_in_statement(body));
                        }
                    }
                }
                _ => {}
            }
        }
        Statement::Compound(compound) => {
            for stmt in &compound.statement_list {
                vars.extend(find_variable_references_in_statement(stmt));
            }
        }
    }
    vars
}
