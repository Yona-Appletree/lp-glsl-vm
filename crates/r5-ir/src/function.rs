//! Functions.

use alloc::vec::Vec;

use crate::{block::Block, signature::Signature};

/// A function in the IR.
///
/// A function consists of:
/// - A signature (parameters and return types)
/// - A list of basic blocks
/// - An entry block (the first block)
#[derive(Debug, Clone)]
pub struct Function {
    /// Function signature.
    pub signature: Signature,
    /// Basic blocks in this function.
    pub blocks: Vec<Block>,
}

impl Function {
    /// Create a new function with the given signature.
    pub fn new(signature: Signature) -> Self {
        Self {
            signature,
            blocks: Vec::new(),
        }
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
