//! Dense entity-to-data maps (like CLIF's PrimaryMap).
//!
//! PrimaryMap provides O(1) lookups from entity references to data.
//! It's essentially a Vec with entity-based indexing, providing type safety
//! and cache-friendly access patterns.

use alloc::vec::Vec;
use core::marker::PhantomData;

use crate::entity::EntityRef;

/// Dense map from entity to data
///
/// This is essentially a Vec with entity-based indexing.
/// Provides O(1) lookups and is cache-friendly.
///
/// # Type Safety
///
/// PrimaryMap is generic over the entity type, preventing mixing
/// different entity types. For example:
///
/// - `PrimaryMap<Block, BlockData>` - maps blocks to block data
/// - `PrimaryMap<Inst, InstData>` - maps instructions to instruction data
/// - `PrimaryMap<Value, Type>` - maps values to their types
#[derive(Debug, Clone)]
pub struct PrimaryMap<K: EntityRef, V> {
    data: Vec<V>,
    _phantom: PhantomData<K>,
}

impl<K: EntityRef, V> PrimaryMap<K, V> {
    /// Create a new empty PrimaryMap
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            _phantom: PhantomData,
        }
    }

    /// Create a new PrimaryMap with the specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            _phantom: PhantomData,
        }
    }

    /// Push a value and return its entity key
    ///
    /// The entity key will have an index equal to the current length
    /// of the map before the push.
    pub fn push(&mut self, value: V) -> K {
        let index = self.data.len();
        self.data.push(value);
        K::from_index(index)
    }

    /// Get a value by entity key
    pub fn get(&self, key: K) -> Option<&V> {
        self.data.get(key.index())
    }

    /// Get a mutable value by entity key
    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        self.data.get_mut(key.index())
    }

    /// Get length
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    /// Reserve capacity for at least `additional` more elements
    pub fn reserve(&mut self, additional: usize) {
        self.data.reserve(additional);
    }

    /// Iterate over entries as (entity, value) pairs
    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.data
            .iter()
            .enumerate()
            .map(|(i, v)| (K::from_index(i), v))
    }

    /// Iterate over values
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.data.iter()
    }

    /// Iterate over mutable values
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.data.iter_mut()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.data.clear();
    }
}

impl<K: EntityRef, V> Default for PrimaryMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::entity::{Block, Inst};

    #[test]
    fn test_primary_map_basic() {
        let mut map: PrimaryMap<Block, i32> = PrimaryMap::new();

        let b1 = map.push(10);
        let b2 = map.push(20);
        let b3 = map.push(30);

        assert_eq!(map.get(b1), Some(&10));
        assert_eq!(map.get(b2), Some(&20));
        assert_eq!(map.get(b3), Some(&30));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn test_primary_map_capacity() {
        let map: PrimaryMap<Block, i32> = PrimaryMap::with_capacity(100);
        assert!(map.capacity() >= 100);
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_primary_map_iteration() {
        let mut map: PrimaryMap<Block, i32> = PrimaryMap::new();
        map.push(10);
        map.push(20);
        map.push(30);

        let mut items: Vec<_> = map.iter().collect();
        items.sort_by_key(|(k, _)| k.index());

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].1, &10);
        assert_eq!(items[1].1, &20);
        assert_eq!(items[2].1, &30);
    }

    #[test]
    fn test_primary_map_type_safety() {
        // Verify that different entity types create different map types
        let mut block_map: PrimaryMap<Block, i32> = PrimaryMap::new();
        let mut inst_map: PrimaryMap<Inst, i32> = PrimaryMap::new();

        let b1 = block_map.push(100);
        let i1 = inst_map.push(200);

        // These are different types, so can't be mixed
        assert_eq!(block_map.get(b1), Some(&100));
        assert_eq!(inst_map.get(i1), Some(&200));

        // Type safety: Rust prevents using Block key in Inst map or vice versa
        // This is enforced at compile time - if you uncomment these, they won't compile:
        // assert_eq!(inst_map.get(b1), None);  // ERROR: expected `Inst`, found `Block`
        // assert_eq!(block_map.get(i1), None); // ERROR: expected `Block`, found `Inst`

        // Verify that indices can be the same but types are different
        assert_eq!(b1.index(), 0);
        assert_eq!(i1.index(), 0);
        // Even though indices are the same, the types are different
        // This is what provides type safety - you can't accidentally use
        // a Block key where an Inst key is expected, even if they have the same index
    }

    #[test]
    fn test_primary_map_growth() {
        let mut map: PrimaryMap<Block, i32> = PrimaryMap::new();

        // Push many items to test growth
        for i in 0..100 {
            map.push(i);
        }

        assert_eq!(map.len(), 100);
        for i in 0..100 {
            let key = Block::from_index(i);
            assert_eq!(map.get(key), Some(&(i as i32)));
        }
    }

    #[test]
    fn test_primary_map_empty() {
        let map: PrimaryMap<Block, i32> = PrimaryMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_primary_map_get_mut() {
        let mut map: PrimaryMap<Block, i32> = PrimaryMap::new();
        let b1 = map.push(10);

        if let Some(value) = map.get_mut(b1) {
            *value = 42;
        }

        assert_eq!(map.get(b1), Some(&42));
    }

    #[test]
    fn test_primary_map_values() {
        let mut map: PrimaryMap<Block, i32> = PrimaryMap::new();
        map.push(10);
        map.push(20);
        map.push(30);

        let values: Vec<_> = map.values().copied().collect();
        assert_eq!(values, vec![10, 20, 30]);
    }

    #[test]
    fn test_primary_map_values_mut() {
        let mut map: PrimaryMap<Block, i32> = PrimaryMap::new();
        map.push(10);
        map.push(20);

        for value in map.values_mut() {
            *value *= 2;
        }

        assert_eq!(map.get(Block::from_index(0)), Some(&20));
        assert_eq!(map.get(Block::from_index(1)), Some(&40));
    }

    #[test]
    fn test_primary_map_clear() {
        let mut map: PrimaryMap<Block, i32> = PrimaryMap::new();
        map.push(10);
        map.push(20);

        assert_eq!(map.len(), 2);
        map.clear();
        assert_eq!(map.len(), 0);
        assert!(map.is_empty());
    }

    #[test]
    fn test_primary_map_reserve() {
        let mut map: PrimaryMap<Block, i32> = PrimaryMap::new();
        let initial_capacity = map.capacity();

        map.reserve(100);
        assert!(map.capacity() >= initial_capacity + 100);
    }

    #[test]
    fn test_primary_map_default() {
        let map: PrimaryMap<Block, i32> = PrimaryMap::default();
        assert!(map.is_empty());
    }
}
