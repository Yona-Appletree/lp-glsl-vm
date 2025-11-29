//! Symbol table for GLSL type checking.
//!
//! This module provides symbol table functionality for tracking
//! function signatures, variable declarations, and scoping.

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

use crate::types::GlslType;

/// Parameter qualifier for function parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParameterQualifier {
    /// Input parameter (default, pass by value)
    In,
    /// Output parameter (pass by reference, caller allocates)
    Out,
    /// Input/output parameter (pass by reference, caller allocates)
    InOut,
}

impl ParameterQualifier {
    /// Get the default qualifier (In).
    pub fn default() -> Self {
        ParameterQualifier::In
    }

    /// Check if this parameter is passed by reference.
    pub fn is_by_reference(self) -> bool {
        matches!(self, ParameterQualifier::Out | ParameterQualifier::InOut)
    }
}

/// Function parameter information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// Parameter qualifier (in, out, inout)
    pub qualifier: ParameterQualifier,
    /// Parameter type
    pub ty: GlslType,
    /// Parameter name
    pub name: String,
}

/// Function signature information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    /// Function name
    pub name: String,
    /// Function parameters
    pub params: Vec<Parameter>,
    /// Return type (None for void)
    pub return_type: Option<GlslType>,
}

/// Variable information in a scope.
#[derive(Debug, Clone)]
pub struct Variable {
    /// Variable type
    pub ty: GlslType,
    /// Variable name
    pub name: String,
}

/// Scope for variable declarations.
#[derive(Debug, Clone)]
struct Scope {
    /// Variables declared in this scope
    variables: BTreeMap<String, Variable>,
}

impl Scope {
    /// Create a new empty scope.
    fn new() -> Self {
        Self {
            variables: BTreeMap::new(),
        }
    }

    /// Declare a variable in this scope.
    ///
    /// Returns `Err` if the variable is already declared.
    fn declare(&mut self, name: String, ty: GlslType) -> Result<(), String> {
        if self.variables.contains_key(&name) {
            return Err(format!(
                "Variable '{}' already declared in this scope",
                name
            ));
        }
        self.variables.insert(
            name.clone(),
            Variable {
                name: name.clone(),
                ty,
            },
        );
        Ok(())
    }

    /// Look up a variable in this scope.
    fn lookup(&self, name: &str) -> Option<&Variable> {
        self.variables.get(name)
    }
}

/// Symbol table for tracking functions and variables.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    /// Function signatures indexed by name
    functions: BTreeMap<String, FunctionSignature>,
    /// Stack of scopes for variable lookup
    scopes: Vec<Scope>,
}

impl SymbolTable {
    /// Create a new empty symbol table.
    pub fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            scopes: Vec::new(),
        }
    }

    /// Register a function signature.
    ///
    /// Returns `Err` if a function with the same name is already registered.
    pub fn register_function(&mut self, sig: FunctionSignature) -> Result<(), String> {
        if self.functions.contains_key(&sig.name) {
            return Err(format!("Function '{}' already defined", sig.name));
        }
        self.functions.insert(sig.name.clone(), sig);
        Ok(())
    }

    /// Look up a function signature by name.
    pub fn lookup_function(&self, name: &str) -> Option<&FunctionSignature> {
        self.functions.get(name)
    }

    /// Push a new scope onto the scope stack.
    pub fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// Pop the topmost scope from the scope stack.
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Declare a variable in the current scope.
    ///
    /// Returns `Err` if the variable is already declared in the current scope.
    pub fn declare_variable(&mut self, name: String, ty: GlslType) -> Result<(), String> {
        if let Some(scope) = self.scopes.last_mut() {
            scope.declare(name, ty)
        } else {
            Err("No active scope".to_string())
        }
    }

    /// Look up a variable, searching from the current scope outward.
    ///
    /// Returns `None` if the variable is not found in any scope.
    pub fn lookup_variable(&self, name: &str) -> Option<&Variable> {
        // Search from innermost scope to outermost
        for scope in self.scopes.iter().rev() {
            if let Some(var) = scope.lookup(name) {
                return Some(var);
            }
        }
        None
    }

    /// Get the number of active scopes.
    pub fn scope_depth(&self) -> usize {
        self.scopes.len()
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_parameter_qualifier() {
        assert!(!ParameterQualifier::In.is_by_reference());
        assert!(ParameterQualifier::Out.is_by_reference());
        assert!(ParameterQualifier::InOut.is_by_reference());
    }

    #[test]
    fn test_symbol_table_function_registration() {
        let mut table = SymbolTable::new();

        let sig = FunctionSignature {
            name: "add".to_string(),
            params: vec![
                Parameter {
                    qualifier: ParameterQualifier::In,
                    ty: GlslType::Int,
                    name: "x".to_string(),
                },
                Parameter {
                    qualifier: ParameterQualifier::In,
                    ty: GlslType::Int,
                    name: "y".to_string(),
                },
            ],
            return_type: Some(GlslType::Int),
        };

        assert!(table.register_function(sig.clone()).is_ok());
        assert_eq!(table.lookup_function("add"), Some(&sig));

        // Duplicate registration should fail
        assert!(table.register_function(sig).is_err());
    }

    #[test]
    fn test_symbol_table_scoping() {
        let mut table = SymbolTable::new();

        // Push outer scope
        table.push_scope();
        table
            .declare_variable("x".to_string(), GlslType::Int)
            .unwrap();

        // Push inner scope
        table.push_scope();
        table
            .declare_variable("y".to_string(), GlslType::Bool)
            .unwrap();

        // Can access both variables
        assert!(table.lookup_variable("x").is_some());
        assert!(table.lookup_variable("y").is_some());

        // Pop inner scope
        table.pop_scope();

        // y should no longer be accessible
        assert!(table.lookup_variable("y").is_none());
        // x should still be accessible
        assert!(table.lookup_variable("x").is_some());
    }

    #[test]
    fn test_symbol_table_variable_shadowing() {
        let mut table = SymbolTable::new();

        table.push_scope();
        table
            .declare_variable("x".to_string(), GlslType::Int)
            .unwrap();

        table.push_scope();
        // Can declare x again in inner scope (shadowing)
        table
            .declare_variable("x".to_string(), GlslType::Bool)
            .unwrap();

        // Should get the inner x (bool)
        let var = table.lookup_variable("x").unwrap();
        assert_eq!(var.ty, GlslType::Bool);

        table.pop_scope();

        // Should get the outer x (int)
        let var = table.lookup_variable("x").unwrap();
        assert_eq!(var.ty, GlslType::Int);
    }

    #[test]
    fn test_symbol_table_duplicate_declaration() {
        let mut table = SymbolTable::new();

        table.push_scope();
        table
            .declare_variable("x".to_string(), GlslType::Int)
            .unwrap();

        // Cannot declare x again in the same scope
        assert!(table
            .declare_variable("x".to_string(), GlslType::Bool)
            .is_err());
    }

    #[test]
    fn test_symbol_table_no_scope() {
        let mut table = SymbolTable::new();

        // Cannot declare variable without a scope
        assert!(table
            .declare_variable("x".to_string(), GlslType::Int)
            .is_err());
    }
}
