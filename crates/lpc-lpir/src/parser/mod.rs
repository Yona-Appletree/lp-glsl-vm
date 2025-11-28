//! Parser for IR text format (Cranelift CLIF-style).

mod block;
mod error;
mod function;
mod instructions;
mod module;
mod primitives;
mod validation;
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
    match parse_function_internal(trimmed, 0) {
        Ok(("", func)) => {
            // Validate all aspects of the function
            if let Err(err_msg) = validation::validate_block_indices(&func) {
                return Err(ParseError {
                    message: err_msg,
                    position: 0, // Position is approximate for validation errors
                });
            }
            if let Err(err_msg) = validation::validate_block_parameters(&func) {
                return Err(ParseError {
                    message: err_msg,
                    position: 0,
                });
            }
            if let Err(err_msg) = validation::validate_return_values(&func) {
                return Err(ParseError {
                    message: err_msg,
                    position: 0,
                });
            }
            if let Err(err_msg) = validation::validate_terminating_instructions(&func) {
                return Err(ParseError {
                    message: err_msg,
                    position: 0,
                });
            }
            if let Err(err_msg) = validation::validate_entry_block(&func) {
                return Err(ParseError {
                    message: err_msg,
                    position: 0,
                });
            }
            if let Err(err_msg) = validation::validate_value_scoping(&func) {
                return Err(ParseError {
                    message: err_msg,
                    position: 0, // Position is approximate for validation errors
                });
            }
            Ok(func)
        }
        Ok((remaining, func)) => {
            // Allow trailing whitespace
            if remaining.trim().is_empty() {
                // Validate all aspects of the function
                if let Err(err_msg) = validation::validate_block_indices(&func) {
                    return Err(ParseError {
                        message: err_msg,
                        position: 0,
                    });
                }
                if let Err(err_msg) = validation::validate_block_parameters(&func) {
                    return Err(ParseError {
                        message: err_msg,
                        position: 0,
                    });
                }
                if let Err(err_msg) = validation::validate_return_values(&func) {
                    return Err(ParseError {
                        message: err_msg,
                        position: 0,
                    });
                }
                if let Err(err_msg) = validation::validate_terminating_instructions(&func) {
                    return Err(ParseError {
                        message: err_msg,
                        position: 0,
                    });
                }
                if let Err(err_msg) = validation::validate_entry_block(&func) {
                    return Err(ParseError {
                        message: err_msg,
                        position: 0,
                    });
                }
                if let Err(err_msg) = validation::validate_value_scoping(&func) {
                    return Err(ParseError {
                        message: err_msg,
                        position: 0, // Position is approximate for validation errors
                    });
                }
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
        assert_eq!(func.block_count(), 1);
        let block = func.entry_block().unwrap();
        let insts: alloc::vec::Vec<_> = func.block_insts(block).collect();
        assert_eq!(insts.len(), 2);
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

    #[test]
    fn test_validate_value_scoping_valid() {
        // Test that valid IR passes validation
        let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 0
    v3 = iconst 1
    brif v3, block1(v1), block2(v2)

block1(v4: i32):
    return v4

block2(v5: i32):
    return v5
}"#;
        let result = parse_function(input.trim());
        assert!(
            result.is_ok(),
            "Valid IR should pass validation: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_value_scoping_valid_cross_block() {
        // Test that using a value from another block is valid if dominated
        // This is CLIF-style cross-block usage - should PASS
        let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 0
    v3 = iconst 1
    brif v3, block1, block2

block1:
    return v1

block2:
    return v2
}"#;
        let result = parse_function(input.trim());
        assert!(
            result.is_ok(),
            "Cross-block usage should be valid (block0 dominates block1 and block2): {:?}",
            result
        );
    }

    #[test]
    fn test_validate_value_scoping_valid_with_params() {
        // Test that passing values as parameters works
        let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    v2 = iconst 0
    v3 = iconst 1
    brif v3, block1(v1), block2(v2)

block1(v4: i32):
    return v4

block2(v5: i32):
    return v5
}"#;
        let result = parse_function(input.trim());
        assert!(
            result.is_ok(),
            "Passing values as parameters should be valid: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_value_scoping_invalid_jump_args() {
        // Test dominance violation: block2 uses v1 from block0, but block0 doesn't dominate block2
        // CFG: block0 -> block1, block0 -> block2, block1 -> block3, block2 -> block3
        // block0 dominates block1 and block3, but NOT block2 (path block0->block2 exists but
        // block2 can also be reached via other paths that don't go through block0... wait, that's not right.
        // Actually: In a diamond pattern, block0 dominates all blocks. To create a violation,
        // we need block2 to use v1, but v1 must come from a block that doesn't dominate block2.
        // Let's use: block0 -> block1, block0 -> block2, block1 -> block3, block2 -> block3
        // block2 uses v1 from block0. But actually block0 DOES dominate block2.
        // Better: block0 -> block1 -> block2, block0 -> block3, block2 -> block3
        // block3 uses v1 from block0, but block0 doesn't dominate block3? No, it does.
        // Actually, the issue is: block2 uses v1 from block0 in its jump args.
        // But v1 was passed to block1, not available in block2's scope.
        // Let me create a case where block2 uses a value from block1, but block1 doesn't dominate block2:
        // block0 -> block1, block0 -> block2, block1 -> block3, block2 -> block3
        // block2 uses v1 from block1, but block1 doesn't dominate block2 (path block0->block2 doesn't go through block1)
        let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v2 = iconst 0
    brif v0, block1, block2(v2)

block1:
    v1 = iconst 42
    v4 = iconst 0
    jump block3(v4)

block2(v3: i32):
    jump block3(v1)

block3(v5: i32):
    return v5
}"#;
        let result = parse_function(input.trim());
        assert!(result.is_err(), "Jump with non-dominated value should fail");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("Value 1")
                && err.message.contains("used in block")
                && err.message.contains("dominated"),
            "Error message should mention dominance violation: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_value_scoping_invalid_branch_args() {
        // Test dominance violation: block3 uses v1 from block1, but block1 doesn't dominate block3
        // CFG: block0 -> block1, block0 -> block2, block1 -> block3, block2 -> block3
        // block1 doesn't dominate block3 because there's a path block0->block2->block3 that doesn't go through block1
        let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v2 = iconst 0
    brif v0, block1, block2(v2)

block1:
    v1 = iconst 42
    jump block3

block2(v3: i32):
    jump block3

block3:
    v4 = iconst 0
    brif v4, block1, block2(v1)
}"#;
        let result = parse_function(input.trim());
        assert!(
            result.is_err(),
            "Branch with non-dominated value should fail"
        );
        let err = result.unwrap_err();
        assert!(
            err.message.contains("Value 1")
                && err.message.contains("used in block")
                && err.message.contains("dominated"),
            "Error message should mention dominance violation: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_value_scoping_valid_same_block() {
        // Test that using values within the same block works
        let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 2
    v3 = iadd v1, v2
    v4 = imul v3, v1
    return v4
}"#;
        let result = parse_function(input.trim());
        assert!(
            result.is_ok(),
            "Using values within same block should be valid: {:?}",
            result
        );
    }
}
