//! The `verifier` subtest - verifier error detection tests

use crate::parser::parse_test_file;
use std::collections::BTreeMap;
use lpc_lpir::{parse_function, verify};

/// Run tests from verifier test files
#[allow(dead_code)]
pub fn run_tests_from_file(content: &str) {
    let test_cases = parse_test_file(content);

    assert!(!test_cases.is_empty(), "No test cases found in test file");

    for case in test_cases {
        assert_eq!(
            case.command, "test verifier",
            "Unexpected test command: {}",
            case.command
        );
        run_verifier_test(&case.function_text, &case.expected_text);
    }
}

/// Extract error annotations from function text
/// Returns a map from instruction line number to expected error message
#[allow(dead_code)]
fn extract_error_annotations(function_text: &str) -> BTreeMap<usize, String> {
    let mut errors = BTreeMap::new();
    let lines: Vec<&str> = function_text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(error_start) = line.find("; error:") {
            let error_msg = String::from(line[error_start + 9..].trim());
            errors.insert(line_idx, error_msg);
        }
    }

    errors
}

/// Run a single verifier test
#[allow(dead_code)]
fn run_verifier_test(function_text: &str, _expected_text: &str) {
    let func = parse_function(function_text.trim()).unwrap_or_else(|e| {
        panic!(
            "Failed to parse function: {:?}\n\nFunction text:\n{}",
            e, function_text
        )
    });

    // Extract expected errors from annotations
    let expected_errors = extract_error_annotations(function_text);

    // Run verifier
    let result = verify(&func, None);

    if expected_errors.is_empty() {
        // Function should be valid
        if let Err(errors) = result {
            panic!(
                "Verifier test failed: expected valid function but got errors:\n{}\n\nFunction:\n{}",
                errors
                    .iter()
                    .map(|e| format!("  - {}", e.message))
                    .collect::<Vec<_>>()
                    .join("\n"),
                function_text
            );
        }
    } else {
        // Function should have errors
        if let Ok(()) = result {
            panic!(
                "Verifier test failed: expected errors but function is valid\n\nFunction:\n{}",
                function_text
            );
        }

        let actual_errors = result.unwrap_err();

        // Check that we got at least the expected number of errors
        // (we can't match line numbers exactly since verifier doesn't track them)
        if actual_errors.len() < expected_errors.len() {
            panic!(
                "Verifier test failed: expected at least {} errors but got {}\n\nExpected errors:\n{}\n\nActual errors:\n{}\n\nFunction:\n{}",
                expected_errors.len(),
                actual_errors.len(),
                expected_errors
                    .values()
                    .map(|e| format!("  - {}", e))
                    .collect::<Vec<_>>()
                    .join("\n"),
                actual_errors
                    .iter()
                    .map(|e| format!("  - {}", e.message))
                    .collect::<Vec<_>>()
                    .join("\n"),
                function_text
            );
        }

        // Try to match error messages (substring match)
        for expected_msg in expected_errors.values() {
            let expected_msg_str: &str = expected_msg;
            let found = actual_errors
                .iter()
                .any(|e| e.message.contains(expected_msg_str) || expected_msg_str.contains(&e.message));
            if !found {
                panic!(
                    "Verifier test failed: expected error message '{}' not found\n\nActual errors:\n{}\n\nFunction:\n{}",
                    expected_msg,
                    actual_errors
                        .iter()
                        .map(|e| format!("  - {}", e.message))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    function_text
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verifier_dominance() {
        let content = include_str!("../filetests/verifier/dominance.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_verifier_types() {
        let content = include_str!("../filetests/verifier/types.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_verifier_ssa() {
        let content = include_str!("../filetests/verifier/ssa.lpir");
        run_tests_from_file(content);
    }
}

