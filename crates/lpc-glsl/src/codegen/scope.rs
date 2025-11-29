//! Scope management for GLSL codegen.
//!
//! This module provides RAII-based scope tracking for variable declarations.

use alloc::{collections::BTreeSet, string::String, vec::Vec};

/// A lexical scope.
pub struct Scope {
    /// Variables declared in this scope
    variables: BTreeSet<String>,
    /// Cleanup actions (future: for destructors, exception handling)
    cleanups: Vec<CleanupAction>,
}

/// A cleanup action to perform when exiting a scope (future use).
#[derive(Debug, Clone)]
pub enum CleanupAction {
    // Future: destructor calls, exception handling, etc.
}

impl Scope {
    /// Create a new empty scope.
    pub fn new() -> Self {
        Self {
            variables: BTreeSet::new(),
            cleanups: Vec::new(),
        }
    }

    /// Declare a variable in this scope.
    pub fn declare(&mut self, name: String) {
        self.variables.insert(name);
    }

    /// Check if a variable is declared in this scope.
    pub fn contains(&self, name: &str) -> bool {
        self.variables.contains(name)
    }

    /// Get all variables declared in this scope.
    pub fn variables(&self) -> &BTreeSet<String> {
        &self.variables
    }
}

/// Stack of scopes.
pub struct ScopeStack {
    scopes: Vec<Scope>,
}

impl ScopeStack {
    /// Create a new empty scope stack.
    pub fn new() -> Self {
        Self { scopes: Vec::new() }
    }

    /// Push a new scope onto the stack.
    pub fn push(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// Pop a scope from the stack, returning it.
    pub fn pop(&mut self) -> Option<Scope> {
        self.scopes.pop()
    }

    /// Declare a variable in the current scope.
    pub fn declare(&mut self, name: String) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.declare(name);
        }
    }

    /// Check if a variable is declared in any scope.
    pub fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }

    /// Get the current scope (if any).
    pub fn current(&self) -> Option<&Scope> {
        self.scopes.last()
    }

    /// Get the current scope mutably (if any).
    pub fn current_mut(&mut self) -> Option<&mut Scope> {
        self.scopes.last_mut()
    }
}

/// RAII guard for scope entry/exit.
pub struct ScopeGuard<'a> {
    stack: &'a mut ScopeStack,
}

impl<'a> ScopeGuard<'a> {
    /// Create a new scope guard, pushing a new scope.
    pub fn new(stack: &'a mut ScopeStack) -> Self {
        stack.push();
        Self { stack }
    }
}

impl<'a> Drop for ScopeGuard<'a> {
    fn drop(&mut self) {
        self.stack.pop();
    }
}
