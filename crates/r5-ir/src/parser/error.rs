//! Parse error types.

use alloc::string::String;
use core::fmt;

/// Parse error with position information.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Parse error at position {}: {}",
            self.position, self.message
        )
    }
}

impl core::error::Error for ParseError {}

pub(crate) fn parse_error(original_input: &str, remaining_input: &str, message: &str) -> ParseError {
    ParseError {
        message: alloc::string::ToString::to_string(message),
        position: original_input.len() - remaining_input.len(),
    }
}
