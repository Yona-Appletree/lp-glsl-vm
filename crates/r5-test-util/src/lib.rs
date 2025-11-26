//! Test utility for R5 compiler infrastructure.
//!
//! Provides a builder pattern for easily testing IR functions by compiling
//! them to ELF and running them in the embive VM.
//!
//! # Example
//!
//! ```rust
//! use r5_builder::FunctionBuilder;
//! use r5_ir::{Signature, Type};
//! use r5_test_util::R5FnTest;
//!
//! #[test]
//! fn test_add() {
//!     let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
//!     let mut builder = FunctionBuilder::new(sig);
//!     let block_idx = builder.create_block();
//!
//!     let a = builder.new_value();
//!     let b = builder.new_value();
//!     let result = builder.new_value();
//!
//!     {
//!         let mut block_builder = builder.block_builder(block_idx);
//!         block_builder.iadd(result, a, b);
//!         block_builder.return_(&vec![result]);
//!     }
//!
//!     let func = builder.finish();
//!
//!     R5FnTest::new(func)
//!         .with_args(&[5, 10])
//!         .expect_return(15)
//!         .run();
//! }
//! ```

mod r5_fn_test;
mod vm_runner;

pub use r5_fn_test::R5FnTest;
pub use vm_runner::VmRunner;
