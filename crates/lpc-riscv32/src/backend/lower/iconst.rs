//! Constant instruction lowering.

use lpc_lpir::Value;
use crate::{Gpr, Inst as RiscvInst};

use super::types::LoweringError;
use super::super::{emit::CodeBuffer, regalloc::RegisterAllocation};

impl super::Lowerer {
    /// Lower iconst instruction.
    pub(super) fn lower_iconst(
        &mut self,
        code: &mut CodeBuffer,
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

#[cfg(test)]
mod tests {
    extern crate std;

    use lpc_lpir::parse_function;

    use crate::backend::Lowerer;
    use crate::backend::{
        Abi, FrameLayout, compute_liveness, allocate_registers, create_spill_reload_plan,
    };

    #[test]
    fn test_lower_iconst() {
        // Function with iconst
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 42
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        assert!(code.instruction_count().as_usize() > 0);
    }
}
