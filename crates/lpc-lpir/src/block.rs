//! Block data (parameters, metadata).

use alloc::vec::Vec;

use crate::{types::Type, value::Value};

/// Block data (what a block is, separate from layout)
///
/// This stores the data associated with a block: its parameters.
/// The layout (where the block appears) is stored in Layout.
/// The instructions (what instructions are in the block) are stored in DFG.
#[derive(Debug, Clone)]
pub struct BlockData {
    /// Block parameters (for phi nodes)
    pub params: Vec<Value>,
    /// Parameter types (parallel to params)
    pub param_types: Vec<Type>,
}

impl BlockData {
    /// Create a new empty block data
    pub fn new() -> Self {
        Self {
            params: Vec::new(),
            param_types: Vec::new(),
        }
    }

    /// Create a new block data with the given parameters
    /// Types default to I32 if not provided
    pub fn with_params(params: Vec<Value>) -> Self {
        let param_types: Vec<Type> = (0..params.len()).map(|_| Type::I32).collect();
        Self {
            params,
            param_types,
        }
    }

    /// Create a new block data with the given parameters and types
    pub fn with_params_and_types(params: Vec<Value>, param_types: Vec<Type>) -> Self {
        assert_eq!(
            params.len(),
            param_types.len(),
            "params and param_types must have the same length"
        );
        Self {
            params,
            param_types,
        }
    }

    /// Get the number of parameters for this block
    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    /// Get the type of a parameter by index
    pub fn param_type(&self, index: usize) -> Option<Type> {
        self.param_types.get(index).copied()
    }
}

impl Default for BlockData {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_block_data_new() {
        let block_data = BlockData::new();
        assert_eq!(block_data.param_count(), 0);
    }

    #[test]
    fn test_block_data_with_params() {
        let params = vec![Value::new(0), Value::new(1)];
        let block_data = BlockData::with_params(params.clone());
        assert_eq!(block_data.param_count(), 2);
        assert_eq!(block_data.params.len(), params.len());
    }
}
