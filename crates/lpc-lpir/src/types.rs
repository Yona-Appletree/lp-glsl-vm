//! Type system for the IR.

use core::fmt;

/// A type in the IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    /// 32-bit signed integer
    I32,
    /// 32-bit unsigned integer
    U32,
    /// 32-bit floating point
    F32,
}

impl Type {
    /// Get the size of this type in bytes.
    pub fn size_bytes(&self) -> usize {
        match self {
            Type::I32 => 4,
            Type::U32 => 4,
            Type::F32 => 4,
        }
    }

    /// Check if this is an integer type.
    pub fn is_integer(&self) -> bool {
        matches!(self, Type::I32 | Type::U32)
    }

    /// Check if this is a floating point type.
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F32)
    }

    /// Check if this is an unsigned integer type.
    pub fn is_unsigned(&self) -> bool {
        matches!(self, Type::U32)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::I32 => write!(f, "i32"),
            Type::U32 => write!(f, "u32"),
            Type::F32 => write!(f, "f32"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_sizes() {
        assert_eq!(Type::I32.size_bytes(), 4);
        assert_eq!(Type::U32.size_bytes(), 4);
        assert_eq!(Type::F32.size_bytes(), 4);
    }

    #[test]
    fn test_type_kinds() {
        assert!(Type::I32.is_integer());
        assert!(Type::U32.is_integer());
        assert!(!Type::I32.is_float());
        assert!(!Type::U32.is_float());
        assert!(Type::F32.is_float());
        assert!(!Type::F32.is_integer());
        assert!(Type::U32.is_unsigned());
        assert!(!Type::I32.is_unsigned());
        assert!(!Type::F32.is_unsigned());
    }
}
