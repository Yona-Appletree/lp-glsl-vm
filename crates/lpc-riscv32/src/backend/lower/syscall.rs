//! Syscall instruction lowering.

use lpc_lpir::Value;

use super::{
    super::{emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation},
    types::LoweringError,
};
use crate::{Gpr, Inst as RiscvInst};

impl super::Lowerer {
    /// Lower syscall instruction.
    pub(super) fn lower_syscall(
        &mut self,
        code: &mut CodeBuffer,
        number: i32,
        args: &[Value],
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        // Move arguments to a0-a7 registers
        for (idx, arg) in args.iter().enumerate() {
            if idx < 8 {
                let arg_reg = Gpr::new(10 + idx as u8); // a0-a7
                self.load_value_into_reg(code, *arg, arg_reg, allocation, frame_layout)?;
            }
            // Known limitation: Syscalls with > 8 arguments are not yet supported
            // (stack arguments for syscalls would need additional implementation)
        }

        // Move syscall number to a7 (last argument register)
        if number < (1 << 12) {
            // Small immediate: addi a7, zero, number
            code.emit(RiscvInst::Addi {
                rd: Gpr::A7,
                rs1: Gpr::Zero,
                imm: number,
            });
        } else {
            // Large immediate: lui + addi
            let imm = number as u32;
            let lui_imm = (imm >> 12) & 0xfffff;
            let addi_imm = (imm & 0xfff) as i32;
            code.emit(RiscvInst::Lui {
                rd: Gpr::A7,
                imm: lui_imm,
            });
            code.emit(RiscvInst::Addi {
                rd: Gpr::A7,
                rs1: Gpr::A7,
                imm: addi_imm,
            });
        }

        // Emit ecall
        code.emit(RiscvInst::Ecall);
        Ok(())
    }
}
