//! Type checking for function-level constructs.

use alloc::{boxed::Box, format, vec::Vec};

use glsl::syntax::{FunctionDefinition, FunctionParameterDeclaration};

use crate::{
    error::{GlslError, GlslResult},
    parser::FunctionInfo,
    stmt::typecheck::type_check_statement_returns,
    symbols::{FunctionSignature, Parameter, ParameterQualifier, SymbolTable},
    types::GlslType,
    util::{extract_type_from_fully_specified, extract_type_from_specifier},
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
    pub fn register_functions(&mut self, functions: &[FunctionInfo]) -> GlslResult<()> {
        for func_info in functions {
            let sig = extract_function_signature(&func_info.definition)?;
            self.symbols
                .register_function(sig)
                .map_err(|e| GlslError::type_error(e))?;
        }
        Ok(())
    }

    /// Type check a function body.
    ///
    /// This type checks all statements in the function body and validates
    /// that return statements match the function's return type.
    pub fn type_check_function_body(&mut self, func_def: &FunctionDefinition) -> GlslResult<()> {
        // Get function signature to check return type
        let sig = extract_function_signature(func_def)?;
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
        let body_stmt = glsl::syntax::Statement::Compound(Box::new(func_def.statement.clone()));
        let returns = type_check_statement_returns(&mut self.symbols, &body_stmt, expected_return)?;

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
                    glsl::syntax::Statement::Simple(simple) => {
                        matches!(
                            simple.as_ref(),
                            glsl::syntax::SimpleStatement::Jump(glsl::syntax::JumpStatement::Return(_))
                                | glsl::syntax::SimpleStatement::Selection(_)
                        )
                    }
                    glsl::syntax::Statement::Compound(_) => true, // Compound statements might contain returns
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
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract function signature from a function definition AST node.
pub fn extract_function_signature(func_def: &FunctionDefinition) -> GlslResult<FunctionSignature> {
    let name = func_def.prototype.name.0.clone();

    // Extract return type
    let return_type = extract_type_from_fully_specified(&func_def.prototype.ty);

    // Extract parameters
    let mut params = Vec::new();
    for param_decl in &func_def.prototype.parameters {
        let param = extract_parameter(param_decl)?;
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
            let qualifier = extract_parameter_qualifier(qualifier_opt);

            // Extract type
            let ty = extract_type_from_specifier(&declarator.ty)
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

