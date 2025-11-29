//! Constant materialization

use lpc_lpir::RelSourceLoc;

use crate::backend3::{
    types::{VReg, Writable},
    vcode::{Constant, MachInst},
    vcode_builder::VCodeBuilder,
};

/// Materialize a constant value, choosing the appropriate strategy
///
/// Returns the VReg representing the constant value.
/// For large constants, this will emit LUI+ADDI instructions via the helpers.
pub fn materialize_constant<I: MachInst, FLui, FAddi>(
    vcode: &mut VCodeBuilder<I>,
    value: i32,
    srcloc: RelSourceLoc,
    create_lui: FLui,
    create_addi: FAddi,
) -> VReg
where
    FLui: FnOnce(Writable<VReg>, u32) -> I,
    FAddi: FnOnce(Writable<VReg>, VReg, i32) -> I,
{
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
        materialize_large_constant(vcode, value, srcloc, create_lui, create_addi)
    }
}

/// Determine if a constant fits in 12-bit signed immediate
fn fits_in_12_bits(value: i32) -> bool {
    value >= -2048 && value <= 2047
}

/// Materialize large constant via LUI + ADDI
fn materialize_large_constant<I: MachInst, FLui, FAddi>(
    vcode: &mut VCodeBuilder<I>,
    value: i32,
    srcloc: RelSourceLoc,
    create_lui: FLui,
    create_addi: FAddi,
) -> VReg
where
    FLui: FnOnce(Writable<VReg>, u32) -> I,
    FAddi: FnOnce(Writable<VReg>, VReg, i32) -> I,
{
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

    // Emit LUI: load upper 20 bits
    let temp_vreg = vcode.alloc_vreg();
    let lui_imm = (upper << 12) as u32;
    let lui_inst = create_lui(Writable::new(temp_vreg), lui_imm);
    vcode.push(lui_inst, srcloc);

    // Emit ADDI: add lower 12 bits (sign-extended)
    let addi_inst = create_addi(Writable::new(vreg), temp_vreg, lower_12 as i32);
    vcode.push(addi_inst, srcloc);

    vreg
}
