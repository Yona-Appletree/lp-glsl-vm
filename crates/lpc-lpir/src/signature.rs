//! Function signatures.

use alloc::vec::Vec;

use crate::types::Type;

/// A function signature (parameter and return types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Parameter types.
    pub params: Vec<Type>,
    /// Return types.
    pub returns: Vec<Type>,
}

impl Signature {
    /// Create a new signature with the given parameters and returns.
    pub fn new(params: Vec<Type>, returns: Vec<Type>) -> Self {
        Self { params, returns }
    }

    /// Create a signature with no parameters and no returns.
    pub fn empty() -> Self {
        Self {
            params: Vec::new(),
            returns: Vec::new(),
        }
    }

    /// Get the number of parameters.
    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    /// Get the number of return values.
    pub fn return_count(&self) -> usize {
        self.returns.len()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_signature_creation() {
        let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
        assert_eq!(sig.param_count(), 2);
        assert_eq!(sig.return_count(), 1);
    }

    #[test]
    fn test_empty_signature() {
        let sig = Signature::empty();
        assert_eq!(sig.param_count(), 0);
        assert_eq!(sig.return_count(), 0);
    }
}
