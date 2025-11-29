//! Linked list node for instructions.

use crate::{
    entity::{Block, Inst},
    layout::{packed_option::PackedOption, sequence::SequenceNumber},
};

/// A node in the instruction linked list
///
/// This stores the layout information for an instruction: which block
/// contains it, its position within that block, and its sequence number
/// for program order comparison.
#[derive(Clone, Debug, Default)]
pub struct InstNode {
    /// Block containing this instruction
    pub block: PackedOption<Block>,
    /// Previous instruction in the block
    pub prev: PackedOption<Inst>,
    /// Next instruction in the block
    pub next: PackedOption<Inst>,
    /// Sequence number for program order comparison
    ///
    /// Sequence numbers are assigned like BASIC line numbers (10, 20, 30...)
    /// to allow O(1) comparison while leaving room for insertions.
    pub seq: SequenceNumber,
}

impl InstNode {
    /// Create a new instruction node
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this instruction is inserted in a block
    pub fn is_inserted(&self) -> bool {
        self.block.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inst_node_creation() {
        let node = InstNode::new();
        assert!(node.block.is_none());
        assert!(node.prev.is_none());
        assert!(node.next.is_none());
        assert_eq!(node.seq, 0);
        assert!(!node.is_inserted());
    }

    #[test]
    fn test_inst_node_links() {
        let mut node = InstNode::new();
        let prev_inst = Inst::new(0);
        let next_inst = Inst::new(2);

        node.prev = PackedOption::some(prev_inst);
        node.next = PackedOption::some(next_inst);

        assert_eq!(node.prev.expand(), Some(prev_inst));
        assert_eq!(node.next.expand(), Some(next_inst));
    }

    #[test]
    fn test_inst_node_block() {
        let mut node = InstNode::new();
        let block = Block::new(5);

        node.block = PackedOption::some(block);

        assert!(node.is_inserted());
        assert_eq!(node.block.expand(), Some(block));
    }

    #[test]
    fn test_inst_node_sequence() {
        let mut node = InstNode::new();
        node.seq = 42;

        assert_eq!(node.seq, 42);
    }
}

