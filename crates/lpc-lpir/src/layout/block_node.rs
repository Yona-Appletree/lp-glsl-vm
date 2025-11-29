//! Linked list node for blocks.

use crate::{
    entity::{Block, Inst},
    layout::packed_option::PackedOption,
};

/// A node in the block linked list
///
/// This stores the layout information for a block: its position in the
/// block ordering and which instructions it contains.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockNode {
    /// Previous block in layout order
    pub prev: PackedOption<Block>,
    /// Next block in layout order
    pub next: PackedOption<Block>,
    /// First instruction in this block
    pub first_inst: PackedOption<Inst>,
    /// Last instruction in this block
    pub last_inst: PackedOption<Inst>,
    /// Is this block marked as "cold"?
    ///
    /// Cold blocks are less frequently executed and can be placed
    /// out of the hot path during code generation.
    pub cold: bool,
}

impl BlockNode {
    /// Create a new empty block node
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this block has any instructions
    pub fn has_insts(&self) -> bool {
        self.first_inst.is_some()
    }

    /// Check if this block is empty (no instructions)
    pub fn is_empty(&self) -> bool {
        !self.has_insts()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_node_creation() {
        let node = BlockNode::new();
        assert!(node.prev.is_none());
        assert!(node.next.is_none());
        assert!(node.first_inst.is_none());
        assert!(node.last_inst.is_none());
        assert!(!node.cold);
        assert!(node.is_empty());
    }

    #[test]
    fn test_block_node_links() {
        let mut node = BlockNode::new();
        let prev_block = Block::new(0);
        let next_block = Block::new(2);

        node.prev = PackedOption::some(prev_block);
        node.next = PackedOption::some(next_block);

        assert_eq!(node.prev.expand(), Some(prev_block));
        assert_eq!(node.next.expand(), Some(next_block));
    }

    #[test]
    fn test_block_node_instructions() {
        let mut node = BlockNode::new();
        let first_inst = Inst::new(0);
        let last_inst = Inst::new(5);

        node.first_inst = PackedOption::some(first_inst);
        node.last_inst = PackedOption::some(last_inst);

        assert!(node.has_insts());
        assert!(!node.is_empty());
        assert_eq!(node.first_inst.expand(), Some(first_inst));
        assert_eq!(node.last_inst.expand(), Some(last_inst));
    }

    #[test]
    fn test_block_node_cold() {
        let mut node = BlockNode::new();
        assert!(!node.cold);

        node.cold = true;
        assert!(node.cold);
    }
}

