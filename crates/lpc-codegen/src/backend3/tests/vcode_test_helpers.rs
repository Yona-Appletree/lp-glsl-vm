//! Test helpers for VCode text format comparison

extern crate alloc;

use alloc::{format, string::String, vec::Vec};

use crate::isa::riscv32::backend3::{inst::Riscv32MachInst, vcode_parser};
use crate::backend3::vcode::VCode;

/// Assert that the actual VCode matches the expected text format.
///
/// This function normalizes whitespace for comparison, making tests more flexible.
pub fn assert_vcode_eq(actual: &VCode<Riscv32MachInst>, expected: &str) {
    let actual_str = format!("{}", actual);
    
    // Normalize both strings: trim each line and remove leading/trailing empty lines
    let normalized_actual: Vec<&str> = actual_str
        .lines()
        .map(|l| l.trim_end())
        .collect();
    let normalized_expected: Vec<&str> = expected
        .lines()
        .map(|l| l.trim_end())
        .collect();

    // Remove leading and trailing empty lines from both
    let actual_start = normalized_actual.iter().position(|s| !s.is_empty()).unwrap_or(normalized_actual.len());
    let actual_end = normalized_actual.iter().rposition(|s| !s.is_empty()).map(|i| i + 1).unwrap_or(actual_start);
    let actual_trimmed = &normalized_actual[actual_start..actual_end];
    
    let expected_start = normalized_expected.iter().position(|s| !s.is_empty()).unwrap_or(normalized_expected.len());
    let expected_end = normalized_expected.iter().rposition(|s| !s.is_empty()).map(|i| i + 1).unwrap_or(expected_start);
    let expected_trimmed = &normalized_expected[expected_start..expected_end];

    assert_eq!(
        actual_trimmed,
        expected_trimmed,
        "\n\nActual VCode:\n{}\n\nExpected:\n{}\n",
        actual_str,
        expected
    );
}

/// Parse VCode from text format (for constructing test cases programmatically)
pub fn parse_vcode(text: &str) -> Result<VCode<Riscv32MachInst>, String> {
    vcode_parser::parse_vcode(text)
}

/// Test builder for lowering tests with a fluent API.
///
/// # Example
///
/// ```rust
/// LowerTest::from_lpir(r#"
/// function %test(i32, i32) -> i32 {
/// block0(v0: i32, v1: i32):
///     v2 = iadd v0, v1
///     return v2
/// }
/// "#)
/// .assert_vcode(r#"
/// vcode {
///   entry: block0
///
///   block0(v0, v1):
///     add v2, v0, v1
///
/// }
/// "#);
/// ```
pub struct LowerTest {
    vcode: VCode<Riscv32MachInst>,
}

impl LowerTest {
    /// Create a test from LPIR function text.
    ///
    /// Parses the function and lowers it to VCode using the RISC-V 32 backend.
    pub fn from_lpir(input_func: &str) -> Self {
        use lpc_lpir::parse_function;
        use crate::backend3::{lower::lower_function, vcode::Callee};
        use crate::isa::riscv32::backend3::{inst::Riscv32ABI, Riscv32LowerBackend};

        let func = parse_function(input_func.trim())
            .expect("Failed to parse input function");
        
        let backend = Riscv32LowerBackend;
        let abi = Callee { abi: Riscv32ABI };
        let vcode = lower_function(func, &backend, abi);
        
        LowerTest { vcode }
    }

    /// Assert that the VCode matches the expected text format.
    pub fn assert_vcode(&self, expected: &str) {
        assert_vcode_eq(&self.vcode, expected);
    }

    /// Get a reference to the VCode (for additional assertions).
    pub fn vcode(&self) -> &VCode<Riscv32MachInst> {
        &self.vcode
    }
}

