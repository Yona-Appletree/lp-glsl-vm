//! Helper methods for instruction lowering.

use lpc_lpir::Value;

use super::{
    super::{
        emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation,
        spill_reload::SpillReloadOp,
    },
    types::LoweringError,
};
use crate::{Gpr, Inst as RiscvInst};

/// Helper methods for Lowerer.
impl super::Lowerer {
    /// Emit a spill or reload operation.
    pub(super) fn emit_spill_reload(
        &mut self,
        code: &mut CodeBuffer,
        op: &SpillReloadOp,
        frame_layout: &FrameLayout,
    ) {
        match op {
            SpillReloadOp::Spill { reg, slot, .. } => {
                let offset = frame_layout.spill_slot_offset(*slot);
                code.emit(RiscvInst::Sw {
                    rs1: Gpr::Sp,
                    rs2: *reg,
                    imm: offset.as_i32(),
                });
            }
            SpillReloadOp::Reload { reg, slot, .. } => {
                let offset = frame_layout.spill_slot_offset(*slot);
                code.emit(RiscvInst::Lw {
                    rd: *reg,
                    rs1: Gpr::Sp,
                    imm: offset.as_i32(),
                });
            }
        }
    }

    /// Get register for a value, or None if spilled.
    pub(super) fn get_register(
        &self,
        value: Value,
        allocation: &RegisterAllocation,
    ) -> Option<Gpr> {
        allocation.value_to_reg.get(&value).copied()
    }

    /// Get spill slot for a value, or None if in register.
    pub(super) fn get_spill_slot(
        &self,
        value: Value,
        allocation: &RegisterAllocation,
    ) -> Option<u32> {
        allocation.value_to_slot.get(&value).copied()
    }

    /// Load a value into a register (handles spills).
    pub(super) fn load_value_into_reg(
        &mut self,
        code: &mut CodeBuffer,
        value: Value,
        target_reg: Gpr,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        if let Some(reg) = self.get_register(value, allocation) {
            // Value is in a register - move it if needed
            #[cfg(feature = "debug-lowering")]
            crate::debug_lowering!(
                "load_value_into_reg: {:?} -> {:?} (from register {:?})",
                value,
                target_reg,
                reg
            );
            if reg != target_reg {
                code.emit(RiscvInst::Addi {
                    rd: target_reg,
                    rs1: reg,
                    imm: 0, // Move: addi rd, rs, 0
                });
            }
            Ok(())
        } else if let Some(slot) = self.get_spill_slot(value, allocation) {
            // Value is spilled - reload it
            let offset = frame_layout.spill_slot_offset(slot);
            #[cfg(feature = "debug-lowering")]
            crate::debug_lowering!(
                "load_value_into_reg: {:?} -> {:?} (from spill slot {}, offset={})",
                value,
                target_reg,
                slot,
                offset.as_i32()
            );
            code.emit(RiscvInst::Lw {
                rd: target_reg,
                rs1: Gpr::Sp,
                imm: offset.as_i32(),
            });
            Ok(())
        } else {
            // Value not found - this shouldn't happen with correct allocation
            Err(LoweringError::ValueNotAllocated { value })
        }
    }

    /// Helper: Get result register, ensuring it's allocated.
    pub(super) fn get_result_reg(
        &self,
        result: Value,
        allocation: &RegisterAllocation,
    ) -> Result<Gpr, LoweringError> {
        if let Some(reg) = self.get_register(result, allocation) {
            Ok(reg)
        } else if allocation.value_to_slot.contains_key(&result) {
            Err(LoweringError::ResultNotInRegister { value: result })
        } else {
            Err(LoweringError::ValueNotAllocated { value: result })
        }
    }

    /// Helper: Get argument register, loading from spill slot if needed.
    pub(super) fn get_arg_reg(
        &mut self,
        code: &mut CodeBuffer,
        arg: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        temp: Gpr,
    ) -> Result<Gpr, LoweringError> {
        if let Some(reg) = self.get_register(arg, allocation) {
            Ok(reg)
        } else {
            self.load_value_into_reg(code, arg, temp, allocation, frame_layout)?;
            Ok(temp)
        }
    }
}
