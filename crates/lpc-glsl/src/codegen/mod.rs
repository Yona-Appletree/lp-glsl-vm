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
pub use r#loop::{LoopInfo, LoopStack};
pub use scope::{Scope, ScopeGuard, ScopeStack};
pub use ssa::SSABuilder;
pub use value::{GlslLValue, GlslRValue, GlslValue};
