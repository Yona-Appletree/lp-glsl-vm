//! GlslTest helper for integration tests.
//!
//! This module provides a convenient way to test the full GLSL compilation
//! pipeline: parse → type check → codegen → LPIR verification.

extern crate alloc;

use alloc::{string::String, vec::Vec};

use lpc_glsl::{parse_glsl, CodeGen, GlslError, GlslResult, GlslType, TypeChecker};
use lpc_lpir::{verify, Function, Signature, Type as LpirType, VerifierError};

/// Test helper for GLSL compilation and LPIR verification.
pub struct GlslTest {
    /// Generated functions: (function_name, Function)
    functions: Vec<(String, Function)>,
}

impl GlslTest {
    /// Parse, type check, and generate LPIR for GLSL source.
    ///
    /// # Errors
    ///
    /// Returns `Err` if parsing, type checking, or code generation fails.
    pub fn new(glsl: &str) -> Result<Self, GlslError> {
        // 1. Parse GLSL
        let function_infos = parse_glsl(glsl)?;

        // 2. Create TypeChecker and register all functions
        let mut checker = TypeChecker::new();
        checker.register_functions(&function_infos)?;

        // 3. Type check all function bodies
        for func_info in &function_infos {
            checker.type_check_function_body(&func_info.definition)?;
        }

        // 4. Generate LPIR for each function
        let mut functions = Vec::new();
        for func_info in &function_infos {
            // Build signature from function prototype
            let sig = Self::build_signature(&func_info.definition)?;

            // Create CodeGen
            let mut codegen = CodeGen::new(func_info.name.clone(), sig);
            codegen.generate_function(&func_info.definition, checker.symbols())?;

            // Store function
            let func = codegen.finish();

            // Validate generated LPIR
            if let Err(errors) = verify(&func, None) {
                let error_msgs: Vec<String> = errors
                    .iter()
                    .map(|e: &VerifierError| {
                        if let Some(loc) = &e.location {
                            format!("  {}: {}", loc, e.message)
                        } else {
                            format!("  {}", e.message)
                        }
                    })
                    .collect();
                return Err(GlslError::codegen(format!(
                    "LPIR validation failed for function '{}':\n{}",
                    func_info.name,
                    error_msgs.join("\n")
                )));
            }

            functions.push((func_info.name.clone(), func));
        }

        Ok(Self { functions })
    }

    /// Assert that a function generates the expected LPIR.
    ///
    /// # Panics
    ///
    /// Panics if the function is not found, if validation fails, or if the LPIR doesn't match.
    pub fn assert_lpir(&self, function_name: &str, expected_lpir: &str) {
        // Validate first
        self.validate_function(function_name);

        // Find function by name
        let (_, func) = self
            .functions
            .iter()
            .find(|(name, _)| name == function_name)
            .unwrap_or_else(|| {
                panic!(
                    "Function '{}' not found. Available functions: {:?}",
                    function_name,
                    self.functions.iter().map(|(n, _)| n).collect::<Vec<_>>()
                )
            });

        // Format Function using Display trait
        let actual_lpir = format!("{}", func);

        // Normalize both actual and expected
        let normalized_actual = Self::normalize_lpir(&actual_lpir);
        let normalized_expected = Self::normalize_lpir(expected_lpir);

        // Compare line-by-line
        if normalized_actual != normalized_expected {
            panic!(
                "LPIR mismatch for function '{}':\n\nExpected:\n{}\n\nActual:\n{}\n",
                function_name,
                normalized_expected.join("\n"),
                normalized_actual.join("\n")
            );
        }
    }

    /// Validate that a function is well-formed LPIR.
    ///
    /// # Panics
    ///
    /// Panics if validation fails, with detailed error messages.
    pub fn validate_function(&self, function_name: &str) {
        let func = self
            .get_function(function_name)
            .unwrap_or_else(|| panic!("Function '{}' not found", function_name));

        if let Err(errors) = verify(func, None) {
            let error_msgs: Vec<String> = errors
                .iter()
                .map(|e: &VerifierError| {
                    if let Some(loc) = &e.location {
                        format!("  {}: {}", loc, e.message)
                    } else {
                        format!("  {}", e.message)
                    }
                })
                .collect();
            panic!(
                "LPIR validation failed for function '{}':\n{}",
                function_name,
                error_msgs.join("\n")
            );
        }
    }

    /// Get the generated Function for a given function name.
    pub fn get_function(&self, function_name: &str) -> Option<&Function> {
        self.functions
            .iter()
            .find(|(name, _)| name == function_name)
            .map(|(_, func)| func)
    }

    /// Print the LPIR for a function (useful for debugging and generating expected output).
    pub fn print_lpir(&self, function_name: &str) {
        if let Some(func) = self.get_function(function_name) {
            println!("{}", func);
        } else {
            println!("Function '{}' not found", function_name);
        }
    }

    /// Normalize LPIR string for comparison.
    ///
    /// - Trim each line
    /// - Remove empty lines
    /// - Normalize whitespace
    fn normalize_lpir(lpir: &str) -> Vec<String> {
        lpir.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// Build LPIR Signature from function definition.
    fn build_signature(func_def: &glsl::syntax::FunctionDefinition) -> GlslResult<Signature> {
        // Use TypeChecker to extract function signature, then convert to LPIR Signature
        let func_sig = lpc_glsl::extract_function_signature(func_def)?;

        // Convert parameter types
        // For out/inout parameters, use I32 (address type) instead of the value type
        let param_types: Vec<LpirType> = func_sig
            .params
            .iter()
            .map(|p| {
                if p.qualifier.is_by_reference() {
                    // Out/inout parameters are passed as addresses (I32)
                    LpirType::I32
                } else {
                    // In parameters are passed by value
                    p.ty.to_lpir()
                }
            })
            .collect();

        // Convert return type
        let return_types: Vec<LpirType> = func_sig
            .return_type
            .map(|ty: GlslType| vec![ty.to_lpir()])
            .unwrap_or_default();

        Ok(Signature::new(param_types, return_types))
    }
}
