//! IR verifier.

use alloc::{format, string::String, vec::Vec};

use crate::Function;

mod cfg;
mod dominance;
mod ssa;
mod types;

pub use cfg::verify_cfg;
pub use dominance::verify_dominance;
pub use ssa::verify_ssa;
pub use types::verify_types;

/// Verifier error
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierError {
    /// Error message describing what's wrong
    pub message: String,
    /// Optional location information (e.g., "block0", "inst5")
    pub location: Option<String>,
}

impl VerifierError {
    /// Create a new verifier error
    pub fn new(message: String) -> Self {
        Self {
            message,
            location: None,
        }
    }

    /// Create a new verifier error with location
    pub fn with_location(message: String, location: String) -> Self {
        Self {
            message,
            location: Some(location),
        }
    }
}

/// Verify a function is well-formed
///
/// This runs all verification checks and returns a list of errors if any are found.
/// Returns `Ok(())` if the function is valid, or `Err(errors)` with a list of
/// verification errors.
pub fn verify(function: &Function) -> Result<(), Vec<VerifierError>> {
    let mut errors = Vec::new();

    // Run all checks
    verify_cfg(function, &mut errors);
    verify_ssa(function, &mut errors);
    verify_dominance(function, &mut errors);
    verify_types(function, &mut errors);
    verify_terminators(function, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Verify that all blocks have proper terminators
fn verify_terminators(function: &Function, errors: &mut Vec<VerifierError>) {
    for block in function.blocks() {
        let has_terminator = function
            .block_insts(block)
            .any(|inst| {
                if let Some(inst_data) = function.dfg.inst_data(inst) {
                    matches!(
                        inst_data.opcode,
                        crate::dfg::Opcode::Jump
                            | crate::dfg::Opcode::Br
                            | crate::dfg::Opcode::Return
                            | crate::dfg::Opcode::Halt
                    )
                } else {
                    false
                }
            });

        if !has_terminator {
            errors.push(VerifierError::with_location(
                format!("Block {} has no terminator", block),
                format!("block{}", block.index()),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dfg::InstData;
    use crate::signature::Signature;

    #[test]
    fn test_verify_valid_function() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        let inst_data = InstData::halt();
        let inst = func.create_inst(inst_data);
        func.append_inst(inst, block);

        let result = verify(&func);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_missing_terminator() {
        let sig = Signature::empty();
        let mut func = Function::new(sig, String::from("test"));
        let block = func.create_block();
        func.append_block(block);

        // Block has no instructions, so no terminator
        let result = verify(&func);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("no terminator"));
    }

    #[test]
    fn test_verifier_error_creation() {
        let error = VerifierError::new(String::from("test error"));
        assert_eq!(error.message, "test error");
        assert_eq!(error.location, None);

        let error_with_loc = VerifierError::with_location(
            String::from("test error"),
            String::from("block0"),
        );
        assert_eq!(error_with_loc.message, "test error");
        assert_eq!(error_with_loc.location, Some(String::from("block0")));
    }
}

