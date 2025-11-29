//! Type checking for GLSL expressions and statements.
//!
//! This module provides type checking functionality that validates
//! GLSL code and builds symbol tables.

use alloc::{boxed::Box, format, vec::Vec};

use glsl::syntax::{
    CompoundStatement, Declaration, Expr, ForInitStatement, FunctionDefinition,
    FunctionParameterDeclaration, IterationStatement, JumpStatement, SelectionRestStatement,
    SelectionStatement, SimpleStatement, Statement,
};

use crate::{
    error::{GlslError, GlslResult},
    symbols::{FunctionSignature, Parameter, ParameterQualifier, SymbolTable},
    types::GlslType,
};

/// Type checker context.
///
/// This holds the symbol table and provides methods for type checking
/// expressions and statements.
pub struct TypeChecker {
    /// Symbol table for function and variable lookup
    symbols: SymbolTable,
}

impl TypeChecker {
    /// Create a new type checker.
    pub fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
        }
    }

    /// Get a reference to the symbol table.
    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    /// Get a mutable reference to the symbol table.
    pub fn symbols_mut(&mut self) -> &mut SymbolTable {
        &mut self.symbols
    }

    /// Extract function signatures from parsed functions and register them.
    ///
    /// This should be called before type checking function bodies to ensure
    /// all functions are available for lookup.
    pub fn register_functions(
        &mut self,
        functions: &[crate::parser::FunctionInfo],
    ) -> GlslResult<()> {
        for func_info in functions {
            let sig = Self::extract_function_signature(&func_info.definition)?;
            self.symbols
                .register_function(sig)
                .map_err(|e| GlslError::type_error(e))?;
        }
        Ok(())
    }

    /// Extract function signature from a function definition AST node.
    pub fn extract_function_signature(
        func_def: &FunctionDefinition,
    ) -> GlslResult<FunctionSignature> {
        let name = func_def.prototype.name.0.clone();

        // Extract return type
        let return_type =
            if let Some(ty) = Self::extract_type_from_fully_specified(&func_def.prototype.ty) {
                Some(ty?)
            } else {
                None // void
            };

        // Extract parameters
        let mut params = Vec::new();
        for param_decl in &func_def.prototype.parameters {
            let param = Self::extract_parameter(param_decl)?;
            params.push(param);
        }

        Ok(FunctionSignature {
            name,
            params,
            return_type,
        })
    }

    /// Extract parameter from parameter declaration.
    fn extract_parameter(param_decl: &FunctionParameterDeclaration) -> GlslResult<Parameter> {
        match param_decl {
            FunctionParameterDeclaration::Named(qualifier_opt, declarator) => {
                // Extract qualifier (in, out, inout)
                let qualifier = Self::extract_parameter_qualifier(qualifier_opt);

                // Extract type
                let ty = Self::extract_type_from_specifier(&declarator.ty)
                    .ok_or_else(|| GlslError::type_error("Unsupported parameter type"))?;

                // Extract name
                let name = declarator.ident.ident.0.clone();

                Ok(Parameter {
                    qualifier,
                    ty,
                    name,
                })
            }
            FunctionParameterDeclaration::Unnamed(_qualifier_opt, _ty_spec) => {
                // Unnamed parameters are not supported in our initial implementation
                Err(GlslError::type_error("Unnamed parameters not supported"))
            }
        }
    }

    /// Extract parameter qualifier from optional qualifier.
    fn extract_parameter_qualifier(
        qualifier_opt: &Option<glsl::syntax::TypeQualifier>,
    ) -> ParameterQualifier {
        if let Some(qualifier) = qualifier_opt {
            for spec in &qualifier.qualifiers.0 {
                match spec {
                    glsl::syntax::TypeQualifierSpec::Storage(storage) => match storage {
                        glsl::syntax::StorageQualifier::In => return ParameterQualifier::In,
                        glsl::syntax::StorageQualifier::Out => return ParameterQualifier::Out,
                        glsl::syntax::StorageQualifier::InOut => return ParameterQualifier::InOut,
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        ParameterQualifier::default() // Default to 'in'
    }

    /// Extract type from fully specified type (with qualifiers).
    fn extract_type_from_fully_specified(
        ty: &glsl::syntax::FullySpecifiedType,
    ) -> Option<GlslResult<GlslType>> {
        Self::extract_type_from_specifier(&ty.ty).map(Ok)
    }

    /// Extract type from type specifier.
    fn extract_type_from_specifier(ty_spec: &glsl::syntax::TypeSpecifier) -> Option<GlslType> {
        GlslType::from_glsl_type_specifier(&ty_spec.ty)
    }

    /// Type check a function body.
    ///
    /// This type checks all statements in the function body and validates
    /// that return statements match the function's return type.
    pub fn type_check_function_body(&mut self, func_def: &FunctionDefinition) -> GlslResult<()> {
        // Get function signature to check return type
        let sig = Self::extract_function_signature(func_def)?;
        let expected_return = sig.return_type;

        // Push function scope
        self.symbols.push_scope();

        // Add function parameters to scope
        for param in &sig.params {
            self.symbols
                .declare_variable(param.name.clone(), param.ty)
                .map_err(|e| GlslError::type_error(e))?;
        }

        // Type check the function body
        let body_stmt = Statement::Compound(Box::new(func_def.statement.clone()));
        let returns = self.type_check_statement_returns(&body_stmt, expected_return)?;

        // For non-void functions, validate that all code paths return a value
        // Note: This is a simple check - it only verifies that the function body
        // ends with a return. More sophisticated control flow analysis would be needed
        // to verify all paths return (e.g., if/else where both branches return).
        // For now, we rely on the code generator to add implicit returns if needed.
        // We'll be lenient and only check the last statement - if it's an if/else
        // or other control flow, we assume it's valid if type checking passed.
        if expected_return.is_some() && !returns {
            // Check if the last statement in the body could return
            let last_stmt = func_def.statement.statement_list.last();
            let might_return = last_stmt
                .map(|s| match s {
                    Statement::Simple(simple) => {
                        matches!(
                            simple.as_ref(),
                            SimpleStatement::Jump(JumpStatement::Return(_))
                                | SimpleStatement::Selection(_)
                        )
                    }
                    Statement::Compound(_) => true, // Compound statements might contain returns
                })
                .unwrap_or(false);

            if !might_return {
                return Err(GlslError::type_error(format!(
                    "Function '{}' must return a value of type {}",
                    sig.name,
                    expected_return.unwrap()
                )));
            }
        }

        // Pop function scope
        self.symbols.pop_scope();

        Ok(())
    }

    /// Type check a statement.
    ///
    /// `expected_return` is the expected return type for return statements
    /// (None for void functions).
    fn type_check_statement(
        &mut self,
        stmt: &Statement,
        expected_return: Option<GlslType>,
    ) -> GlslResult<()> {
        match stmt {
            Statement::Simple(simple) => self.type_check_simple_statement(simple, expected_return),
            Statement::Compound(compound) => {
                self.type_check_compound_statement(compound, expected_return)
            }
        }
    }

    /// Type check a compound statement (block).
    fn type_check_compound_statement(
        &mut self,
        compound: &CompoundStatement,
        expected_return: Option<GlslType>,
    ) -> GlslResult<()> {
        // Push new scope for the compound statement
        self.symbols.push_scope();

        // Type check each statement
        // Track if we've encountered a return statement (which makes subsequent code unreachable)
        let mut has_return = false;
        for stmt in &compound.statement_list {
            if has_return {
                // Unreachable code after return - warn but don't error (GLSL allows this)
                // We could add a warning here in the future
            }
            let returns = self.type_check_statement_returns(stmt, expected_return)?;
            if returns {
                has_return = true;
            }
        }

        // Pop scope
        self.symbols.pop_scope();

        Ok(())
    }

    /// Type check a statement and return whether it returns (makes subsequent code unreachable).
    fn type_check_statement_returns(
        &mut self,
        stmt: &Statement,
        expected_return: Option<GlslType>,
    ) -> GlslResult<bool> {
        match stmt {
            Statement::Simple(simple) => {
                self.type_check_simple_statement(simple, expected_return)?;
                // Check if this is a return statement or an if/else where both branches return
                match simple.as_ref() {
                    SimpleStatement::Jump(JumpStatement::Return(_)) => Ok(true),
                    SimpleStatement::Selection(sel) => {
                        self.selection_statement_returns(sel, expected_return)
                    }
                    _ => Ok(false),
                }
            }
            Statement::Compound(compound) => {
                // Push scope for the compound statement
                self.symbols.push_scope();

                // Type check each statement and track if any returns
                let mut has_return = false;
                for stmt in &compound.statement_list {
                    let returns = self.type_check_statement_returns(stmt, expected_return)?;
                    if returns {
                        has_return = true;
                    }
                }

                // Pop scope
                self.symbols.pop_scope();

                Ok(has_return)
            }
        }
    }

    /// Type check a simple statement.
    fn type_check_simple_statement(
        &mut self,
        simple: &SimpleStatement,
        expected_return: Option<GlslType>,
    ) -> GlslResult<()> {
        match simple {
            SimpleStatement::Declaration(decl) => self.type_check_declaration(decl),
            SimpleStatement::Expression(expr_stmt) => {
                if let Some(expr) = expr_stmt {
                    // Expression statements allow void expressions (function calls that return void)
                    // So we check the expression but allow void function calls
                    match self.type_check_expr(expr) {
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
                self.type_check_selection_statement(sel, expected_return)
            }
            SimpleStatement::Iteration(iter) => {
                self.type_check_iteration_statement(iter, expected_return)
            }
            SimpleStatement::Jump(jump) => self.type_check_jump_statement(jump, expected_return),
            SimpleStatement::Switch(_) => Err(GlslError::type_error("Switch not supported")),
            SimpleStatement::CaseLabel(_) => {
                Err(GlslError::type_error("Case labels not supported"))
            }
        }
    }

    /// Type check a variable declaration.
    fn type_check_declaration(&mut self, decl: &Declaration) -> GlslResult<()> {
        match decl {
            Declaration::InitDeclaratorList(list) => {
                // Extract type from head declaration
                let ty = Self::extract_type_from_fully_specified(&list.head.ty)
                    .ok_or_else(|| GlslError::type_error("Unsupported variable type"))??;

                // Declare the head variable
                if let Some(name) = &list.head.name {
                    // Check initializer if present
                    if let Some(init) = &list.head.initializer {
                        let init_ty = self.type_check_initializer(init)?;
                        if init_ty != ty {
                            return Err(GlslError::type_error(format!(
                                "Variable '{}' type mismatch: declared as {}, initialized with {}",
                                name.0, ty, init_ty
                            )));
                        }
                    }

                    self.symbols
                        .declare_variable(name.0.clone(), ty)
                        .map_err(|e| GlslError::type_error(e))?;
                }

                // Declare tail variables
                for decl_no_type in &list.tail {
                    // Tail declarations use the same type as head
                    if let Some(init) = &decl_no_type.initializer {
                        let init_ty = self.type_check_initializer(init)?;
                        if init_ty != ty {
                            return Err(GlslError::type_error(format!(
                                "Variable type mismatch: declared as {}, initialized with {}",
                                ty, init_ty
                            )));
                        }
                    }

                    self.symbols
                        .declare_variable(decl_no_type.ident.ident.0.clone(), ty)
                        .map_err(|e| GlslError::type_error(e))?;
                }

                Ok(())
            }
            _ => Err(GlslError::type_error("Unsupported declaration type")),
        }
    }

    /// Type check an initializer.
    fn type_check_initializer(&self, init: &glsl::syntax::Initializer) -> GlslResult<GlslType> {
        match init {
            glsl::syntax::Initializer::Simple(expr) => self.type_check_expr(expr),
            glsl::syntax::Initializer::List(_) => {
                Err(GlslError::type_error("List initializers not supported"))
            }
        }
    }

    /// Type check a selection statement (if/else).
    fn type_check_selection_statement(
        &mut self,
        sel: &SelectionStatement,
        expected_return: Option<GlslType>,
    ) -> GlslResult<()> {
        // Condition must be bool
        let cond_ty = self.type_check_expr(&sel.cond)?;
        if cond_ty != GlslType::Bool {
            return Err(GlslError::type_error(format!(
                "If condition must be bool, got {}",
                cond_ty
            )));
        }

        // Type check the branches
        match &sel.rest {
            SelectionRestStatement::Statement(true_stmt) => {
                self.type_check_statement(true_stmt, expected_return)?;
            }
            SelectionRestStatement::Else(true_stmt, false_stmt) => {
                self.type_check_statement(true_stmt, expected_return)?;
                self.type_check_statement(false_stmt, expected_return)?;
            }
        }

        Ok(())
    }

    /// Check if a selection statement returns (both branches return).
    fn selection_statement_returns(
        &mut self,
        sel: &SelectionStatement,
        expected_return: Option<GlslType>,
    ) -> GlslResult<bool> {
        match &sel.rest {
            SelectionRestStatement::Statement(true_stmt) => {
                // If-only: check if true branch returns
                self.type_check_statement_returns(true_stmt, expected_return)
            }
            SelectionRestStatement::Else(true_stmt, false_stmt) => {
                // If/else: both branches must return
                let true_returns = self.type_check_statement_returns(true_stmt, expected_return)?;
                let false_returns =
                    self.type_check_statement_returns(false_stmt, expected_return)?;
                Ok(true_returns && false_returns)
            }
        }
    }

    /// Type check an iteration statement (for/while).
    fn type_check_iteration_statement(
        &mut self,
        iter: &IterationStatement,
        expected_return: Option<GlslType>,
    ) -> GlslResult<()> {
        match iter {
            IterationStatement::While(cond, body) => {
                // Push scope for loop body
                self.symbols.push_scope();

                // Condition must be bool
                let cond_ty = match cond {
                    glsl::syntax::Condition::Expr(expr) => self.type_check_expr(expr)?,
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
                self.type_check_statement(body, expected_return)?;

                // Pop scope
                self.symbols.pop_scope();

                Ok(())
            }
            IterationStatement::DoWhile(body, cond_expr) => {
                // Push scope for loop body
                self.symbols.push_scope();

                // Type check body first
                self.type_check_statement(body, expected_return)?;

                // Condition must be bool
                let cond_ty = self.type_check_expr(cond_expr)?;
                if cond_ty != GlslType::Bool {
                    return Err(GlslError::type_error(format!(
                        "Do-while condition must be bool, got {}",
                        cond_ty
                    )));
                }

                // Pop scope
                self.symbols.pop_scope();

                Ok(())
            }
            IterationStatement::For(init, rest, body) => {
                // Push scope for for loop
                self.symbols.push_scope();

                // Type check initialization
                match init {
                    ForInitStatement::Expression(expr_opt) => {
                        if let Some(expr) = expr_opt {
                            self.type_check_expr(expr)?;
                        }
                    }
                    ForInitStatement::Declaration(decl) => {
                        self.type_check_declaration(decl)?;
                    }
                }

                // Type check condition (must be bool if present)
                if let Some(cond) = &rest.condition {
                    let cond_ty = match cond {
                        glsl::syntax::Condition::Expr(expr) => self.type_check_expr(expr)?,
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
                self.type_check_statement(body, expected_return)?;

                // Type check increment (if present)
                if let Some(post_expr) = &rest.post_expr {
                    self.type_check_expr(post_expr)?;
                }

                // Pop scope
                self.symbols.pop_scope();

                Ok(())
            }
        }
    }

    /// Type check a jump statement (return/break/continue).
    fn type_check_jump_statement(
        &self,
        jump: &JumpStatement,
        expected_return: Option<GlslType>,
    ) -> GlslResult<()> {
        match jump {
            JumpStatement::Return(expr_opt) => {
                match (expr_opt.as_ref(), expected_return) {
                    (None, None) => Ok(()), // void return
                    (Some(expr), Some(expected_ty)) => {
                        let actual_ty = self.type_check_expr(expr)?;
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

    /// Type check an expression.
    ///
    /// Returns the type of the expression, or an error if type checking fails.
    pub fn type_check_expr(&self, expr: &Expr) -> GlslResult<GlslType> {
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
                self.symbols
                    .lookup_variable(name)
                    .map(|var| var.ty)
                    .ok_or_else(|| GlslError::type_error(format!("Undefined variable '{}'", name)))
            }

            // Unary operators
            Expr::Unary(op, operand) => {
                let operand_ty = self.type_check_expr(operand)?;
                Self::type_check_unary_op(op.clone(), operand_ty)
            }

            // Binary operators
            Expr::Binary(op, left, right) => {
                let left_ty = self.type_check_expr(left)?;
                let right_ty = self.type_check_expr(right)?;
                Self::type_check_binary_op(op.clone(), left_ty, right_ty)
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
                let sig = self.symbols.lookup_function(name).ok_or_else(|| {
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
                    let arg_ty = self.type_check_expr(arg_expr)?;
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

                let lhs_ty = self.type_check_expr(lhs)?;
                let rhs_ty = self.type_check_expr(rhs)?;
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
    fn type_check_unary_op(
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
    fn type_check_binary_op(
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
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, string::String};

    use super::*;

    #[test]
    fn test_type_check_int_literal() {
        let checker = TypeChecker::new();
        let expr = Expr::IntConst(42);
        assert_eq!(checker.type_check_expr(&expr).unwrap(), GlslType::Int);
    }

    #[test]
    fn test_type_check_bool_literal() {
        let checker = TypeChecker::new();
        let expr = Expr::BoolConst(true);
        assert_eq!(checker.type_check_expr(&expr).unwrap(), GlslType::Bool);
    }

    #[test]
    fn test_type_check_unsupported_literal() {
        let checker = TypeChecker::new();
        let expr = Expr::FloatConst(3.14);
        assert!(checker.type_check_expr(&expr).is_err());
    }

    #[test]
    fn test_type_check_binary_add() {
        let checker = TypeChecker::new();
        let expr = Expr::Binary(
            glsl::syntax::BinaryOp::Add,
            Box::new(Expr::IntConst(10)),
            Box::new(Expr::IntConst(20)),
        );
        assert_eq!(checker.type_check_expr(&expr).unwrap(), GlslType::Int);
    }

    #[test]
    fn test_type_check_binary_add_type_mismatch() {
        let checker = TypeChecker::new();
        let expr = Expr::Binary(
            glsl::syntax::BinaryOp::Add,
            Box::new(Expr::IntConst(10)),
            Box::new(Expr::BoolConst(true)),
        );
        assert!(checker.type_check_expr(&expr).is_err());
    }

    #[test]
    fn test_type_check_binary_comparison() {
        let checker = TypeChecker::new();
        let expr = Expr::Binary(
            glsl::syntax::BinaryOp::LT,
            Box::new(Expr::IntConst(10)),
            Box::new(Expr::IntConst(20)),
        );
        assert_eq!(checker.type_check_expr(&expr).unwrap(), GlslType::Bool);
    }

    #[test]
    fn test_type_check_binary_logical() {
        let checker = TypeChecker::new();
        let expr = Expr::Binary(
            glsl::syntax::BinaryOp::And,
            Box::new(Expr::BoolConst(true)),
            Box::new(Expr::BoolConst(false)),
        );
        assert_eq!(checker.type_check_expr(&expr).unwrap(), GlslType::Bool);
    }

    #[test]
    fn test_type_check_unary_minus() {
        let checker = TypeChecker::new();
        let expr = Expr::Unary(glsl::syntax::UnaryOp::Minus, Box::new(Expr::IntConst(42)));
        assert_eq!(checker.type_check_expr(&expr).unwrap(), GlslType::Int);
    }

    #[test]
    fn test_type_check_unary_not() {
        let checker = TypeChecker::new();
        let expr = Expr::Unary(glsl::syntax::UnaryOp::Not, Box::new(Expr::BoolConst(true)));
        assert_eq!(checker.type_check_expr(&expr).unwrap(), GlslType::Bool);
    }

    #[test]
    fn test_type_check_variable_declaration() {
        let mut checker = TypeChecker::new();
        checker.symbols.push_scope();

        let decl =
            glsl::syntax::Declaration::InitDeclaratorList(glsl::syntax::InitDeclaratorList {
                head: glsl::syntax::SingleDeclaration {
                    ty: glsl::syntax::FullySpecifiedType {
                        qualifier: None,
                        ty: glsl::syntax::TypeSpecifier {
                            ty: glsl::syntax::TypeSpecifierNonArray::Int,
                            array_specifier: None,
                        },
                    },
                    name: Some(glsl::syntax::Identifier(String::from("x"))),
                    array_specifier: None,
                    initializer: Some(glsl::syntax::Initializer::Simple(Box::new(Expr::IntConst(
                        42,
                    )))),
                },
                tail: Vec::new(),
            });

        assert!(checker.type_check_declaration(&decl).is_ok());
        assert!(checker.symbols.lookup_variable("x").is_some());
    }

    #[test]
    fn test_type_check_if_statement() {
        let mut checker = TypeChecker::new();
        checker.symbols.push_scope();

        let if_stmt = glsl::syntax::SimpleStatement::Selection(glsl::syntax::SelectionStatement {
            cond: Box::new(Expr::BoolConst(true)),
            rest: glsl::syntax::SelectionRestStatement::Statement(Box::new(Statement::Simple(
                Box::new(glsl::syntax::SimpleStatement::Expression(None)),
            ))),
        });

        assert!(checker.type_check_simple_statement(&if_stmt, None).is_ok());
    }

    #[test]
    fn test_type_check_if_statement_bad_condition() {
        let mut checker = TypeChecker::new();
        checker.symbols.push_scope();

        let if_stmt = glsl::syntax::SimpleStatement::Selection(glsl::syntax::SelectionStatement {
            cond: Box::new(Expr::IntConst(42)), // Should be bool
            rest: glsl::syntax::SelectionRestStatement::Statement(Box::new(Statement::Simple(
                Box::new(glsl::syntax::SimpleStatement::Expression(None)),
            ))),
        });

        assert!(checker.type_check_simple_statement(&if_stmt, None).is_err());
    }

    #[test]
    fn test_type_check_return_statement() {
        let checker = TypeChecker::new();

        let return_stmt = glsl::syntax::JumpStatement::Return(Some(Box::new(Expr::IntConst(42))));

        assert!(checker
            .type_check_jump_statement(&return_stmt, Some(GlslType::Int))
            .is_ok());
    }

    #[test]
    fn test_type_check_return_statement_type_mismatch() {
        let checker = TypeChecker::new();

        let return_stmt =
            glsl::syntax::JumpStatement::Return(Some(Box::new(Expr::BoolConst(true))));

        assert!(checker
            .type_check_jump_statement(&return_stmt, Some(GlslType::Int))
            .is_err());
    }
}
