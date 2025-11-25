//! Modules for multi-function programs.

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::fmt;

use crate::Function;

/// A module containing multiple functions.
///
/// A module is the top-level unit of compilation, containing:
/// - Multiple functions (by name)
/// - An optional entry point function
#[derive(Debug, Clone)]
pub struct Module {
    /// Functions in this module, indexed by name.
    pub functions: BTreeMap<String, Function>,
    /// Name of the entry function (if any).
    pub entry_function: Option<String>,
}

impl Module {
    /// Create a new empty module.
    pub fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            entry_function: None,
        }
    }

    /// Add a function to this module.
    ///
    /// # Arguments
    ///
    /// * `name` - Function name (must be unique)
    /// * `func` - The function to add
    ///
    /// # Panics
    ///
    /// Panics if a function with the same name already exists.
    pub fn add_function(&mut self, name: String, func: Function) {
        if self.functions.contains_key(&name) {
            panic!("Function '{}' already exists in module", name);
        }
        self.functions.insert(name, func);
    }

    /// Get a function by name.
    pub fn get_function(&self, name: &str) -> Option<&Function> {
        self.functions.get(name)
    }

    /// Get a mutable reference to a function by name.
    pub fn get_function_mut(&mut self, name: &str) -> Option<&mut Function> {
        self.functions.get_mut(name)
    }

    /// Set the entry function.
    ///
    /// # Panics
    ///
    /// Panics if the function doesn't exist in the module.
    pub fn set_entry_function(&mut self, name: String) {
        if !self.functions.contains_key(&name) {
            panic!("Function '{}' does not exist in module", name);
        }
        self.entry_function = Some(name);
    }

    /// Get the entry function.
    pub fn entry_function(&self) -> Option<&Function> {
        self.entry_function
            .as_ref()
            .and_then(|name| self.functions.get(name))
    }

    /// Get the number of functions in this module.
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    /// Get all function names.
    pub fn function_names(&self) -> Vec<&String> {
        self.functions.keys().collect()
    }
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "module {{")?;

        if let Some(entry) = &self.entry_function {
            writeln!(f, "  entry: @{}", entry)?;
        }

        // Print each function
        for (name, func) in &self.functions {
            writeln!(f)?;
            // Temporarily set the function name for display
            let mut func_with_name = func.clone();
            func_with_name.set_name(name.clone());
            write!(f, "{}", func_with_name)?;
        }

        writeln!(f, "}}")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Signature, Type};

    #[test]
    fn test_module_creation() {
        let module = Module::new();
        assert_eq!(module.function_count(), 0);
        assert_eq!(module.entry_function(), None);
    }

    #[test]
    fn test_module_add_function() {
        let mut module = Module::new();
        let sig = Signature::new(vec![Type::I32], vec![Type::I32]);
        let func = Function::new(sig);

        module.add_function("test".to_string(), func);
        assert_eq!(module.function_count(), 1);
        assert!(module.get_function("test").is_some());
    }

    #[test]
    fn test_module_entry_function() {
        let mut module = Module::new();
        let sig = Signature::empty();
        let func = Function::new(sig);

        module.add_function("main".to_string(), func);
        module.set_entry_function("main".to_string());

        assert!(module.entry_function().is_some());
        assert_eq!(module.entry_function.as_ref().unwrap(), "main");
    }

    #[test]
    #[should_panic(expected = "Function 'test' already exists")]
    fn test_module_duplicate_function() {
        let mut module = Module::new();
        let sig = Signature::empty();
        let func1 = Function::new(sig.clone());
        let func2 = Function::new(sig);

        module.add_function("test".to_string(), func1);
        module.add_function("test".to_string(), func2);
    }

    #[test]
    #[should_panic(expected = "Function 'nonexistent' does not exist")]
    fn test_module_invalid_entry_function() {
        let mut module = Module::new();
        module.set_entry_function("nonexistent".to_string());
    }
}

