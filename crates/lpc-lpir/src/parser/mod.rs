//! Parser for IR text format (Cranelift CLIF-style).

mod block;
mod error;
mod function;
mod instructions;
mod module;
mod primitives;
mod whitespace;

use error::parse_error;
pub use error::ParseError;
use function::parse_function_internal;
use module::parse_module_internal;

use crate::{function::Function, module::Module};

/// Parse a complete module from IR text.
pub fn parse_module(input: &str) -> Result<Module, ParseError> {
    // Trim leading/trailing whitespace
    let trimmed = input.trim();
    match parse_module_internal(trimmed) {
        Ok(("", module)) => Ok(module),
        Ok((remaining, module)) => {
            // Allow trailing whitespace
            if remaining.trim().is_empty() {
                Ok(module)
            } else {
                Err(parse_error(
                    trimmed,
                    remaining,
                    &alloc::format!("Unexpected input remaining: {}", remaining),
                ))
            }
        }
        Err(e) => Err(parse_error(
            trimmed,
            trimmed,
            &alloc::format!("Parse error: {:?}", e),
        )),
    }
}

/// Parse a function from IR text.
pub fn parse_function(input: &str) -> Result<Function, ParseError> {
    // Trim leading/trailing whitespace
    let trimmed = input.trim();
    match parse_function_internal(trimmed) {
        Ok(("", func)) => Ok(func),
        Ok((remaining, func)) => {
            // Allow trailing whitespace
            if remaining.trim().is_empty() {
                Ok(func)
            } else {
                Err(parse_error(
                    trimmed,
                    remaining,
                    &alloc::format!("Unexpected input remaining: {}", remaining),
                ))
            }
        }
        Err(e) => Err(parse_error(
            trimmed,
            trimmed,
            &alloc::format!("Parse error: {:?}", e),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_module_empty() {
        // Test that empty input fails
        let result = parse_module("");
        assert!(result.is_err(), "Should fail on empty input");
    }

    #[test]
    fn test_parse_module_invalid_syntax() {
        // Test that invalid syntax fails
        let result = parse_module("invalid");
        assert!(result.is_err(), "Should fail on invalid syntax");
    }

    #[test]
    fn test_parse_module_missing_brace() {
        // Test that missing closing brace fails
        let result = parse_module("module {");
        assert!(result.is_err(), "Should fail on missing closing brace");
    }

    #[test]
    fn test_parse_function_empty() {
        // Test that empty input fails
        let result = parse_function("");
        assert!(result.is_err(), "Should fail on empty input");
    }

    #[test]
    fn test_parse_function_invalid_syntax() {
        // Test that invalid syntax fails
        let result = parse_function("invalid");
        assert!(result.is_err(), "Should fail on invalid syntax");
    }

    #[test]
    fn test_parse_function_missing_brace() {
        // Test that missing closing brace fails
        let result = parse_function("function %test() {");
        assert!(result.is_err(), "Should fail on missing closing brace");
    }

    #[test]
    fn test_parse_function_unexpected_remaining() {
        // Test that unexpected remaining input fails
        let result = parse_function("function %test() {\nblock0:\n    return\n} extra");
        assert!(result.is_err(), "Should fail on unexpected remaining input");
    }

    #[test]
    fn test_parse_function_with_comments() {
        // Test function with comments
        let input = r#"function %test() -> i32 {
            ; This is a comment
            block0:
                ; Comment before instruction
                v0 = iconst 42 ; inline comment
                return v0 ; return comment
        }"#;
        let result = parse_function(input.trim());
        assert!(
            result.is_ok(),
            "parse_function with comments failed: {:?}",
            result
        );
        let func = result.unwrap();
        assert_eq!(func.blocks.len(), 1);
        assert_eq!(func.blocks[0].insts.len(), 2);
    }

    #[test]
    fn test_parse_module_with_comments() {
        // Test module with comments
        let input = r#"module {
            ; Module-level comment
            entry: %main ; entry point comment

            function %main() {
                ; Function comment
                block0:
                    v0 = iconst 42 ; constant comment
                    return v0
            }
        }"#;
        let result = parse_module(input.trim());
        assert!(
            result.is_ok(),
            "parse_module with comments failed: {:?}",
            result
        );
        let module = result.unwrap();
        assert_eq!(module.function_count(), 1);
        assert!(module.entry_function().is_some());
    }
}
