//! Relocation handling
//!
//! Relocations are references to external symbols (functions, data, etc.) that need
//! to be resolved during code emission. This module provides utilities for recording
//! relocations during lowering.
//!
//! ## Relocation Lifecycle
//!
//! 1. **During Lowering**: Relocations are recorded in VCode when instructions that
//!    reference external symbols are lowered (e.g., function calls).
//!
//! 2. **During Emission**: Relocation positions are recorded in the emission state,
//!    and the actual addresses or offsets are computed.
//!
//! 3. **After Emission**: Relocations are resolved by patching the emitted code with
//!    the correct addresses or offsets.
//!
//! ## Relocation Types
//!
//! - **FunctionCall**: Direct call to a function (needs function address)
//! - **Branch**: Branch target (typically resolved during emission)
//!
//! Note: Currently, relocations are recorded but not automatically used during lowering.
//! This will be implemented in a future phase when function call lowering is enhanced.

use crate::backend3::{
    symbols::Symbol,
    types::InsnIndex,
    vcode::{RelocKind, VCodeReloc},
};

/// Record a relocation in VCode
///
/// This should be called during lowering when an instruction that requires
/// a relocation is created (e.g., a function call).
pub fn record_reloc(
    relocations: &mut alloc::vec::Vec<VCodeReloc>,
    inst_idx: InsnIndex,
    kind: RelocKind,
    target: Symbol,
) {
    relocations.push(VCodeReloc {
        inst_idx,
        kind,
        target,
    });
}
