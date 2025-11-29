//! File-based tests for LPIR.
//!
//! Similar to Cranelift's filetests, these tests read `.lpir` files that contain:
//! - Test commands (e.g., `test cat`, `test verifier`, `test domtree`, etc.)
//! - Functions to test
//! - Expected output or annotations in comments

pub mod filecheck;
pub mod parser;

mod test_cat;
mod test_cfg;
mod test_compile;
mod test_domtree;
mod test_transform;
mod test_verifier;

pub use filecheck::{parse_filecheck_directives, match_filecheck};
pub use parser::{parse_test_file, TestCase};

