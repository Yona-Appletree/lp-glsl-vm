//! Type checking for GLSL expressions.

use alloc::{boxed::Box, format};

use glsl::syntax::Expr;

use crate::{
    error::{GlslError, GlslResult},
    symbols::SymbolTable,
    types::GlslType,
    util::{extract_type_from_fully_specified, extract_type_from_specifier},
};

/// Type check an expression.
///
/// Returns the type of the expression, or an error if type checking fails.
pub fn type_check_expr(expr: &Expr, symbols: &SymbolTable) -> GlslResult<GlslType> {
    match expr {
        // Literals
        Expr::IntConst(_) => Ok(GlslType::Int),
        Expr::BoolConst(_) => Ok(GlslType::Bool),
        Expr::UIntConst(_) => Err(GlslError::type_error("Unsigned integers not supported")),
        Expr::FloatConst(_) | Expr::DoubleConst(_) => {
            Err(GlslError::type_error("Floating point types not supported"))
        }

        // Variable reference
        Expr::Variable(ident) => {
            let name = ident.0.as_str();
            symbols
                .lookup_variable(name)
                .map(|var| var.ty)
                .ok_or_else(|| GlslError::type_error(format!("Undefined variable '{}'", name)))
        }

        // Unary operators
        Expr::Unary(op, operand) => {
            let operand_ty = type_check_expr(operand, symbols)?;
            type_check_unary_op(op.clone(), operand_ty)
        }

        // Binary operators
        Expr::Binary(op, left, right) => {
            let left_ty = type_check_expr(left, symbols)?;
            let right_ty = type_check_expr(right, symbols)?;
            type_check_binary_op(op.clone(), left_ty, right_ty)
        }

        // Function call
        Expr::FunCall(fun_ident, args) => {
            let name = match fun_ident {
                glsl::syntax::FunIdentifier::Identifier(ident) => ident.0.as_str(),
                _ => {
                    return Err(GlslError::type_error(
                        "Complex function identifiers not supported",
                    ))
                }
            };

            // Look up function signature
            let sig = symbols.lookup_function(name).ok_or_else(|| {
                GlslError::type_error(format!("Undefined function '{}'", name))
            })?;

            // Type check arguments
            if args.len() != sig.params.len() {
                return Err(GlslError::type_error(format!(
                    "Function '{}' expects {} arguments, got {}",
                    name,
                    sig.params.len(),
                    args.len()
                )));
            }

            for (arg_expr, param) in args.iter().zip(sig.params.iter()) {
                let arg_ty = type_check_expr(arg_expr, symbols)?;
                if arg_ty != param.ty {
                    return Err(GlslError::type_error(format!(
                        "Type mismatch: expected {}, got {}",
                        param.ty, arg_ty
                    )));
                }
            }

            // Return the function's return type (None for void)
            // Void function calls are allowed in expression statements
            sig.return_type
                .ok_or_else(|| GlslError::void_function_call(name))
        }

        // Assignment
        Expr::Assignment(lhs, _op, rhs) => {
            // Assignment can only be to a variable, not arbitrary expressions
            match lhs.as_ref() {
                Expr::Variable(_) => {
                    // Valid: assignment to variable
                }
                _ => {
                    return Err(GlslError::type_error(
                        "Assignment can only be to a variable, not to an expression",
                    ));
                }
            }

            let lhs_ty = type_check_expr(lhs, symbols)?;
            let rhs_ty = type_check_expr(rhs, symbols)?;
            if lhs_ty != rhs_ty {
                return Err(GlslError::type_error(format!(
                    "Assignment type mismatch: cannot assign {} to {}",
                    rhs_ty, lhs_ty
                )));
            }
            Ok(lhs_ty) // Assignment returns the assigned type
        }

        // Not supported in initial implementation
        Expr::Ternary(_, _, _) => Err(GlslError::type_error("Ternary operator not supported")),
        Expr::Bracket(_, _) => Err(GlslError::type_error("Array indexing not supported")),
        Expr::Dot(_, _) => Err(GlslError::type_error("Struct field access not supported")),
        Expr::PostInc(_) | Expr::PostDec(_) => Err(GlslError::type_error(
            "Post-increment/decrement not supported",
        )),
        Expr::Comma(_, _) => Err(GlslError::type_error("Comma operator not supported")),
    }
}

/// Type check a unary operator.
pub fn type_check_unary_op(
    op: glsl::syntax::UnaryOp,
    operand_ty: GlslType,
) -> GlslResult<GlslType> {
    match op {
        glsl::syntax::UnaryOp::Minus => {
            // Unary minus: requires int, returns int
            if operand_ty != GlslType::Int {
                return Err(GlslError::type_error(format!(
                    "Unary minus requires int, got {}",
                    operand_ty
                )));
            }
            Ok(GlslType::Int)
        }
        glsl::syntax::UnaryOp::Not => {
            // Logical not: requires bool, returns bool
            if operand_ty != GlslType::Bool {
                return Err(GlslError::type_error(format!(
                    "Logical not requires bool, got {}",
                    operand_ty
                )));
            }
            Ok(GlslType::Bool)
        }
        _ => Err(GlslError::type_error("Unsupported unary operator")),
    }
}

/// Type check a binary operator.
pub fn type_check_binary_op(
    op: glsl::syntax::BinaryOp,
    left_ty: GlslType,
    right_ty: GlslType,
) -> GlslResult<GlslType> {
    // Arithmetic operators: require int, int, return int
    match op {
        glsl::syntax::BinaryOp::Add
        | glsl::syntax::BinaryOp::Sub
        | glsl::syntax::BinaryOp::Mult
        | glsl::syntax::BinaryOp::Div
        | glsl::syntax::BinaryOp::Mod => {
            if left_ty != GlslType::Int || right_ty != GlslType::Int {
                return Err(GlslError::type_error(format!(
                    "Arithmetic operator requires int, int, got {}, {}",
                    left_ty, right_ty
                )));
            }
            Ok(GlslType::Int)
        }

        // Comparison operators: require matching types, return bool
        glsl::syntax::BinaryOp::LT
        | glsl::syntax::BinaryOp::GT
        | glsl::syntax::BinaryOp::LTE
        | glsl::syntax::BinaryOp::GTE
        | glsl::syntax::BinaryOp::Equal
        | glsl::syntax::BinaryOp::NonEqual => {
            if left_ty != right_ty {
                return Err(GlslError::type_error(format!(
                    "Comparison operator requires matching types, got {}, {}",
                    left_ty, right_ty
                )));
            }
            Ok(GlslType::Bool)
        }

        // Logical operators: require bool, bool, return bool
        glsl::syntax::BinaryOp::And | glsl::syntax::BinaryOp::Or => {
            if left_ty != GlslType::Bool || right_ty != GlslType::Bool {
                return Err(GlslError::type_error(format!(
                    "Logical operator requires bool, bool, got {}, {}",
                    left_ty, right_ty
                )));
            }
            Ok(GlslType::Bool)
        }

        _ => Err(GlslError::type_error("Unsupported binary operator")),
    }
}

