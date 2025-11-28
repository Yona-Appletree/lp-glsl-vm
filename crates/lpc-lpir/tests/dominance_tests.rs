//! Dominance validation tests for LPIR.
//!
//! These tests demonstrate dominance-based value scoping using parsed examples.
//! They show valid and invalid cases of value usage across blocks based on
//! dominance relationships.

extern crate alloc;

use lpc_lpir::parse_function;

#[test]
fn test_valid_dominance_simple_linear() {
    // Value defined in block0, used in block1
    // block0 dominates block1, so this is valid
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    jump block1

block1:
    v2 = iadd v0, v1
    return v2
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Value from dominating block should be valid: {:?}",
        result
    );
}

#[test]
fn test_valid_dominance_diamond_pattern() {
    // Value defined in block0, used in both block1 and block2
    // block0 dominates both block1 and block2, so this is valid (CLIF-style)
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    brif v0, block1, block2

block1:
    return v1

block2:
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Value from dominating block should be valid in both branches: {:?}",
        result
    );
}

#[test]
fn test_valid_dominance_nested_branches() {
    // Value defined in block0, used in block3
    // block0 dominates block3 (through block1 or block2), so this is valid
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    brif v0, block1, block2

block1:
    jump block3

block2:
    jump block3

block3:
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Value from dominating block should be valid after merge: {:?}",
        result
    );
}

#[test]
fn test_invalid_dominance_diamond_merge() {
    // Value defined in block1, used in block3
    // block1 does NOT dominate block3 (path through block2 doesn't go through block1)
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 42
    jump block3

block2:
    jump block3

block3:
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_err(),
        "Value from non-dominating block should fail validation"
    );
    let err = result.unwrap_err();
    assert!(
        err.message.contains("Value 1")
            && err.message.contains("used in block")
            && err.message.contains("dominated"),
        "Error should mention dominance violation: {}",
        err.message
    );
}

#[test]
fn test_invalid_dominance_sibling_block() {
    // Value defined in block1, used in block2
    // block1 does NOT dominate block2 (they're siblings from block0)
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 42
    return v1

block2:
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_err(),
        "Value from sibling block should fail validation"
    );
    let err = result.unwrap_err();
    assert!(
        err.message.contains("Value 1") && err.message.contains("dominated"),
        "Error should mention dominance violation: {}",
        err.message
    );
}

#[test]
fn test_invalid_dominance_backward_use() {
    // Value defined in block2, used in block1
    // block2 does NOT dominate block1 (block1 comes before block2)
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    return v1

block2:
    v1 = iconst 42
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_err(),
        "Value used before definition should fail validation"
    );
    let err = result.unwrap_err();
    assert!(
        err.message.contains("Value 1") && err.message.contains("dominated"),
        "Error should mention dominance violation: {}",
        err.message
    );
}

#[test]
fn test_valid_phi_node_merge() {
    // Valid phi node: value passed from both block1 and block2 to block3
    // Each value is defined in a block that dominates the merge point
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    brif v0, block1, block2

block1:
    v2 = iconst 10
    jump block3(v2)

block2:
    v3 = iconst 20
    jump block3(v3)

block3(v4: i32):
    return v4
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Valid phi node merge should pass: {:?}",
        result
    );
}

#[test]
fn test_invalid_phi_node_non_dominated() {
    // Invalid phi node: block2 tries to pass v1 from block1
    // block1 does NOT dominate block2
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 10
    jump block3(v1)

block2:
    jump block3(v1)

block3(v2: i32):
    return v2
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_err(),
        "Phi node with non-dominated value should fail"
    );
    let err = result.unwrap_err();
    assert!(
        err.message.contains("Value 1") && err.message.contains("dominated"),
        "Error should mention dominance violation: {}",
        err.message
    );
}

#[test]
fn test_valid_dominance_loop() {
    // Value defined before loop, used inside loop
    // Entry block dominates loop header, so this is valid
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    jump block1

block1:
    v2 = iadd v1, v0
    brif v2, block1, block2

block2:
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Value from dominating block used in loop should be valid: {:?}",
        result
    );
}

#[test]
fn test_invalid_dominance_loop_backward() {
    // Value defined in block2, used in block1
    // block2 does NOT dominate block1 (block1 comes before block2 in execution)
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v2 = iadd v1, v0
    return v2

block2:
    v1 = iconst 42
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(result.is_err(), "Value used before definition should fail");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("Value 1")
            && (err.message.contains("dominated")
                || err.message.contains("used before definition")),
        "Error should mention dominance violation or use-before-def: {}",
        err.message
    );
}

#[test]
fn test_valid_dominance_multiple_levels() {
    // Value defined in block0, used deep in nested structure
    // block0 dominates all blocks, so this is valid
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 42
    brif v0, block1, block2

block1:
    brif v0, block3, block4

block2:
    brif v0, block5, block6

block3:
    return v1

block4:
    return v1

block5:
    return v1

block6:
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Value from root block should be valid at any depth: {:?}",
        result
    );
}

#[test]
fn test_invalid_dominance_branch_args() {
    // Value defined in block1, used as branch argument in block3
    // block1 does NOT dominate block3
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    brif v0, block1, block2

block1:
    v1 = iconst 42
    jump block3

block2:
    jump block3

block3:
    v2 = iconst 0
    brif v2, block4(v1), block5

block4(v3: i32):
    return v3

block5:
    v4 = iconst 0
    return v4
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_err(),
        "Branch argument from non-dominating block should fail"
    );
    let err = result.unwrap_err();
    assert!(
        err.message.contains("Value 1") && err.message.contains("dominated"),
        "Error should mention dominance violation: {}",
        err.message
    );
}

#[test]
fn test_valid_dominance_same_block() {
    // Values used within the same block they're defined
    // This is always valid (definition dominates use within same block)
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 2
    v3 = iadd v1, v2
    v4 = imul v3, v1
    v5 = isub v4, v2
    return v5
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Values used in same block should always be valid: {:?}",
        result
    );
}

#[test]
fn test_invalid_dominance_use_before_def_same_block() {
    // Value used before it's defined in the same block
    // This violates the ordering requirement within a block
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v2, v0
    v2 = iconst 42
    return v1
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_err(),
        "Value used before definition in same block should fail"
    );
    let err = result.unwrap_err();
    assert!(
        err.message.contains("used before definition") || err.message.contains("dominated"),
        "Error should mention use-before-def or dominance: {}",
        err.message
    );
}

#[test]
fn test_valid_dominance_entry_params() {
    // Entry block parameters can be used anywhere
    // Entry block dominates all blocks
    let input = r#"function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    brif v0, block1, block2

block1:
    v2 = iadd v0, v1
    return v2

block2:
    v3 = isub v0, v1
    return v3
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Entry block parameters should be valid everywhere: {:?}",
        result
    );
}

#[test]
fn test_valid_dominance_complex_merge() {
    // Complex merge with multiple paths, all values properly dominated
    let input = r#"function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 0
    brif v0, block1, block2

block1:
    v2 = iconst 10
    brif v1, block3, block4

block2:
    v3 = iconst 20
    brif v1, block5, block6

block3:
    jump block7(v2)

block4:
    jump block7(v2)

block5:
    jump block7(v3)

block6:
    jump block7(v3)

block7(v4: i32):
    return v4
}
"#;
    let result = parse_function(input.trim());
    assert!(
        result.is_ok(),
        "Complex merge with proper dominance should be valid: {:?}",
        result
    );
}
