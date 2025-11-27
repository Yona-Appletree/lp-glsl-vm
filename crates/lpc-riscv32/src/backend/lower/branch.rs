//! Branch instruction lowering.

use lpc_lpir::Value;

use super::{
    super::{emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation},
    types::{LoweringError, Relocation, RelocationInstType, RelocationTarget},
};
use crate::{Gpr, Inst as RiscvInst};

impl super::Lowerer {
    /// Lower branch instruction.
    ///
    /// Emits placeholder instructions and records relocations for fixup.
    pub(super) fn lower_br(
        &mut self,
        code: &mut CodeBuffer,
        condition: Value,
        target_true: u32,
        target_false: u32,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        // Load condition into a register
        let cond_reg = if let Some(reg) = self.get_register(condition, allocation) {
            reg
        } else {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, condition, temp, allocation, frame_layout)?;
            temp
        };

        // Emit placeholder beq instruction (offset 0, will be fixed up)
        let beq_inst_idx = code.instruction_count();
        code.emit(RiscvInst::Beq {
            rs1: cond_reg,
            rs2: Gpr::Zero,
            imm: 0, // Placeholder
        });

        // Record relocation for beq (false target)
        self.function_relocations.push(Relocation {
            offset: beq_inst_idx,
            target: RelocationTarget::Block(target_false as usize),
            inst_type: RelocationInstType::Beq {
                rs1: cond_reg,
                rs2: Gpr::Zero,
            },
        });

        // Emit placeholder jal instruction (offset 0, will be fixed up)
        let jal_inst_idx = code.instruction_count();
        code.emit(RiscvInst::Jal {
            rd: Gpr::Zero,
            imm: 0, // Placeholder
        });

        // Record relocation for jal (true target)
        self.function_relocations.push(Relocation {
            offset: jal_inst_idx,
            target: RelocationTarget::Block(target_true as usize),
            inst_type: RelocationInstType::Jal { rd: Gpr::Zero },
        });

        Ok(())
    }
}
