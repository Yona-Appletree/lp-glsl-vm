//! The `compile` subtest - backend3 compilation tests
//!
//! Tests that LPIR functions compile correctly through the backend3 pipeline
//! and emit expected RISC-V 32 assembly code.

extern crate alloc;

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use lpc_lpir::parse_function;

use crate::{
    filecheck::{match_filecheck, parse_filecheck_directives},
    parser::parse_test_file,
};

/// Run tests from compile test files
pub fn run_tests_from_file(content: &str) {
    let test_cases = parse_test_file(content);

    assert!(!test_cases.is_empty(), "No test cases found in test file");

    for case in test_cases {
        assert_eq!(
            case.command, "test compile",
            "Unexpected test command: {}",
            case.command
        );
        run_compile_test(&case.function_text, &case.expected_text);
    }
}

/// Run a single compile test
fn run_compile_test(function_text: &str, expected_text: &str) {
    // Parse function(s) from text
    // Support multiple functions in one test (for call tests)
    let functions = parse_functions(function_text);

    // Compile each function and collect emitted code
    let mut all_emitted_code = Vec::new();
    let mut symbol_table = lpc_codegen::backend3::symbols::SymbolTable::new();

    for (func_name, func) in &functions {
        // Lower function to VCode
        use lpc_codegen::{
            backend3::{lower::lower_function, vcode::Callee},
            isa::riscv32::backend3::{inst::Riscv32ABI, Riscv32LowerBackend},
        };

        let backend = Riscv32LowerBackend;
        let abi = Callee { abi: Riscv32ABI };
        let vcode = lower_function(func.clone(), &backend, abi);

        // Run register allocation
        let regalloc = vcode
            .run_regalloc()
            .expect("register allocation should succeed");

        // Emit code with symbol table
        let buffer = vcode.emit(&regalloc, Some(&mut symbol_table), Some(func_name));

        // Disassemble emitted code
        let code_bytes = buffer.as_bytes();
        let labels = build_label_map(&buffer, func_name);
        use lpc_codegen::disassemble_code_with_labels;
        let disasm = disassemble_code_with_labels(&code_bytes, Some(&labels));

        // Add function header and disassembly to output
        all_emitted_code.push(format!("function %{}", func_name));
        all_emitted_code.push(disasm);
    }

    let actual_output = all_emitted_code.join("\n");

    // Check if expected_text contains filecheck directives
    let directives = parse_filecheck_directives(expected_text);
    if !directives.is_empty() {
        // Use filecheck matching
        if let Err(e) = match_filecheck(&actual_output, &directives) {
            panic!(
                "Compile test failed (filecheck): \
                 {}\n\nExpected:\n{}\n\nActual:\n{}\n\nFunction:\n{}",
                e, expected_text, actual_output, function_text
            );
        }
    } else {
        // Fall back to simple text matching (normalize whitespace)
        let actual_normalized = normalize_assembly(&actual_output);
        let expected_normalized = normalize_assembly(expected_text);

        if actual_normalized != expected_normalized {
            panic!(
                "Compile test failed!\n\nExpected:\n{}\n\nActual:\n{}\n\nFunction:\n{}",
                expected_text, actual_output, function_text
            );
        }
    }
}

/// Parse one or more functions from text
/// Returns a vector of (function_name, function) pairs
fn parse_functions(text: &str) -> Vec<(String, lpc_lpir::Function)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut functions = Vec::new();
    let mut i = 0;

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

            // Extract function name
            let func_line = lines[i].trim();
            let func_name = if let Some(percent_pos) = func_line.find('%') {
                let name_part = &func_line[percent_pos + 1..];
                if let Some(space_pos) = name_part.find(' ') {
                    String::from(&name_part[..space_pos])
                } else if let Some(paren_pos) = name_part.find('(') {
                    String::from(&name_part[..paren_pos])
                } else {
                    String::from(name_part)
                }
            } else {
                format!("func{}", functions.len())
            };

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
                .map(|l| String::from(*l))
                .collect::<Vec<_>>()
                .join("\n");

            // Parse function
            let func = parse_function(&function_text).unwrap_or_else(|e| {
                panic!(
                    "Failed to parse function: {:?}\n\nFunction text:\n{}",
                    e, function_text
                )
            });

            functions.push((func_name, func));
            i = function_end + 1;
        } else {
            i += 1;
        }
    }

    functions
}

/// Build a label map for disassembly
/// Maps code offsets to label names (e.g., ".Lblock0")
fn build_label_map(
    _buffer: &lpc_codegen::isa::riscv32::inst_buffer::InstBuffer,
    _func_name: &str,
) -> BTreeMap<u32, String> {
    // For now, labels are auto-generated by disassemble_code_with_labels
    // In the future, we could extract block labels from VCode and map them
    // to code offsets, but the auto-generation should work for most tests
    BTreeMap::new()
}

/// Normalize assembly text for comparison
fn normalize_assembly(asm: &str) -> Vec<String> {
    asm.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| String::from(l))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_lowering() {
        let content = include_str!("../filetests/backend3/branch-lowering.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_branch_emission() {
        let content = include_str!("../filetests/backend3/branch-emission.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_call_lowering() {
        let content = include_str!("../filetests/backend3/call-lowering.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_call_emission() {
        let content = include_str!("../filetests/backend3/call-emission.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_reloc_fixup() {
        let content = include_str!("../filetests/backend3/reloc-fixup.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_complex_cfg() {
        let content = include_str!("../filetests/backend3/complex-cfg.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_branch_range() {
        let content = include_str!("../filetests/backend3/branch-range.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_label_fixup() {
        let content = include_str!("../filetests/backend3/label-fixup.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_multi_return() {
        let content = include_str!("../filetests/backend3/multi-return.lpir");
        run_tests_from_file(content);
    }

    #[test]
    fn test_branch_no_fallthrough() {
        let content = include_str!("../filetests/backend3/branch-no-fallthrough.lpir");
        run_tests_from_file(content);
    }
}
