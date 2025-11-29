//! Analysis modules for IR functions.
//!
//! This module provides control flow graph and dominance analysis
//! capabilities for validating and analyzing IR functions.

pub mod cfg;
pub mod dominance;

pub use cfg::ControlFlowGraph;
pub use dominance::DominatorTree;
