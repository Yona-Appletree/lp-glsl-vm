//! Arithmetic instruction lowering.

use r5_ir::Value;
use riscv32_encoder::{Gpr, Inst as RiscvInst};

use super::types::LoweringError;
use crate::{emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation};

impl super::Lowerer {
    /// Lower iadd instruction.
    pub(super) fn lower_iadd(
        &mut self,
        code: &mut CodeBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
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

        // Load operands into registers
        let arg1_reg = if let Some(reg) = self.get_register(arg1, allocation) {
            reg
        } else {
            // Load spilled arg1 into result_reg or temp
            if result_reg == Gpr::T0 {
                // Can't use T0, use T1
                self.load_value_into_reg(code, arg1, Gpr::T1, allocation, frame_layout)?;
                Gpr::T1
            } else {
                self.load_value_into_reg(code, arg1, result_reg, allocation, frame_layout)?;
                result_reg
            }
        };

        let arg2_reg = if let Some(reg) = self.get_register(arg2, allocation) {
            reg
        } else {
            // Load spilled arg2 into a temp register
            let temp = if arg1_reg == Gpr::T0 {
                Gpr::T1
            } else {
                Gpr::T0
            };
            self.load_value_into_reg(code, arg2, temp, allocation, frame_layout)?;
            temp
        };

        // If arg1 is in result_reg, we can use it directly
        // Otherwise, move arg1 to result_reg first
        if arg1_reg != result_reg {
            code.emit(RiscvInst::Addi {
                rd: result_reg,
                rs1: arg1_reg,
                imm: 0, // Move
            });
        }

        // Add arg2 to result_reg
        code.emit(RiscvInst::Add {
            rd: result_reg,
            rs1: result_reg,
            rs2: arg2_reg,
        });
        Ok(())
    }

    /// Lower isub instruction.
    pub(super) fn lower_isub(
        &mut self,
        code: &mut CodeBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        // For result values, check if in allocation first
        // If not in allocation at all, return ValueNotAllocated
        // If in allocation but not in register (spilled), return ResultNotInRegister
        let result_reg = if let Some(reg) = self.get_register(result, allocation) {
            reg
        } else if allocation.value_to_slot.contains_key(&result) {
            // Result is spilled - result values should always be in registers
            return Err(LoweringError::ResultNotInRegister { value: result });
        } else {
            // Result is not in allocation at all
            return Err(LoweringError::ValueNotAllocated { value: result });
        };

        let arg1_reg = if let Some(reg) = self.get_register(arg1, allocation) {
            reg
        } else {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, arg1, temp, allocation, frame_layout)?;
            temp
        };
        let arg2_reg = if let Some(reg) = self.get_register(arg2, allocation) {
            reg
        } else {
            let temp = Gpr::T1;
            self.load_value_into_reg(code, arg2, temp, allocation, frame_layout)?;
            temp
        };

        code.emit(RiscvInst::Sub {
            rd: result_reg,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
        Ok(())
    }

    /// Lower imul instruction.
    pub(super) fn lower_imul(
        &mut self,
        code: &mut CodeBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        // For result values, check if in allocation first
        // If not in allocation at all, return ValueNotAllocated
        // If in allocation but not in register (spilled), return ResultNotInRegister
        let result_reg = if let Some(reg) = self.get_register(result, allocation) {
            reg
        } else if allocation.value_to_slot.contains_key(&result) {
            // Result is spilled - result values should always be in registers
            return Err(LoweringError::ResultNotInRegister { value: result });
        } else {
            // Result is not in allocation at all
            return Err(LoweringError::ValueNotAllocated { value: result });
        };

        let arg1_reg = if let Some(reg) = self.get_register(arg1, allocation) {
            reg
        } else {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, arg1, temp, allocation, frame_layout)?;
            temp
        };
        let arg2_reg = if let Some(reg) = self.get_register(arg2, allocation) {
            reg
        } else {
            let temp = Gpr::T1;
            self.load_value_into_reg(code, arg2, temp, allocation, frame_layout)?;
            temp
        };

        code.emit(RiscvInst::Mul {
            rd: result_reg,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use r5_ir::parse_function;

    use super::super::Lowerer;
    use crate::{
        abi::Abi, frame::FrameLayout, liveness::compute_liveness, regalloc::allocate_registers,
        spill_reload::create_spill_reload_plan,
    };

    #[test]
    fn test_lower_simple_add() {
        // Simple function: v0 = iconst 1; v1 = iconst 2; v2 = iadd v0, v1; return v2
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    return v2
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

        // Should have generated some code
        assert!(code.instruction_count() > 0);

        // Should have prologue, instructions, and epilogue
        let instructions = code.instructions();
        assert!(!instructions.is_empty());
    }
}
