//! Packed Option for entity references.
//!
//! This is a space-efficient Option wrapper for entity references.
//! For now, it's a simple wrapper, but can be optimized later to pack
//! the entity index more efficiently.

use crate::entity::EntityRef;

/// Space-efficient Option for entity references
///
/// Uses an invalid index (u32::MAX) to represent None.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PackedOption<T: EntityRef> {
    index: u32,
    _phantom: core::marker::PhantomData<T>,
}

impl<T: EntityRef> PackedOption<T> {
    /// Create a None value
    pub fn none() -> Self {
        Self {
            index: u32::MAX,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Create a Some value
    pub fn some(entity: T) -> Self {
        Self {
            index: entity.index() as u32,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Expand to Option
    pub fn expand(self) -> Option<T> {
        if self.index == u32::MAX {
            None
        } else {
            Some(T::from_index(self.index as usize))
        }
    }

    /// Check if this is Some
    pub fn is_some(&self) -> bool {
        self.index != u32::MAX
    }

    /// Check if this is None
    pub fn is_none(&self) -> bool {
        self.index == u32::MAX
    }
}

impl<T: EntityRef> Default for PackedOption<T> {
    fn default() -> Self {
        Self::none()
    }
}

impl<T: EntityRef> From<Option<T>> for PackedOption<T> {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(entity) => Self::some(entity),
            None => Self::none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Block, Inst};

    #[test]
    fn test_packed_option_some() {
        let block = Block::new(5);
        let packed = PackedOption::some(block);

        assert!(packed.is_some());
        assert!(!packed.is_none());
        assert_eq!(packed.expand(), Some(block));
    }

    #[test]
    fn test_packed_option_none() {
        let packed: PackedOption<Block> = PackedOption::none();

        assert!(!packed.is_some());
        assert!(packed.is_none());
        assert_eq!(packed.expand(), None);
    }

    #[test]
    fn test_packed_option_roundtrip() {
        let block = Block::new(42);
        let packed = PackedOption::some(block);
        let expanded = packed.expand();

        assert_eq!(expanded, Some(block));

        let packed2: PackedOption<Block> = expanded.into();
        assert_eq!(packed, packed2);
    }

    #[test]
    fn test_packed_option_default() {
        let packed: PackedOption<Block> = PackedOption::default();
        assert!(packed.is_none());
    }

    #[test]
    fn test_packed_option_from_option() {
        let block = Block::new(10);
        let opt = Some(block);
        let packed: PackedOption<Block> = opt.into();

        assert_eq!(packed.expand(), Some(block));

        let opt_none: Option<Block> = None;
        let packed_none: PackedOption<Block> = opt_none.into();
        assert!(packed_none.is_none());
    }

    #[test]
    fn test_packed_option_different_types() {
        let block = Block::new(5);
        let inst = Inst::new(5);

        let packed_block = PackedOption::some(block);
        let packed_inst = PackedOption::some(inst);

        // Different types, can't compare directly
        assert_eq!(packed_block.expand(), Some(block));
        assert_eq!(packed_inst.expand(), Some(inst));
    }
}

