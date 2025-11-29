//! The `domtree` subtest - dominator tree tests

use crate::filecheck::match_filecheck;
use crate::parser::parse_test_file;
use std::collections::BTreeMap;
use lpc_lpir::{parse_function, ControlFlowGraph, DominatorTree};

/// Run tests from domtree test files
#[allow(dead_code)]
pub fn run_tests_from_file(content: &str) {
    let test_cases = parse_test_file(content);

    assert!(!test_cases.is_empty(), "No test cases found in test file");

    for case in test_cases {
        assert_eq!(
            case.command, "test domtree",
            "Unexpected test command: {}",
            case.command
        );
        run_domtree_test(&case.function_text, &case.expected_text);
    }
}

/// Format dominator tree in preorder traversal
#[allow(dead_code)]
fn format_domtree_preorder(
    func: &lpc_lpir::Function,
    cfg: &ControlFlowGraph,
    domtree: &DominatorTree,
) -> String {
    let mut output = Vec::new();
    let mut block_name_to_index = BTreeMap::new();
    for (idx, _block) in func.blocks().enumerate() {
        let block_name = format!("block{}", idx);
        block_name_to_index.insert(block_name, idx);
    }

    // Build dominator tree structure: block -> children
    let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for block_idx in 0..func.block_count() {
        if let Some(idom) = domtree.immediate_dominator(block_idx) {
            children.entry(idom).or_insert_with(|| Vec::new()).push(block_idx);
        }
    }

    // Sort children for deterministic output
    for children_list in children.values_mut() {
        children_list.sort();
    }

    // Preorder traversal starting from entry
    fn preorder(
        block_idx: usize,
        children: &BTreeMap<usize, Vec<usize>>,
        output: &mut Vec<String>,
    ) {
        let block_name = format!("block{}", block_idx);
        let children_list = children.get(&block_idx).cloned().unwrap_or_default();

        if children_list.is_empty() {
            output.push(format!("{}:", block_name));
        } else {
            let children_str: Vec<String> = children_list.iter().map(|i| format!("block{}", i)).collect();
            output.push(format!("{}: {}", block_name, children_str.join(" ")));
        }

        for child in children_list {
            preorder(child, children, output);
        }
    }

    preorder(cfg.entry(), &children, &mut output);
    output.join("\n")
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

/// Extract dominance annotations from function text
/// Returns a map from block name to set of blocks it should dominate
#[allow(dead_code)]
fn extract_dominance_annotations(function_text: &str) -> BTreeMap<String, Vec<String>> {
    let mut annotations = BTreeMap::new();
    let lines: Vec<&str> = function_text.lines().collect();

    for line in lines {
        if let Some(dom_start) = line.find("; dominates:") {
            // Extract block name from the line (should be before the annotation)
            // Format: "block0:" or "block0(v0: i32):"
            let before_annotation = &line[..dom_start];
            if let Some(block_name_end) = before_annotation.find(':') {
                let block_name = String::from(before_annotation[..block_name_end].trim());

                // Extract dominated blocks
                let dominated_str = &line[dom_start + 12..].trim();
                let dominated: Vec<String> = dominated_str
                    .split_whitespace()
                    .map(|s| String::from(s))
                    .collect();

                annotations.insert(block_name, dominated);
            }
        }
    }

    annotations
}

/// Run a single domtree test
#[allow(dead_code)]
fn run_domtree_test(function_text: &str, expected_text: &str) {
    let func = parse_function(function_text.trim()).unwrap_or_else(|e| {
        panic!(
            "Failed to parse function: {:?}\n\nFunction text:\n{}",
            e, function_text
        )
    });

    // Build CFG and dominator tree
    let cfg = ControlFlowGraph::from_function(&func);
    let domtree = DominatorTree::from_cfg(&cfg);

    // Check if expected_text contains filecheck directives
    if !expected_text.trim().is_empty() {
        // Use filecheck matching
        let mut actual_output: Vec<String> = Vec::new();

        // Format cfg_postorder if requested
        if expected_text.contains("cfg_postorder") {
            let post_order = format_cfg_postorder(&cfg);
            actual_output.push(String::from("cfg_postorder:"));
            for block in post_order {
                actual_output.push(block);
            }
        }

        // Format domtree_preorder if requested
        if expected_text.contains("domtree_preorder") {
            let preorder = format_domtree_preorder(&func, &cfg, &domtree);
            actual_output.push(String::from("domtree_preorder {"));
            for line in preorder.lines() {
                actual_output.push(String::from(line));
            }
            actual_output.push(String::from("}"));
        }

        let actual = actual_output.join("\n");
        if let Err(e) = match_filecheck(&actual, expected_text) {
            panic!(
                "Domtree test failed (filecheck): {}\n\nExpected:\n{}\n\nActual:\n{}\n\nFunction:\n{}",
                e, expected_text, actual, function_text
            );
        }
    } else {
        // Fall back to annotation-based matching
        // Build block name to index mapping
        let mut block_name_to_index = BTreeMap::new();
        for (idx, _block) in func.blocks().enumerate() {
            // Block names are like "block0", "block1", etc.
            let block_name = format!("block{}", idx);
            block_name_to_index.insert(block_name, idx);
        }

        // Extract dominance annotations
        let annotations = extract_dominance_annotations(function_text);

        // Verify dominance relationships
        for (block_name, expected_dominated) in annotations {
            if let Some(&block_idx) = block_name_to_index.get(&block_name) {
                for dominated_name in expected_dominated {
                    if let Some(&dominated_idx) = block_name_to_index.get(&dominated_name) {
                        if !domtree.dominates(block_idx, dominated_idx) {
                            panic!(
                                "Domtree test failed: {} does not dominate {}\n\nFunction:\n{}",
                                block_name, dominated_name, function_text
                            );
                        }
                    } else {
                        panic!(
                            "Domtree test failed: dominated block '{}' not found\n\nFunction:\n{}",
                            dominated_name, function_text
                        );
                    }
                }
            } else {
                panic!(
                    "Domtree test failed: block '{}' not found\n\nFunction:\n{}",
                    block_name, function_text
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domtree_basic() {
        let content = include_str!("../filetests/domtree/basic.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_domtree_loops() {
        let content = include_str!("../filetests/domtree/loops.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_domtree_complex() {
        let content = include_str!("../filetests/domtree/complex.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_domtree_wide_tree() {
        let content = include_str!("../filetests/domtree/wide-tree.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_domtree_tall_tree() {
        let content = include_str!("../filetests/domtree/tall-tree.lpir");
        run_tests_from_file(content);
    }
}

