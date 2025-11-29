//! Stack allocation tests for LPIR.
//!
//! These tests verify that the `stackalloc` instruction works correctly:
//! - Parsing and formatting
//! - Roundtrip through builder API
//! - Integration with Load/Store operations
//! - Multiple allocations

extern crate alloc;

use alloc::{string::String, vec, vec::Vec};

use lpc_lpir::{parse_function, verify, FunctionBuilder, Signature, Type};

/// Helper function to build a function, format it as text, parse it back, and compare.
fn build_parse_roundtrip<F>(
    name: &str,
    signature: Signature,
    builder_fn: F,
    expected_source: Option<&str>,
) where
    F: FnOnce(&mut FunctionBuilder) -> (),
{
    // Build the function
    let mut function_builder = FunctionBuilder::new(signature.clone(), String::from(name));
    builder_fn(&mut function_builder);
    let built_func = function_builder.finish();

    // Format as text
    let text = format!("{}", built_func);

    // Compare against expected source if provided
    if let Some(expected) = expected_source {
        // Normalize whitespace for comparison (trim each line, normalize newlines)
        let normalized_actual: Vec<&str> = text.lines().map(|l| l.trim()).collect();
        let normalized_expected: Vec<&str> = expected.lines().map(|l| l.trim()).collect();

        assert_eq!(
            normalized_actual, normalized_expected,
            "Formatted IR should match expected source"
        );
    }

    // Parse it back
    let parsed_func = parse_function(&text).expect("Failed to parse built function");

    // Compare key properties
    assert_eq!(
        built_func.name(),
        parsed_func.name(),
        "Function names should match"
    );
    assert_eq!(
        built_func.signature.params.len(),
        parsed_func.signature.params.len(),
        "Parameter counts should match"
    );
    assert_eq!(
        built_func.signature.returns.len(),
        parsed_func.signature.returns.len(),
        "Return counts should match"
    );
    assert_eq!(
        built_func.block_count(),
        parsed_func.block_count(),
        "Block counts should match"
    );
}

#[test]
fn test_stackalloc_parse() {
    // Test parsing stackalloc instruction
    let input = r#"function %test() -> i32 {
block0:
    v0 = stackalloc 4
    v1 = iconst 42
    store.i32 v0, v1
    v2 = load.i32 v0
    return v2
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Function with stackalloc should parse: {:?}",
        result
    );

    let func = result.unwrap();
    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid stackalloc function should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_stackalloc_builder_roundtrip() {
    // Build: fn stack_test() -> i32 { addr = stackalloc 4; store 42 to addr; load from addr }
    let sig = Signature::new(vec![], vec![Type::I32]);
    let expected = r#"function %stack_test() -> i32 {
block0:
    v0 = stackalloc 4
    v1 = iconst 42
    store.i32 v0, v1
    v2 = load.i32 v0
    return v2
}
"#;
    build_parse_roundtrip(
        "stack_test",
        sig,
        |builder| {
            let entry_block = builder.create_block();
            let addr = builder.new_value();
            let value = builder.new_value();
            let result = builder.new_value();

            {
                let mut block_builder = builder.block_builder(entry_block);
                block_builder.stackalloc(addr, 4);
                block_builder.iconst(value, 42);
                block_builder.store(addr, value, Type::I32);
                block_builder.load(result, addr, Type::I32);
                block_builder.return_(&vec![result]);
            }
        },
        Some(expected),
    );
}

#[test]
fn test_multiple_stackalloc() {
    // Build: fn multi_stack() -> i32 { a = stackalloc 4; b = stackalloc 8; store 10 to a; store 20 to b; load from a }
    let sig = Signature::new(vec![], vec![Type::I32]);
    let expected = r#"function %multi_stack() -> i32 {
block0:
    v0 = stackalloc 4
    v1 = stackalloc 8
    v2 = iconst 10
    store.i32 v0, v2
    v3 = iconst 20
    store.i32 v1, v3
    v4 = load.i32 v0
    return v4
}
"#;
    build_parse_roundtrip(
        "multi_stack",
        sig,
        |builder| {
            let entry_block = builder.create_block();
            let addr_a = builder.new_value();
            let addr_b = builder.new_value();
            let value10 = builder.new_value();
            let value20 = builder.new_value();
            let result = builder.new_value();

            {
                let mut block_builder = builder.block_builder(entry_block);
                block_builder.stackalloc(addr_a, 4);
                block_builder.stackalloc(addr_b, 8);
                block_builder.iconst(value10, 10);
                block_builder.store(addr_a, value10, Type::I32);
                block_builder.iconst(value20, 20);
                block_builder.store(addr_b, value20, Type::I32);
                block_builder.load(result, addr_a, Type::I32);
                block_builder.return_(&vec![result]);
            }
        },
        Some(expected),
    );
}

#[test]
fn test_stackalloc_different_sizes() {
    // Test stackalloc with different sizes
    let input = r#"function %test() -> i32 {
block0:
    v0 = stackalloc 1
    v1 = stackalloc 4
    v2 = stackalloc 8
    v3 = stackalloc 16
    v4 = iconst 42
    store.i32 v1, v4
    v5 = load.i32 v1
    return v5
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Function with multiple stackalloc sizes should parse: {:?}",
        result
    );

    let func = result.unwrap();
    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid stackalloc function with different sizes should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_stackalloc_with_arithmetic() {
    // Test stackalloc address used in arithmetic (iadd for offset)
    let input = r#"function %test() -> i32 {
block0:
    v0 = stackalloc 16
    v1 = iconst 4
    v2 = iadd v0, v1
    v3 = iconst 42
    store.i32 v2, v3
    v4 = load.i32 v2
    return v4
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Function with stackalloc address arithmetic should parse: {:?}",
        result
    );

    let func = result.unwrap();
    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid stackalloc function with address arithmetic should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_stackalloc_parameter_passing() {
    // Test stackalloc address passed as function parameter
    let input = r#"module {
function %helper(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 100
    store.i32 v0, v1
    v2 = load.i32 v0
    return v2
}

function %main() -> i32 {
block0:
    v0 = stackalloc 4
    call %helper(v0) -> v1
    v2 = load.i32 v0
    return v2
}
}
"#;
    use lpc_lpir::{parse_module, verify_module};
    let result = parse_module(input.trim());
    assert!(
        result.is_ok(),
        "Module with stackalloc parameter passing should parse: {:?}",
        result
    );

    let module = result.unwrap();
    let verify_result = verify_module(&module);
    assert!(
        verify_result.is_ok(),
        "Valid module with stackalloc parameter passing should verify: {:?}",
        verify_result
    );
}
