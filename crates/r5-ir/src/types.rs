//! Type system for the IR.

/// A type in the IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    /// 32-bit signed integer
    I32,
    /// 64-bit signed integer
    I64,
    /// 32-bit floating point
    F32,
    /// 64-bit floating point
    F64,
}

impl Type {
    /// Get the size of this type in bytes.
    pub fn size_bytes(&self) -> usize {
        match self {
            Type::I32 => 4,
            Type::I64 => 8,
            Type::F32 => 4,
            Type::F64 => 8,
        }
    }

    /// Check if this is an integer type.
    pub fn is_integer(&self) -> bool {
        matches!(self, Type::I32 | Type::I64)
    }

    /// Check if this is a floating point type.
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32 | Type::F64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_sizes() {
        assert_eq!(Type::I32.size_bytes(), 4);
        assert_eq!(Type::I64.size_bytes(), 8);
        assert_eq!(Type::F32.size_bytes(), 4);
        assert_eq!(Type::F64.size_bytes(), 8);
    }

    #[test]
    fn test_type_kinds() {
        assert!(Type::I32.is_integer());
        assert!(Type::I64.is_integer());
        assert!(!Type::I32.is_float());
        assert!(!Type::I64.is_float());
        assert!(Type::F32.is_float());
        assert!(Type::F64.is_float());
        assert!(!Type::F32.is_integer());
        assert!(!Type::F64.is_integer());
    }
}
