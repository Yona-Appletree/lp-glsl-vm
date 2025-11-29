//! Constant materialization

use crate::backend3::types::VReg;
use crate::backend3::vcode::{Constant, MachInst};
use crate::backend3::vcode_builder::VCodeBuilder;

/// Materialize a constant value, choosing the appropriate strategy
///
/// Returns the VReg representing the constant value.
pub fn materialize_constant<I: MachInst>(
    vcode: &mut VCodeBuilder<I>,
    value: i32,
) -> VReg {
    if fits_in_12_bits(value) {
        // Strategy 1: Inline immediate
        // The constant will be embedded directly in the instruction
        // during lowering (e.g., addi rd, rs1, value)
        // Return a VReg that represents this constant value
        let vreg = vcode.alloc_vreg();
        vcode.record_constant(vreg, Constant::Inline(value));
        vreg
    } else {
        // Strategy 2: LUI + ADDI sequence
        materialize_large_constant(vcode, value)
    }
}

/// Determine if a constant fits in 12-bit signed immediate
fn fits_in_12_bits(value: i32) -> bool {
    value >= -2048 && value <= 2047
}

/// Materialize large constant via LUI + ADDI
fn materialize_large_constant<I: MachInst>(
    vcode: &mut VCodeBuilder<I>,
    value: i32,
) -> VReg {
    let vreg = vcode.alloc_vreg();

    // Split value into upper 20 bits and lower 12 bits
    // Note: RISC-V sign-extends the lower 12 bits, so we need to handle sign correctly
    let lower_12 = value & 0xFFF;
    let upper_20 = (value >> 12) & 0xFFFFF;

    // If lower 12 bits have sign bit set (bit 11), we need to adjust upper
    // because addi sign-extends the immediate
    let upper = if (lower_12 & 0x800) != 0 {
        // Sign bit set in lower, increment upper
        (upper_20 + 1) & 0xFFFFF
    } else {
        upper_20
    };

    // Record constant for later emission
    // TODO: When we implement emission, we'll emit LUI + ADDI here
    // For now, just record it as a large constant
    vcode.record_constant(vreg, Constant::Large(value));

    vreg
}

