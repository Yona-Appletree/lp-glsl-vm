//! Constant materialization

use lpc_lpir::RelSourceLoc;
use regalloc2::RegClass;

use crate::backend3::{
    types::{Reg, VReg, Writable},
    vcode::{Constant, MachInst},
    vcode_builder::VCodeBuilder,
};

/// Materialize a constant value, choosing the appropriate strategy
///
/// Returns the VReg representing the constant value.
/// For small constants (<12 bits), emits ADDI with zero_reg().
/// For large constants, emits LUI+ADDI instructions via the helpers.
pub fn materialize_constant<I: MachInst, FLui, FAddi, FZeroReg>(
    vcode: &mut VCodeBuilder<I>,
    value: i32,
    srcloc: RelSourceLoc,
    create_lui: FLui,
    create_addi: FAddi,
    zero_reg: FZeroReg,
) -> VReg
where
    FLui: FnOnce(Writable<Reg>, u32) -> I,
    FAddi: FnOnce(Writable<Reg>, Reg, i32) -> I,
    FZeroReg: FnOnce() -> Reg,
{
    if fits_in_12_bits(value) {
        // Strategy 1: Emit ADDI with zero_reg() for small constants
        // This ensures the VReg is properly defined (SSA requirement)
        let vreg = vcode.alloc_vreg(RegClass::Int);
        let rd = Writable::new(Reg::from_virtual_reg(vreg));
        let rs1 = zero_reg();
        let addi_inst = create_addi(rd, rs1, value);
        vcode.push(addi_inst, srcloc);
        vreg
    } else {
        // Strategy 2: LUI + ADDI sequence
        materialize_large_constant(vcode, value, srcloc, create_lui, create_addi)
    }
}

/// Determine if a constant fits in 12-bit signed immediate
pub(crate) fn fits_in_12_bits(value: i32) -> bool {
    value >= -2048 && value <= 2047
}

/// Materialize large constant via LUI + ADDI
///
/// This implements the standard RISC-V constant materialization algorithm:
/// 1. Split the 32-bit value into upper 20 bits and lower 12 bits
/// 2. If the lower 12 bits have the sign bit set (bit 11), increment the upper bits
///    because ADDI sign-extends its immediate operand
/// 3. Emit LUI to load the upper 20 bits (shifted left by 12)
/// 4. Emit ADDI to add the lower 12 bits (which will be sign-extended)
///
/// Example: For value 0x12345800:
/// - lower_12 = 0x800 (sign bit set)
/// - upper_20 = 0x12345, adjusted to 0x12346
/// - LUI loads 0x12346000
/// - ADDI adds 0x800 (sign-extended to 0xFFFFF800)
/// - Result: 0x12346000 + 0xFFFFF800 = 0x12345800 ✓
fn materialize_large_constant<I: MachInst, FLui, FAddi>(
    vcode: &mut VCodeBuilder<I>,
    value: i32,
    srcloc: RelSourceLoc,
    create_lui: FLui,
    create_addi: FAddi,
) -> VReg
where
    FLui: FnOnce(Writable<Reg>, u32) -> I,
    FAddi: FnOnce(Writable<Reg>, Reg, i32) -> I,
{
    let vreg = vcode.alloc_vreg(RegClass::Int);

    // Split value into upper 20 bits and lower 12 bits
    // Note: RISC-V sign-extends the lower 12 bits in ADDI, so we need to handle sign correctly
    let lower_12 = value & 0xFFF;
    let upper_20 = (value >> 12) & 0xFFFFF;

    // If lower 12 bits have sign bit set (bit 11), we need to adjust upper
    // because ADDI sign-extends the immediate, effectively subtracting 0x1000
    // when bit 11 is set. To compensate, we increment the upper bits.
    let upper = if (lower_12 & 0x800) != 0 {
        // Sign bit set in lower, increment upper to compensate for sign extension
        (upper_20 + 1) & 0xFFFFF
    } else {
        upper_20
    };

    // Emit LUI: load upper 20 bits (shifted left by 12)
    let temp_vreg = vcode.alloc_vreg(RegClass::Int);
    let temp_reg = Reg::from_virtual_reg(temp_vreg);
    let lui_imm = (upper << 12) as u32;
    let lui_inst = create_lui(Writable::new(temp_reg), lui_imm);
    vcode.push(lui_inst, srcloc);

    // Emit ADDI: add lower 12 bits (sign-extended by the instruction)
    let result_reg = Reg::from_virtual_reg(vreg);
    let addi_inst = create_addi(Writable::new(result_reg), temp_reg, lower_12 as i32);
    vcode.push(addi_inst, srcloc);

    vreg
}
