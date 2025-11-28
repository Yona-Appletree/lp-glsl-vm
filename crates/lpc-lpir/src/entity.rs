//! Entity reference system for type-safe entity IDs.
//!
//! This module provides the foundation for the entity system, similar to
//! Cranelift's EntityRef pattern. Entities (Block, Inst, Value) are type-safe
//! references that prevent mixing different entity types.

use core::fmt;

/// Base trait for entity references (like CLIF's EntityRef)
///
/// Entities are type-safe identifiers for compiler IR elements. They provide
/// O(1) conversion to/from indices while maintaining type safety.
pub trait EntityRef: Copy + Clone + PartialEq + Eq + core::hash::Hash + fmt::Debug {
    /// Get the index of this entity
    fn index(self) -> usize;

    /// Create an entity from an index
    fn from_index(index: usize) -> Self;

    /// Get the next available index (for entity creation)
    fn next_index(self) -> Self {
        Self::from_index(self.index() + 1)
    }
}

/// Block entity reference
///
/// A type-safe identifier for a basic block in a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Block(u32);

impl Block {
    /// Create a new block entity with the given index
    pub fn new(index: u32) -> Self {
        Block(index)
    }

    /// Get the index of this block
    pub fn index(self) -> u32 {
        self.0
    }
}

impl EntityRef for Block {
    fn index(self) -> usize {
        self.0 as usize
    }

    fn from_index(index: usize) -> Self {
        Block(index as u32)
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "block{}", self.0)
    }
}

/// Instruction entity reference
///
/// A type-safe identifier for an instruction in a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Inst(u32);

impl Inst {
    /// Create a new instruction entity with the given index
    pub fn new(index: u32) -> Self {
        Inst(index)
    }

    /// Get the index of this instruction
    pub fn index(self) -> u32 {
        self.0
    }
}

impl EntityRef for Inst {
    fn index(self) -> usize {
        self.0 as usize
    }

    fn from_index(index: usize) -> Self {
        Inst(index as u32)
    }
}

impl fmt::Display for Inst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "inst{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;

    use super::*;

    #[test]
    fn test_entity_ref_trait() {
        let block = Block::from_index(5);
        assert_eq!(block.index(), 5);

        let next = block.next_index();
        assert_eq!(next.index(), 6);

        let from_index = Block::from_index(10);
        assert_eq!(from_index.index(), 10);
    }

    #[test]
    fn test_entity_ordering() {
        let b1 = Block::new(1);
        let b2 = Block::new(2);
        let b3 = Block::new(1);

        assert!(b1 < b2);
        assert!(b1 == b3);
        assert!(b2 > b1);
    }

    #[test]
    fn test_entity_hashing() {
        use alloc::collections::BTreeSet;

        let mut set = BTreeSet::new();
        set.insert(Block::new(1));
        set.insert(Block::new(2));
        set.insert(Block::new(1)); // Duplicate

        assert_eq!(set.len(), 2);
        assert!(set.contains(&Block::new(1)));
        assert!(set.contains(&Block::new(2)));
    }

    #[test]
    fn test_block_creation() {
        let b1 = Block::new(0);
        let b2 = Block::new(1);

        assert_eq!(b1.index(), 0);
        assert_eq!(b2.index(), 1);
        assert_ne!(b1, b2);
    }

    #[test]
    fn test_block_equality() {
        let b1 = Block::new(5);
        let b2 = Block::new(5);
        let b3 = Block::new(6);

        assert_eq!(b1, b2);
        assert_ne!(b1, b3);
    }

    #[test]
    fn test_block_ordering() {
        let b1 = Block::new(1);
        let b2 = Block::new(2);
        let b3 = Block::new(3);

        assert!(b1 < b2);
        assert!(b2 < b3);
        assert!(b1 < b3);
    }

    #[test]
    fn test_block_display() {
        let b = Block::new(42);
        assert_eq!(format!("{}", b), "block42");
    }

    #[test]
    fn test_inst_creation() {
        let i1 = Inst::new(0);
        let i2 = Inst::new(1);

        assert_eq!(i1.index(), 0);
        assert_eq!(i2.index(), 1);
        assert_ne!(i1, i2);
    }

    #[test]
    fn test_inst_equality() {
        let i1 = Inst::new(5);
        let i2 = Inst::new(5);
        let i3 = Inst::new(6);

        assert_eq!(i1, i2);
        assert_ne!(i1, i3);
    }

    #[test]
    fn test_inst_ordering() {
        let i1 = Inst::new(1);
        let i2 = Inst::new(2);
        let i3 = Inst::new(3);

        assert!(i1 < i2);
        assert!(i2 < i3);
        assert!(i1 < i3);
    }

    #[test]
    fn test_inst_display() {
        let i = Inst::new(42);
        assert_eq!(format!("{}", i), "inst42");
    }

    #[test]
    fn test_type_safety() {
        // Verify that Block and Inst are different types
        let block = Block::new(5);
        let inst = Inst::new(5);

        // They should not be equal even with same index
        // (This is enforced by Rust's type system, but we can test the indices)
        assert_eq!(block.index(), inst.index());
        // But they are different types, so can't compare directly
    }
}
