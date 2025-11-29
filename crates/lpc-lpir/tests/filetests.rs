//! File-based tests for LPIR transformations.
//!
//! Similar to Cranelift's filetests, these tests read `.lpir` files that contain:
//! - Test commands (e.g., `test transform fixed-point`)
//! - Functions to transform
//! - Expected output in comments after each function

extern crate alloc;

use alloc::{string::String, vec::Vec};

use lpc_lpir::{convert_floats_to_fixed16x16, parse_function};

/// A test case extracted from a test file
#[derive(Debug, Clone)]
struct TestCase {
    /// The function text before transformation
    function_text: String,
    /// The expected output text (from comments)
    expected_text: String,
    /// The test command type
    command: String,
}

/// Parse a test file and extract functions with their expected outputs
fn parse_test_file(content: &str) -> Vec<TestCase> {
    let lines: Vec<&str> = content.lines().collect();
    let mut test_cases = Vec::new();
    let mut i = 0;

    // Parse test command from header
    let mut command = String::new();
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("test ") {
            command = line.to_string();
            i += 1;
            // Skip blank lines after command
            while i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            break;
        }
        i += 1;
    }

    // Parse functions and their expected outputs
    while i < lines.len() {
        // Skip blank lines
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        // Look for function definition
        if lines[i].trim().starts_with("function ") {
            let function_start = i;
            let mut brace_count = 0;
            let mut function_end = i;

            // Find the end of the function (matching braces)
            for j in i..lines.len() {
                let line = lines[j];
                for ch in line.chars() {
                    if ch == '{' {
                        brace_count += 1;
                    } else if ch == '}' {
                        brace_count -= 1;
                        if brace_count == 0 {
                            function_end = j;
                            break;
                        }
                    }
                }
                if brace_count == 0 {
                    break;
                }
            }

            // Extract function text
            let function_text: String = lines[function_start..=function_end]
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join("\n");

            // Look for expected output in comments after the function
            let mut expected_start = function_end + 1;
            // Skip blank lines
            while expected_start < lines.len() && lines[expected_start].trim().is_empty() {
                expected_start += 1;
            }

            // Check if there are comments starting with ';'
            if expected_start < lines.len() && lines[expected_start].trim().starts_with(';') {
                let mut expected_end = expected_start;
                // Collect all comment lines until we hit a non-comment line or blank line
                while expected_end < lines.len() {
                    let line = lines[expected_end].trim();
                    if line.is_empty() {
                        // Check if next non-empty line is a comment or function
                        let mut next_non_empty = expected_end + 1;
                        while next_non_empty < lines.len()
                            && lines[next_non_empty].trim().is_empty()
                        {
                            next_non_empty += 1;
                        }
                        if next_non_empty >= lines.len()
                            || lines[next_non_empty].trim().starts_with(';')
                            || lines[next_non_empty].trim().starts_with("function ")
                        {
                            expected_end = next_non_empty;
                            break;
                        }
                        break;
                    } else if line.starts_with(';') {
                        expected_end += 1;
                    } else {
                        break;
                    }
                }

                // Extract expected text (strip ';' prefix from comments)
                let expected_text: String = lines[expected_start..expected_end]
                    .iter()
                    .map(|l| {
                        let trimmed = l.trim();
                        if trimmed.starts_with("; ") {
                            trimmed[2..].to_string()
                        } else if trimmed.starts_with(';') {
                            trimmed[1..].to_string()
                        } else {
                            trimmed.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                test_cases.push(TestCase {
                    function_text,
                    expected_text,
                    command: command.clone(),
                });

                i = expected_end;
            } else {
                // No expected output found, skip this function
                i = function_end + 1;
            }
        } else {
            i += 1;
        }
    }

    test_cases
}

/// Normalize IR text for comparison
fn normalize_ir(ir: &str) -> Vec<String> {
    ir.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Run a single transform test
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

#[test]
fn test_fixed_point_transform() {
    let content = include_str!("filetests/transform/fixed-point.lpir");
    let test_cases = parse_test_file(content);

    assert!(
        !test_cases.is_empty(),
        "No test cases found in fixed-point.lpir"
    );

    for case in test_cases {
        assert_eq!(
            case.command, "test transform fixed-point",
            "Unexpected test command: {}",
            case.command
        );
        run_transform_test(&case.function_text, &case.expected_text);
    }
}
