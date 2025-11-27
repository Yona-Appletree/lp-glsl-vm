//! Constant instruction lowering.

use lpc_lpir::Value;

use super::{
    super::regalloc::RegisterAllocation,
    types::LoweringError,
};
use crate::{Gpr, Inst as RiscvInst};
use crate::inst_buffer::InstBuffer;

impl super::Lowerer {
    /// Lower iconst instruction.
    pub(super) fn lower_iconst(
        &mut self,
        code: &mut InstBuffer,
        result: Value,
        value: i64,
        allocation: &RegisterAllocation,
    ) -> Result<(), LoweringError> {
        // For result values, they must be in registers
        // If not in allocation at all, return ValueNotAllocated
        // If in allocation but not in register (spilled), return ResultNotInRegister
        let result_reg = if let Some(reg) = self.get_register(result, allocation) {
            reg
        } else if !allocation.value_to_reg.contains_key(&result)
            && !allocation.value_to_slot.contains_key(&result)
        {
            // Result is not in allocation at all
            return Err(LoweringError::ValueNotAllocated { value: result });
        } else {
            // Result is in allocation but not in register (spilled) - result values should always be in registers
            return Err(LoweringError::ResultNotInRegister { value: result });
        };

        // Handle large constants (require lui + addi)
        if value >= -(1 << 11) && value < (1 << 11) {
            // Small constant: addi rd, x0, imm
            code.emit(RiscvInst::Addi {
                rd: result_reg,
                rs1: Gpr::Zero,
                imm: value as i32,
            });
        } else {
            // Large constant: lui + addi
            let imm = value as u32;
            let lui_imm = (imm >> 12) & 0xfffff;
            let addi_imm = (imm & 0xfff) as i32;
            let final_lui_imm = if (addi_imm & 0x800) != 0 {
                // Sign extend: if addi_imm is negative, increment lui_imm
                lui_imm + 1
            } else {
                lui_imm
            };

            code.emit(RiscvInst::Lui {
                rd: result_reg,
                imm: final_lui_imm,
            });
            code.emit(RiscvInst::Addi {
                rd: result_reg,
                rs1: result_reg,
                imm: addi_imm,
            });
        }
        Ok(())
    }
}