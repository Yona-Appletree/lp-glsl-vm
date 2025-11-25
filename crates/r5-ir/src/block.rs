//! Basic blocks.

use alloc::vec::Vec;

use crate::{inst::Inst, value::Value};

/// A basic block in a function.
///
/// A basic block is a sequence of instructions with a single entry point
/// and a single exit point. It may have parameters (for phi nodes) and
/// contains a list of instructions.
#[derive(Debug, Clone)]
pub struct Block {
    /// Block parameters (for phi nodes / SSA form at control flow merges).
    pub params: Vec<Value>,
    /// Instructions in this block.
    pub insts: Vec<Inst>,
}

impl Block {
    /// Create a new empty block.
    pub fn new() -> Self {
        Self {
            params: Vec::new(),
            insts: Vec::new(),
        }
    }

    /// Create a new block with the given parameters.
    pub fn with_params(params: Vec<Value>) -> Self {
        Self {
            params,
            insts: Vec::new(),
        }
    }

    /// Add an instruction to this block.
    pub fn push_inst(&mut self, inst: Inst) {
        self.insts.push(inst);
    }

    /// Get the number of instructions in this block.
    pub fn inst_count(&self) -> usize {
        self.insts.len()
    }

    /// Get the number of parameters for this block.
    pub fn param_count(&self) -> usize {
        self.params.len()
    }
}

impl Default for Block {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_block_creation() {
        let block = Block::new();
        assert_eq!(block.inst_count(), 0);
        assert_eq!(block.param_count(), 0);
    }

    #[test]
    fn test_block_with_params() {
        let params = vec![Value::new(0), Value::new(1)];
        let block = Block::with_params(params.clone());
        assert_eq!(block.param_count(), 2);
        assert_eq!(block.param_count(), params.len());
    }

    #[test]
    fn test_block_add_inst() {
        let mut block = Block::new();
        let inst = Inst::Iconst {
            result: Value::new(0),
            value: 42,
        };
        block.push_inst(inst.clone());
        assert_eq!(block.inst_count(), 1);
        assert_eq!(block.inst_count(), 1);
    }
}
