//! Type extraction utilities shared between type checking and code generation.

use crate::types::GlslType;

/// Extract type from fully specified type (with qualifiers).
pub fn extract_type_from_fully_specified(
    ty: &glsl::syntax::FullySpecifiedType,
) -> Option<GlslType> {
    extract_type_from_specifier(&ty.ty)
}

/// Extract type from type specifier.
pub fn extract_type_from_specifier(ty: &glsl::syntax::TypeSpecifier) -> Option<GlslType> {
    GlslType::from_glsl_type_specifier(&ty.ty)
}
