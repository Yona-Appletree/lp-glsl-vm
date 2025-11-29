//! The `transform` subtest - transformation pass tests

extern crate alloc;

use crate::parser::{normalize_ir, parse_test_file};
use alloc::format;
use lpc_lpir::{convert_floats_to_fixed16x16, parse_function};

/// Run tests from transform test files
#[allow(dead_code)]
pub fn run_tests_from_file(content: &str) {
    let test_cases = parse_test_file(content);

    assert!(!test_cases.is_empty(), "No test cases found in test file");

    for case in test_cases {
        assert!(
            case.command.starts_with("test transform"),
            "Unexpected test command: {}",
            case.command
        );
        run_transform_test(&case.function_text, &case.expected_text);
    }
}

/// Run a single transform test
#[allow(dead_code)]
fn run_transform_test(function_text: &str, expected_text: &str) {
    let mut func = parse_function(function_text.trim()).unwrap_or_else(|e| {
        panic!(
            "Failed to parse function: {:?}\n\nFunction text:\n{}",
            e, function_text
        )
    });

    convert_floats_to_fixed16x16(&mut func).unwrap_or_else(|e| {
        panic!(
            "Transformation failed: {:?}\n\nFunction text:\n{}",
            e, function_text
        )
    });

    let actual = format!("{}", func);
    let actual_normalized = normalize_ir(&actual);
    let expected_normalized = normalize_ir(expected_text);

    if actual_normalized != expected_normalized {
        panic!(
            "Transform test failed!\n\nExpected:\n{}\n\nActual:\n{}\n\nOriginal function:\n{}",
            expected_text, actual, function_text
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_point_transform() {
        let content = include_str!("../filetests/transform/fixed-point.lpir");
        run_tests_from_file(content);
    }
}

