//! SSA value identifiers.

use core::fmt;

use crate::entity::EntityRef;

/// An SSA value identifier.
///
/// In SSA form, each value is assigned exactly once. This is a simple
/// identifier that uniquely identifies a value in a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Value(u32);

impl Value {
    /// Create a new value with the given index.
    pub fn new(index: u32) -> Self {
        Self(index)
    }

    /// Get the index of this value.
    pub fn index(&self) -> u32 {
        self.0
    }
}

impl EntityRef for Value {
    fn index(self) -> usize {
        self.0 as usize
    }

    fn from_index(index: usize) -> Self {
        Value(index as u32)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;

    use super::*;
    use crate::entity::EntityRef;

    #[test]
    fn test_value_creation() {
        let v1 = Value::new(0);
        let v2 = Value::new(1);
        assert_eq!(v1.index(), 0);
        assert_eq!(v2.index(), 1);
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_value_display() {
        let v = Value::new(42);
        assert_eq!(format!("{}", v), "v42");
    }

    #[test]
    fn test_value_entity_ref() {
        let v = Value::new(5);
        assert_eq!(v.index(), 5);

        let next = v.next_index();
        assert_eq!(next.index(), 6);

        let from_index = Value::from_index(10);
        assert_eq!(from_index.index(), 10);
    }

    #[test]
    fn test_value_entity_ref_roundtrip() {
        let original = Value::new(42);
        let index = original.index();
        let reconstructed = Value::from_index(index);
        assert_eq!(original, reconstructed);
    }
}
