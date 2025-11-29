//! GLSL type system.
//!
//! This module defines GLSL types and their mapping to LPIR types.

use lpc_lpir::Type as LpirType;

/// GLSL type representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlslType {
    /// 32-bit signed integer
    Int,
    /// Boolean type (maps to U32 in LPIR: 0 = false, 1 = true)
    Bool,
}

impl GlslType {
    /// Convert GLSL type to LPIR type.
    pub fn to_lpir(self) -> LpirType {
        match self {
            GlslType::Int => LpirType::I32,
            GlslType::Bool => LpirType::U32,
        }
    }

    /// Try to convert from GLSL AST type specifier.
    ///
    /// Returns `None` if the type is not supported in the initial implementation.
    pub fn from_glsl_type_specifier(
        spec: &glsl::syntax::TypeSpecifierNonArray,
    ) -> Option<Self> {
        match spec {
            glsl::syntax::TypeSpecifierNonArray::Int => Some(GlslType::Int),
            glsl::syntax::TypeSpecifierNonArray::Bool => Some(GlslType::Bool),
            _ => None, // Not supported in initial implementation
        }
    }

    /// Get the name of this type as a string.
    pub fn name(self) -> &'static str {
        match self {
            GlslType::Int => "int",
            GlslType::Bool => "bool",
        }
    }
}

impl core::fmt::Display for GlslType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;

    use super::*;

    #[test]
    fn test_type_to_lpir() {
        assert_eq!(GlslType::Int.to_lpir(), LpirType::I32);
        assert_eq!(GlslType::Bool.to_lpir(), LpirType::U32);
    }

    #[test]
    fn test_from_glsl_type_specifier() {
        assert_eq!(
            GlslType::from_glsl_type_specifier(&glsl::syntax::TypeSpecifierNonArray::Int),
            Some(GlslType::Int)
        );
        assert_eq!(
            GlslType::from_glsl_type_specifier(&glsl::syntax::TypeSpecifierNonArray::Bool),
            Some(GlslType::Bool)
        );
        // Unsupported types should return None
        assert_eq!(
            GlslType::from_glsl_type_specifier(&glsl::syntax::TypeSpecifierNonArray::Float),
            None
        );
    }

    #[test]
    fn test_type_name() {
        assert_eq!(GlslType::Int.name(), "int");
        assert_eq!(GlslType::Bool.name(), "bool");
    }

    #[test]
    fn test_type_display() {
        assert_eq!(format!("{}", GlslType::Int), "int");
        assert_eq!(format!("{}", GlslType::Bool), "bool");
    }
}

