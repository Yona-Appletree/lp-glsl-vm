//! Symbol table for tracking function addresses and offsets.
//!
//! This module provides a symbol table that maps symbols to their code offsets
//! (for local functions) or addresses (for external functions resolved at runtime).

use alloc::{collections::BTreeMap, string::String};

/// Symbol identifier for functions and external references.
///
/// Similar to Cranelift's ExternalName, this enum supports different kinds
/// of symbols that may need different resolution strategies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Symbol {
    /// A function defined in the current module (local function).
    /// The string is the function name.
    Local(String),

    /// An external function (defined elsewhere, e.g., in JIT context).
    /// The string is the function name or identifier.
    /// For JIT, this might resolve to a function pointer provided by the runtime.
    External(String),

    /// A test case function name (for testing/debugging).
    /// Similar to Cranelift's TestCase variant.
    TestCase(String),
}

impl Symbol {
    /// Create a local symbol from a function name.
    pub fn local(name: impl Into<String>) -> Self {
        Self::Local(name.into())
    }

    /// Create an external symbol from a function name.
    pub fn external(name: impl Into<String>) -> Self {
        Self::External(name.into())
    }

    /// Create a test case symbol from a name.
    pub fn testcase(name: impl Into<String>) -> Self {
        Self::TestCase(name.into())
    }

    /// Get the name/identifier of this symbol.
    pub fn name(&self) -> &str {
        match self {
            Symbol::Local(name) => name,
            Symbol::External(name) => name,
            Symbol::TestCase(name) => name,
        }
    }
}

/// Symbol table for tracking function addresses/offsets.
///
/// Maps symbols to their code offsets (for local functions) or addresses
/// (for external functions resolved at runtime).
pub struct SymbolTable {
    /// Symbol -> code offset mapping (for local functions)
    local_symbols: BTreeMap<Symbol, u32>,

    /// Symbol -> address mapping (for external functions, resolved at runtime)
    /// Initially empty; populated by the JIT runtime or linker.
    external_symbols: BTreeMap<Symbol, u64>,
}

impl SymbolTable {
    /// Create a new empty symbol table.
    pub fn new() -> Self {
        Self {
            local_symbols: BTreeMap::new(),
            external_symbols: BTreeMap::new(),
        }
    }

    /// Add a local symbol (function defined in this module).
    pub fn add_local(&mut self, symbol: Symbol, offset: u32) {
        self.local_symbols.insert(symbol, offset);
    }

    /// Add an external symbol address (for JIT/runtime resolution).
    pub fn add_external(&mut self, symbol: Symbol, address: u64) {
        self.external_symbols.insert(symbol, address);
    }

    /// Look up a symbol's address/offset.
    ///
    /// Returns `None` if the symbol is not found.
    /// For local symbols, returns the code offset.
    /// For external symbols, returns the runtime address.
    pub fn lookup(&self, symbol: &Symbol) -> Option<u64> {
        if let Some(&offset) = self.local_symbols.get(symbol) {
            Some(offset as u64)
        } else if let Some(&address) = self.external_symbols.get(symbol) {
            Some(address)
        } else {
            None
        }
    }

    /// Check if a symbol is external (not defined in this module).
    pub fn is_external(&self, symbol: &Symbol) -> bool {
        !self.local_symbols.contains_key(symbol)
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
