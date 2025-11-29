//! Code generation module for GLSL frontend.
//!
//! This module provides the infrastructure for converting type-checked GLSL AST
//! to LPIR, including proper SSA construction, value representation, and scope management.

pub mod builder;
pub mod r#loop;
pub mod scope;
pub mod ssa;
pub mod value;

pub use builder::CodeGenBuilder;
pub use r#loop::LoopStack;
pub use scope::ScopeStack;
pub use ssa::SSABuilder;
// Value types are exported but unused - see docs/glsl/05-values.md for migration plan
#[allow(unused_imports)]
pub use value::{GlslLValue, GlslRValue, GlslValue};
