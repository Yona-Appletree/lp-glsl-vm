//! The `print-cfg` subtest - CFG construction tests

use crate::filecheck::match_filecheck;
use crate::parser::{normalize_ir, parse_test_file};
use lpc_lpir::{parse_function, ControlFlowGraph};

/// Run tests from cfg test files
#[allow(dead_code)]
pub fn run_tests_from_file(content: &str) {
    let test_cases = parse_test_file(content);

    assert!(!test_cases.is_empty(), "No test cases found in test file");

    for case in test_cases {
        assert_eq!(
            case.command, "test print-cfg",
            "Unexpected test command: {}",
            case.command
        );
        run_print_cfg_test(&case.function_text, &case.expected_text);
    }
}

/// Format CFG in post-order
/// Uses the same algorithm as ControlFlowGraph::reverse_post_order but reverses it
#[allow(dead_code)]
fn format_cfg_postorder(cfg: &ControlFlowGraph) -> Vec<String> {
    // Use the CFG's built-in reverse_post_order and reverse it to get post-order
    let rpo = cfg.reverse_post_order();
    rpo.iter()
        .rev()
        .map(|&idx| format!("block{}", idx))
        .collect()
}

/// Run a single print-cfg test
#[allow(dead_code)]
fn run_print_cfg_test(function_text: &str, expected_text: &str) {
    let func = parse_function(function_text.trim()).unwrap_or_else(|e| {
        panic!(
            "Failed to parse function: {:?}\n\nFunction text:\n{}",
            e, function_text
        )
    });

    // Build CFG
    let cfg = ControlFlowGraph::from_function(&func);

    // Check if expected_text contains filecheck directives
    if !expected_text.trim().is_empty() {
        // Use filecheck matching
        let mut actual_output = Vec::new();

        // Format cfg_postorder if requested
        if expected_text.contains("cfg_postorder") {
            let post_order = format_cfg_postorder(&cfg);
            actual_output.push(String::from("cfg_postorder:"));
            for block in post_order {
                actual_output.push(block);
            }
        }

        // Predecessors
        if expected_text.contains("predecessors") {
            actual_output.push(String::from("predecessors:"));
            for idx in 0..func.block_count() {
                let preds: Vec<usize> = cfg.predecessors(idx).iter().copied().collect();
                actual_output.push(format!("block{}: {:?}", idx, preds));
            }
        }

        let actual = actual_output.join("\n");
        if let Err(e) = match_filecheck(&actual, expected_text) {
            panic!(
                "Print-cfg test failed (filecheck): {}\n\nExpected:\n{}\n\nActual:\n{}\n\nFunction:\n{}",
                e, expected_text, actual, function_text
            );
        }
    } else {
        // Fall back to simple text matching
        let mut output = Vec::new();

        // Post-order
        let post_order = format_cfg_postorder(&cfg);
        output.push(String::from("cfg_postorder:"));
        for block in post_order {
            output.push(format!("  {}", block));
        }

        // Predecessors
        output.push(String::from("predecessors:"));
        for idx in 0..func.block_count() {
            let preds: Vec<usize> = cfg.predecessors(idx).iter().copied().collect();
            output.push(format!("  block{}: {:?}", idx, preds));
        }

        let actual = output.join("\n");
        let actual_normalized = normalize_ir(&actual);
        let expected_normalized = normalize_ir(expected_text);

        if actual_normalized != expected_normalized {
            panic!(
                "Print-cfg test failed!\n\nExpected:\n{}\n\nActual:\n{}\n\nFunction:\n{}",
                expected_text, actual, function_text
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_cfg_basic() {
        let content = include_str!("../filetests/cfg/basic.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_print_cfg_loops() {
        let content = include_str!("../filetests/cfg/loops.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_print_cfg_complex() {
        let content = include_str!("../filetests/cfg/complex.lpir");
        run_tests_from_file(content);
    }
}

