//! Backend3 tests

pub mod blockorder_edge_tests;
pub mod blockorder_tests;
pub mod cfg_patterns_tests;
pub mod clobber_tests;
pub mod constants_tests;
pub mod integration_tests;
pub mod lower_tests;
pub mod operand_collection_completeness_tests;
pub mod operand_tests;
pub mod regalloc_tests;
pub mod reloc_tests;
pub mod srcloc_tests;
pub mod vcode_invariants_tests;
pub mod vcode_tests;

#[cfg(test)]
mod vcode_test_helpers;
