//! Functions.

use alloc::{format, string::String, vec::Vec};
use core::fmt;

use crate::{block::Block, signature::Signature};

/// A function in the IR.
///
/// A function consists of:
/// - A signature (parameters and return types)
/// - A list of basic blocks
/// - An entry block (the first block)
/// - An optional name (for debugging and module lookup)
#[derive(Debug, Clone)]
pub struct Function {
    /// Function signature.
    pub signature: Signature,
    /// Basic blocks in this function.
    pub blocks: Vec<Block>,
    /// Optional function name (for debugging and module lookup).
    pub name: Option<String>,
}

impl Function {
    /// Create a new function with the given signature.
    pub fn new(signature: Signature) -> Self {
        Self {
            signature,
            blocks: Vec::new(),
            name: None,
        }
    }

    /// Create a new function with a name.
    pub fn with_name(signature: Signature, name: String) -> Self {
        Self {
            signature,
            blocks: Vec::new(),
            name: Some(name),
        }
    }

    /// Set the function name.
    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }

    /// Get the function name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Add a block to this function.
    pub fn add_block(&mut self, block: Block) -> usize {
        let index = self.blocks.len();
        self.blocks.push(block);
        index
    }

    /// Get the entry block (first block), if any.
    pub fn entry_block(&self) -> Option<&Block> {
        self.blocks.first()
    }

    /// Get a mutable reference to the entry block.
    pub fn entry_block_mut(&mut self) -> Option<&mut Block> {
        self.blocks.first_mut()
    }

    /// Get a block by index.
    pub fn block(&self, index: usize) -> Option<&Block> {
        self.blocks.get(index)
    }

    /// Get a mutable reference to a block by index.
    pub fn block_mut(&mut self, index: usize) -> Option<&mut Block> {
        self.blocks.get_mut(index)
    }

    /// Get the number of blocks in this function.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Function name (if any)
        if let Some(name) = &self.name {
            write!(f, "function @{}", name)?;
        } else {
            write!(f, "function")?;
        }

        // Signature
        write!(f, "(")?;
        for (i, param_ty) in self.signature.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param_ty)?;
        }
        write!(f, ")")?;

        if !self.signature.returns.is_empty() {
            write!(f, " -> ")?;
            for (i, ret_ty) in self.signature.returns.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", ret_ty)?;
            }
        }

        writeln!(f, " {{")?;

        // Print each block
        for (i, block) in self.blocks.iter().enumerate() {
            writeln!(f, "block{}:", i)?;
            write!(f, "{}", block)?;
        }

        writeln!(f, "}}")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::types::Type;

    #[test]
    fn test_function_creation() {
        let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
        let func = Function::new(sig.clone());
        // Note: Can't compare functions directly due to f64 in Inst
        assert_eq!(func.block_count(), 0);
    }

    #[test]
    fn test_function_add_block() {
        let sig = Signature::empty();
        let mut func = Function::new(sig);
        let block = Block::new();
        let index = func.add_block(block.clone());
        assert_eq!(index, 0);
        assert_eq!(func.block_count(), 1);
        // Note: Can't compare blocks directly due to f64 in Inst
        assert_eq!(func.block_count(), 1);
    }

    #[test]
    fn test_entry_block() {
        let sig = Signature::empty();
        let mut func = Function::new(sig);
        let block = Block::new();
        func.add_block(block.clone());
        // Note: Can't compare blocks directly due to f64 in Inst
        assert!(func.entry_block().is_some());
    }
}
