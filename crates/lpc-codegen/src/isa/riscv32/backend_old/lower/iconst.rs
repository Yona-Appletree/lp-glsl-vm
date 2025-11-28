//! Lower integer constant instructions.

use lpc_lpir::Value;

use super::Lowerer;
use crate::Gpr;

/// Lower ICONST: result = value
pub fn lower_iconst(lowerer: &mut Lowerer, result: Value, value: i64) {
    let result_reg = lowerer.get_reg_for_value(result);

    // Truncate to i32 for RV32
    let value_i32 = value as i32;

    // Check if value fits in 12-bit signed immediate
    if value_i32 >= -2048 && value_i32 <= 2047 {
        // Single addi instruction: addi rd, zero, imm
        lowerer
            .inst_buffer_mut()
            .push_addi(result_reg, Gpr::Zero, value_i32);
    } else {
        // Need to load constant with lui + addi
        // Extract upper 20 bits and lower 12 bits
        let upper = ((value_i32 as u32) >> 12) & 0xfffff;
        let lower = (value_i32 as u32) & 0xfff;

        // Sign-extend lower 12 bits if the upper bit is set
        let lower_signed = if (lower & 0x800) != 0 {
            // Sign-extend: set upper bits to 1
            lower | 0xfffff000
        } else {
            lower
        } as i32;

        // Load upper 20 bits
        lowerer.inst_buffer_mut().push_lui(result_reg, upper);

        // Add lower 12 bits (sign-extended)
        if lower_signed != 0 {
            lowerer
                .inst_buffer_mut()
                .push_addi(result_reg, result_reg, lower_signed);
        }
    }
}
