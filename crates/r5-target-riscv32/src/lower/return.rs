//! Return instruction lowering.

use riscv32_encoder::{Gpr, Inst as RiscvInst};

use crate::{
    abi::AbiInfo,
    emit::CodeBuffer,
    frame::FrameLayout,
    regalloc::RegisterAllocation,
};

use super::types::{LoweringError, Relocation, RelocationInstType, RelocationTarget};
use r5_ir::Value;

impl super::Lowerer {
    /// Lower return instruction.
    ///
    /// Moves return values to return registers and jumps to the epilogue.
    /// Emits a placeholder jal and records a relocation for fixup.
    pub(super) fn lower_return(
        &mut self,
        code: &mut CodeBuffer,
        values: &[Value],
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) -> Result<(), LoweringError> {
        // Move return values to return registers (first 8)
        for (idx, value) in values.iter().enumerate() {
            if let Some(return_reg) = abi_info.return_regs.get(&idx) {
                self.load_value_into_reg(code, *value, *return_reg, allocation, frame_layout)?;
            }
        }

        // Store stack return values (index >= 8) to stack
        // These are stored at positive offsets from SP (before epilogue)
        for (idx, value) in values.iter().enumerate() {
            if idx >= 8 {
                if let Some(stack_offset) = abi_info.return_stack_offsets.get(&idx) {
                    // Load value into temp register
                    let temp_reg = Gpr::T0;
                    self.load_value_into_reg(code, *value, temp_reg, allocation, frame_layout)?;

                    // Store to stack (offset relative to SP before epilogue)
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::SP,
                        rs2: temp_reg,
                        imm: *stack_offset, // Positive offset
                    });
                }
            }
        }

        // Emit placeholder jal instruction (offset 0, will be fixed up)
        let jal_inst_idx = code.instruction_count();
        code.emit(RiscvInst::Jal {
            rd: Gpr::ZERO,
            imm: 0, // Placeholder
        });

        // Record relocation for jal (epilogue target)
        self.function_relocations.push(Relocation {
            offset: jal_inst_idx,
            target: RelocationTarget::Epilogue,
            inst_type: RelocationInstType::Jal { rd: Gpr::ZERO },
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
        abi::Abi, frame::FrameLayout, liveness::compute_liveness,
        regalloc::allocate_registers, spill_reload::create_spill_reload_plan,
    };

    #[test]
    fn test_lower_return() {
        // Function that returns a value
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
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

        assert!(code.instruction_count() > 0);

        // Should end with jalr (return)
        let instructions = code.instructions();
        assert!(matches!(
            instructions.last(),
            Some(riscv32_encoder::Inst::Jalr { .. })
        ));
    }
}

