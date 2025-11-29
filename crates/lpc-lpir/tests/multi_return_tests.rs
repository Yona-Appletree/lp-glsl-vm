//! Integration tests for multi-return functionality in LPIR.
//!
//! These tests verify that functions can return multiple values (3+),
//! that validation correctly checks return counts and types,
//! and that calls with multi-return work correctly.

extern crate alloc;

use lpc_lpir::{parse_function, parse_module, verify, verify_module, Type};

#[test]
fn test_multi_return_success_three_returns() {
    // Function with 3 return values matching signature
    let input = r#"function %test(i32) -> i32, f32, i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = fconst 2.0
    v3 = iconst 3
    return v1, v2, v3
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Function with 3 returns matching signature should parse: {:?}",
        result
    );

    let func = result.unwrap();
    assert_eq!(func.return_count(), 3);
    assert!(func.uses_multi_return());

    // Verify the function
    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid multi-return function should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_success_four_returns() {
    // Function with 4 return values
    let input = r#"function %test() -> i32, f32, i32, f32 {
block0:
    v1 = iconst 1
    v2 = fconst 2.0
    v3 = iconst 3
    v4 = fconst 4.0
    return v1, v2, v3, v4
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Function with 4 returns should parse: {:?}",
        result
    );

    let func = result.unwrap();
    assert_eq!(func.return_count(), 4);
    assert!(func.uses_multi_return());

    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid 4-return function should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_success_five_returns() {
    // Function with 5 return values
    let input = r#"function %test() -> i32, i32, i32, i32, i32 {
block0:
    v1 = iconst 1
    v2 = iconst 2
    v3 = iconst 3
    v4 = iconst 4
    v5 = iconst 5
    return v1, v2, v3, v4, v5
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Function with 5 returns should parse: {:?}",
        result
    );

    let func = result.unwrap();
    assert_eq!(func.return_count(), 5);
    assert!(func.uses_multi_return());

    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid 5-return function should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_failure_wrong_count_too_few() {
    // Function signature expects 3 returns, but only 2 provided
    let input = r#"function %test() -> i32, f32, i32 {
block0:
    v1 = iconst 1
    v2 = fconst 2.0
    return v1, v2
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_err(),
        "Function with wrong return count (too few) should fail: {:?}",
        result
    );

    if let Err(err) = result {
        assert!(
            err.message.contains("returns 2 values") || err.message.contains("expects 3"),
            "Error should mention return count mismatch: {}",
            err.message
        );
    }
}

#[test]
fn test_multi_return_failure_wrong_count_too_many() {
    // Function signature expects 2 returns, but 3 provided
    let input = r#"function %test() -> i32, f32 {
block0:
    v1 = iconst 1
    v2 = fconst 2.0
    v3 = iconst 3
    return v1, v2, v3
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_err(),
        "Function with wrong return count (too many) should fail: {:?}",
        result
    );

    if let Err(err) = result {
        assert!(
            err.message.contains("returns 3 values") || err.message.contains("expects 2"),
            "Error should mention return count mismatch: {}",
            err.message
        );
    }
}

#[test]
fn test_multi_return_failure_wrong_type() {
    // Function signature expects i32, f32, i32 but gets i32, i32, i32
    let input = r#"function %test() -> i32, f32, i32 {
block0:
    v1 = iconst 1
    v2 = iconst 2
    v3 = iconst 3
    return v1, v2, v3
}
"#;
    let result = parse_function(input.trim());
    // Parsing might succeed, but verification should fail
    if let Ok(func) = result {
        let verify_result = verify(&func, None);
        assert!(
            verify_result.is_err(),
            "Function with wrong return types should fail verification: {:?}",
            verify_result
        );

        if let Err(errors) = verify_result {
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("type") && e.message.contains("f32")),
                "Error should mention type mismatch: {:?}",
                errors
            );
        }
    } else {
        // Parsing might catch it too
        assert!(result.is_err());
    }
}

#[test]
fn test_multi_return_failure_wrong_type_order() {
    // Function signature expects i32, f32 but gets f32, i32
    let input = r#"function %test() -> i32, f32 {
block0:
    v1 = fconst 1.0
    v2 = iconst 2
    return v1, v2
}
"#;
    let result = parse_function(input.trim());
    if let Ok(func) = result {
        let verify_result = verify(&func, None);
        assert!(
            verify_result.is_err(),
            "Function with wrong return type order should fail verification: {:?}",
            verify_result
        );
    } else {
        assert!(result.is_err());
    }
}

#[test]
fn test_multi_return_call_success() {
    // Call a function with 3+ returns
    let input = r#"module {
function %callee(i32) -> i32, f32, i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = fconst 2.0
    v3 = iconst 3
    return v1, v2, v3
}

function %caller() -> i32, f32, i32 {
block0:
    v0 = iconst 42
    v1, v2, v3 = call %callee(v0)
    return v1, v2, v3
}
}
"#;
    let result = parse_module(input.trim());
    assert!(
        result.is_ok(),
        "Module with multi-return call should parse: {:?}",
        result
    );

    let module = result.unwrap();
    let verify_result = verify_module(&module);
    assert!(
        verify_result.is_ok(),
        "Module with valid multi-return call should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_call_failure_wrong_result_count() {
    // Call expects 3 results but only gets 2
    let input = r#"module {
function %callee(i32) -> i32, f32, i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = fconst 2.0
    v3 = iconst 3
    return v1, v2, v3
}

function %caller() -> i32, f32 {
block0:
    v0 = iconst 42
    v1, v2 = call %callee(v0)
    return v1, v2
}
}
"#;
    let result = parse_module(input.trim());
    if let Ok(module) = result {
        let verify_result = verify_module(&module);
        assert!(
            verify_result.is_err(),
            "Call with wrong result count should fail verification: {:?}",
            verify_result
        );

        if let Err(errors) = verify_result {
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("returns 3 values")
                        || e.message.contains("2 results")),
                "Error should mention result count mismatch: {:?}",
                errors
            );
        }
    } else {
        // Parsing might catch it
        assert!(result.is_err());
    }
}

#[test]
fn test_multi_return_call_failure_wrong_arg_count() {
    // Call with wrong argument count
    let input = r#"module {
function %callee(i32, i32) -> i32, f32, i32 {
block0(v0: i32, v1: i32):
    v2 = iconst 1
    v3 = fconst 2.0
    v4 = iconst 3
    return v2, v3, v4
}

function %caller() -> i32, f32, i32 {
block0:
    v0 = iconst 42
    v1, v2, v3 = call %callee(v0)
    return v1, v2, v3
}
}
"#;
    let result = parse_module(input.trim());
    if let Ok(module) = result {
        let verify_result = verify_module(&module);
        assert!(
            verify_result.is_err(),
            "Call with wrong arg count should fail verification: {:?}",
            verify_result
        );

        if let Err(errors) = verify_result {
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("expects 2 arguments")
                        || e.message.contains("got 1")),
                "Error should mention argument count mismatch: {:?}",
                errors
            );
        }
    } else {
        assert!(result.is_err());
    }
}

#[test]
fn test_multi_return_multiple_functions() {
    // Module with multiple functions using multi-return
    let input = r#"module {
function %func1() -> i32, f32, i32 {
block0:
    v1 = iconst 1
    v2 = fconst 2.0
    v3 = iconst 3
    return v1, v2, v3
}

function %func2() -> i32, i32, i32, i32 {
block0:
    v1 = iconst 10
    v2 = iconst 20
    v3 = iconst 30
    v4 = iconst 40
    return v1, v2, v3, v4
}

function %caller() -> i32, f32, i32 {
block0:
    v1, v2, v3 = call %func1()
    return v1, v2, v3
}
}
"#;
    let result = parse_module(input.trim());
    assert!(
        result.is_ok(),
        "Module with multiple multi-return functions should parse: {:?}",
        result
    );

    let module = result.unwrap();
    let verify_result = verify_module(&module);
    assert!(
        verify_result.is_ok(),
        "Module with multiple multi-return functions should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_nested_calls() {
    // Nested calls with multi-return
    let input = r#"module {
function %inner() -> i32, f32, i32 {
block0:
    v1 = iconst 1
    v2 = fconst 2.0
    v3 = iconst 3
    return v1, v2, v3
}

function %middle() -> i32, f32, i32 {
block0:
    v1, v2, v3 = call %inner()
    return v1, v2, v3
}

function %outer() -> i32, f32, i32 {
block0:
    v1, v2, v3 = call %middle()
    return v1, v2, v3
}
}
"#;
    let result = parse_module(input.trim());
    assert!(
        result.is_ok(),
        "Module with nested multi-return calls should parse: {:?}",
        result
    );

    let module = result.unwrap();
    let verify_result = verify_module(&module);
    assert!(
        verify_result.is_ok(),
        "Module with nested multi-return calls should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_void_function() {
    // Function with no returns (void)
    let input = r#"function %test() -> {
block0:
    halt
}
"#;
    let result = parse_function(input.trim());
    assert!(result.is_ok(), "Void function should parse: {:?}", result);

    let func = result.unwrap();
    assert_eq!(func.return_count(), 0);
    assert!(!func.uses_multi_return());

    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid void function should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_single_return() {
    // Function with single return (not multi-return)
    let input = r#"function %test() -> i32 {
block0:
    v1 = iconst 42
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Single return function should parse: {:?}",
        result
    );

    let func = result.unwrap();
    assert_eq!(func.return_count(), 1);
    assert!(!func.uses_multi_return());

    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid single return function should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_double_return() {
    // Function with 2 returns (not multi-return, but still valid)
    let input = r#"function %test() -> i32, f32 {
block0:
    v1 = iconst 1
    v2 = fconst 2.0
    return v1, v2
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Double return function should parse: {:?}",
        result
    );

    let func = result.unwrap();
    assert_eq!(func.return_count(), 2);
    assert!(!func.uses_multi_return()); // 2 is not > 2

    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid double return function should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_return_types_helper() {
    // Test return_types() helper method
    let input = r#"function %test() -> i32, f32, i32 {
block0:
    v1 = iconst 1
    v2 = fconst 2.0
    v3 = iconst 3
    return v1, v2, v3
}
"#;
    let result = parse_function(input.trim());
    assert!(result.is_ok());

    let func = result.unwrap();
    let return_types = func.return_types();
    assert_eq!(return_types.len(), 3);
    assert_eq!(return_types[0], Type::I32);
    assert_eq!(return_types[1], Type::F32);
    assert_eq!(return_types[2], Type::I32);
}

#[test]
fn test_multi_return_different_blocks() {
    // Different return paths with same multi-return signature
    let input = r#"function %test(i32) -> i32, f32, i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 10
    v2 = fconst 20.0
    v3 = iconst 30
    return v1, v2, v3

block2:
    v4 = iconst 100
    v5 = fconst 200.0
    v6 = iconst 300
    return v4, v5, v6
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Function with multi-return in different blocks should parse: {:?}",
        result
    );

    let func = result.unwrap();
    assert_eq!(func.return_count(), 3);
    assert!(func.uses_multi_return());

    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid multi-return function with multiple return paths should verify: {:?}",
        verify_result
    );
}

#[test]
fn test_multi_return_mixed_types() {
    // Multi-return with mixed integer and float types
    let input = r#"function %test() -> i32, f32, i32, f32, i32 {
block0:
    v1 = iconst 1
    v2 = fconst 2.5
    v3 = iconst 3
    v4 = fconst 4.5
    v5 = iconst 5
    return v1, v2, v3, v4, v5
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Function with mixed return types should parse: {:?}",
        result
    );

    let func = result.unwrap();
    assert_eq!(func.return_count(), 5);
    assert!(func.uses_multi_return());

    let verify_result = verify(&func, None);
    assert!(
        verify_result.is_ok(),
        "Valid multi-return function with mixed types should verify: {:?}",
        verify_result
    );
}
