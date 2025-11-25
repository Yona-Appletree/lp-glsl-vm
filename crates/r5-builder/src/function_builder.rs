//! Function builder.

use alloc::vec::Vec;

use r5_ir::{Block, Function, Signature, Type, Value};

use crate::{block_builder::BlockBuilder, ssa::SSABuilder};

/// Builder for constructing functions in IR.
///
/// This builder helps construct functions with proper SSA form.
#[derive(Debug)]
pub struct FunctionBuilder {
    /// The function being built.
    function: Function,
    /// SSA builder for tracking variable definitions.
    ssa: SSABuilder,
    /// Current block being built (if any).
    current_block: Option<usize>,
}

impl FunctionBuilder {
    /// Create a new function builder with the given signature.
    pub fn new(signature: Signature) -> Self {
        Self {
            function: Function::new(signature),
            ssa: SSABuilder::new(),
            current_block: None,
        }
    }

    /// Create a new block and return its index.
    pub fn create_block(&mut self) -> usize {
        let block = Block::new();
        let index = self.function.add_block(block);
        index
    }

    /// Create a new block with parameters (for phi nodes).
    pub fn create_block_with_params(&mut self, params: Vec<Value>) -> usize {
        let block = Block::with_params(params);
        let index = self.function.add_block(block);
        index
    }

    /// Switch to building the given block.
    ///
    /// Returns a `BlockBuilder` for adding instructions to this block.
    pub fn block_builder(&mut self, block_index: usize) -> BlockBuilder<'_> {
        self.current_block = Some(block_index);
        BlockBuilder::new(self, block_index)
    }

    /// Get the SSA builder (for advanced use cases).
    pub fn ssa(&mut self) -> &mut SSABuilder {
        &mut self.ssa
    }

    /// Get a new value from the SSA builder.
    pub fn new_value(&mut self) -> Value {
        self.ssa.new_value()
    }

    /// Finish building and return the function.
    pub fn finish(self) -> Function {
        self.function
    }

    /// Get a mutable reference to the function (for internal use).
    pub(crate) fn function_mut(&mut self) -> &mut Function {
        &mut self.function
    }

    /// Get the current block index.
    pub fn current_block(&self) -> Option<usize> {
        self.current_block
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use r5_ir::Inst;

    use super::*;

    #[test]
    fn test_function_builder_creation() {
        let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
        let builder = FunctionBuilder::new(sig);
        let func = builder.finish();
        assert_eq!(func.block_count(), 0);
    }

    #[test]
    fn test_create_block() {
        let sig = Signature::empty();
        let mut builder = FunctionBuilder::new(sig);
        let block_idx = builder.create_block();
        assert_eq!(block_idx, 0);
        let func = builder.finish();
        assert_eq!(func.block_count(), 1);
    }

    #[test]
    fn test_block_builder() {
        let sig = Signature::empty();
        let mut builder = FunctionBuilder::new(sig);
        let block_idx = builder.create_block();

        let v1 = builder.new_value();
        let v2 = builder.new_value();
        let v3 = builder.new_value();

        {
            let mut block_builder = builder.block_builder(block_idx);
            block_builder.iconst(v3, 42);
            block_builder.return_(&vec![v3]);
        }

        let func = builder.finish();
        let block = func.block(0).unwrap();
        assert_eq!(block.inst_count(), 2);
    }

    #[test]
    fn test_build_add_function() {
        // Build: fn add(a: i32, b: i32) -> i32 { a + b }
        let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
        let mut builder = FunctionBuilder::new(sig);
        let block_idx = builder.create_block();

        // Get parameter values (in real usage, these would come from block params)
        let a = builder.new_value();
        let b = builder.new_value();
        let result = builder.new_value();

        {
            let mut block_builder = builder.block_builder(block_idx);
            block_builder.iadd(result, a, b);
            block_builder.return_(&vec![result]);
        }

        let func = builder.finish();
        let block = func.block(0).unwrap();
        assert_eq!(block.inst_count(), 2);
    }
}
