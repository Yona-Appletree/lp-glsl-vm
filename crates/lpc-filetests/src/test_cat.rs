//! The `cat` subtest - roundtrip parse/print tests

extern crate alloc;

use crate::parser::{normalize_ir, parse_test_file};
use alloc::format;
use lpc_lpir::parse_function;

/// Run tests from cat test files
#[allow(dead_code)]
pub fn run_tests_from_file(content: &str) {
    let test_cases = parse_test_file(content);

    assert!(!test_cases.is_empty(), "No test cases found in test file");

    for case in test_cases {
        assert_eq!(
            case.command, "test cat",
            "Unexpected test command: {}",
            case.command
        );
        run_cat_test(&case.function_text, &case.expected_text);
    }
}

/// Run a single cat test (roundtrip parse/print)
#[allow(dead_code)]
fn run_cat_test(function_text: &str, expected_text: &str) {
    let func = parse_function(function_text.trim()).unwrap_or_else(|e| {
        panic!(
            "Failed to parse function: {:?}\n\nFunction text:\n{}",
            e, function_text
        )
    });

    let actual = format!("{}", func);
    let actual_normalized = normalize_ir(&actual);
    let expected_normalized = normalize_ir(expected_text);

    if actual_normalized != expected_normalized {
        panic!(
            "Cat test failed!\n\nExpected:\n{}\n\nActual:\n{}\n\nOriginal function:\n{}",
            expected_text, actual, function_text
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cat_basic() {
        let content = include_str!("../filetests/cat/basic.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_cat_complex() {
        let content = include_str!("../filetests/cat/complex.lpir");
        run_tests_from_file(content);
    }
}

