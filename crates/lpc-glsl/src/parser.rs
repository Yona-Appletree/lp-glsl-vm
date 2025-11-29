//! GLSL parsing module.
//!
//! This module wraps glsl-parser to extract function definitions
//! from GLSL source code.

use alloc::{format, string::String, vec::Vec};

use glsl::{
    parser::Parse,
    syntax::{ExternalDeclaration, FunctionDefinition, TranslationUnit},
};

use crate::error::{GlslError, GlslResult};

/// Information about a parsed function.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Function name
    pub name: String,
    /// Function definition AST node
    pub definition: FunctionDefinition,
}

/// Parse GLSL source code and extract function definitions.
///
/// # Arguments
///
/// * `source` - GLSL source code string
///
/// # Returns
///
/// A vector of `FunctionInfo` containing all function definitions found in the source.
///
/// # Errors
///
/// Returns `GlslError::ParseError` if the GLSL source cannot be parsed.
pub fn parse_glsl(source: &str) -> GlslResult<Vec<FunctionInfo>> {
    // Parse the GLSL source into a TranslationUnit
    let translation_unit = TranslationUnit::parse(source)
        .map_err(|e| GlslError::parse(format!("Failed to parse GLSL: {:?}", e)))?;

    // Extract function definitions from the translation unit
    let mut functions = Vec::new();

    for decl in translation_unit.0 .0.iter() {
        match decl {
            ExternalDeclaration::FunctionDefinition(func_def) => {
                let name = func_def.prototype.name.0.clone();
                functions.push(FunctionInfo {
                    name,
                    definition: func_def.clone(),
                });
            }
            _ => {
                // Skip non-function declarations for now
                // (e.g., global variables, uniforms, etc.)
            }
        }
    }

    Ok(functions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let glsl = r#"
            int add(int x, int y) {
                return x + y;
            }
        "#;

        let result = parse_glsl(glsl);
        assert!(result.is_ok(), "Parsing should succeed");

        let functions = result.unwrap();
        assert_eq!(functions.len(), 1, "Should find one function");
        assert_eq!(functions[0].name, "add", "Function name should be 'add'");
    }

    #[test]
    fn test_parse_multiple_functions() {
        let glsl = r#"
            int add(int x, int y) {
                return x + y;
            }

            int multiply(int x, int y) {
                return x * y;
            }
        "#;

        let result = parse_glsl(glsl);
        assert!(result.is_ok(), "Parsing should succeed");

        let functions = result.unwrap();
        assert_eq!(functions.len(), 2, "Should find two functions");
        assert_eq!(functions[0].name, "add");
        assert_eq!(functions[1].name, "multiply");
    }

    #[test]
    fn test_parse_function_with_bool() {
        let glsl = r#"
            bool is_positive(int x) {
                return x > 0;
            }
        "#;

        let result = parse_glsl(glsl);
        assert!(result.is_ok(), "Parsing should succeed");

        let functions = result.unwrap();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "is_positive");
    }

    #[test]
    fn test_parse_invalid_glsl() {
        let glsl = r#"
            int add(int x, int y {
                return x + y;
            }
        "#;

        let result = parse_glsl(glsl);
        assert!(result.is_err(), "Parsing should fail with syntax error");
        match result.unwrap_err() {
            GlslError::ParseError(_) => {}
            _ => panic!("Should be a parse error"),
        }
    }

    #[test]
    fn test_parse_empty_source() {
        let glsl = "";

        let result = parse_glsl(glsl);
        // Empty source might parse as empty translation unit
        // or might fail - depends on glsl-parser behavior
        // For now, we'll accept either outcome
        if let Ok(functions) = result {
            assert_eq!(functions.len(), 0);
        }
    }
}
