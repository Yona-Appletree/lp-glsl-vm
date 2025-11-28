//! Block data (parameters, metadata).

use alloc::vec::Vec;

use crate::value::Value;

/// Block data (what a block is, separate from layout)
///
/// This stores the data associated with a block: its parameters.
/// The layout (where the block appears) is stored in Layout.
/// The instructions (what instructions are in the block) are stored in DFG.
#[derive(Debug, Clone)]
pub struct BlockData {
    /// Block parameters (for phi nodes)
    pub params: Vec<Value>,
}

impl BlockData {
    /// Create a new empty block data
    pub fn new() -> Self {
        Self {
            params: Vec::new(),
        }
    }

    /// Create a new block data with the given parameters
    pub fn with_params(params: Vec<Value>) -> Self {
        Self { params }
    }

    /// Get the number of parameters for this block
    pub fn param_count(&self) -> usize {
        self.params.len()
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
