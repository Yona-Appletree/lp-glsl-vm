//! Comparison instruction lowering.

use lpc_lpir::Value;

use super::{
    super::{frame::FrameLayout, regalloc::RegisterAllocation},
    types::LoweringError,
};
use crate::{Gpr, Inst as RiscvInst};
use crate::inst_buffer::InstBuffer;

impl super::Lowerer {
    /// Lower icmp_eq instruction: result = (arg1 == arg2) ? 1 : 0
    pub(super) fn lower_icmp_eq(
        &mut self,
        code: &mut InstBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        let result_reg = self.get_result_reg(result, allocation)?;
        let arg1_reg = self.get_arg_reg(code, arg1, allocation, frame_layout, Gpr::T0)?;
        let arg2_reg = self.get_arg_reg(code, arg2, allocation, frame_layout, Gpr::T1)?;

        // Use: sub temp, arg1, arg2; sltiu result, temp, 1
        // If arg1 == arg2, then temp == 0, so (temp < 1) = 1
        let temp = Gpr::T2;
        code.emit(RiscvInst::Sub {
            rd: temp,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
        code.emit(RiscvInst::Sltiu {
            rd: result_reg,
            rs1: temp,
            imm: 1,
        });
        Ok(())
    }

    /// Lower icmp_ne instruction: result = (arg1 != arg2) ? 1 : 0
    pub(super) fn lower_icmp_ne(
        &mut self,
        code: &mut InstBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        let result_reg = self.get_result_reg(result, allocation)?;
        let arg1_reg = self.get_arg_reg(code, arg1, allocation, frame_layout, Gpr::T0)?;
        let arg2_reg = self.get_arg_reg(code, arg2, allocation, frame_layout, Gpr::T1)?;

        // Use: sub temp, arg1, arg2; sltu result, x0, temp
        // If arg1 != arg2, then temp != 0, so (0 < temp) = 1
        let temp = Gpr::T2;
        code.emit(RiscvInst::Sub {
            rd: temp,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
        code.emit(RiscvInst::Sltu {
            rd: result_reg,
            rs1: Gpr::Zero,
            rs2: temp,
        });
        Ok(())
    }

    /// Lower icmp_lt instruction: result = (arg1 < arg2) ? 1 : 0 (signed)
    pub(super) fn lower_icmp_lt(
        &mut self,
        code: &mut InstBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        let result_reg = self.get_result_reg(result, allocation)?;
        let arg1_reg = self.get_arg_reg(code, arg1, allocation, frame_layout, Gpr::T0)?;
        let arg2_reg = self.get_arg_reg(code, arg2, allocation, frame_layout, Gpr::T1)?;

        code.emit(RiscvInst::Slt {
            rd: result_reg,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
        Ok(())
    }

    /// Lower icmp_le instruction: result = (arg1 <= arg2) ? 1 : 0 (signed)
    pub(super) fn lower_icmp_le(
        &mut self,
        code: &mut InstBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        let result_reg = self.get_result_reg(result, allocation)?;
        let arg1_reg = self.get_arg_reg(code, arg1, allocation, frame_layout, Gpr::T0)?;
        let arg2_reg = self.get_arg_reg(code, arg2, allocation, frame_layout, Gpr::T1)?;

        // arg1 <= arg2 is equivalent to !(arg2 < arg1)
        // Use: slt temp, arg2, arg1; xori result, temp, 1
        let temp = Gpr::T2;
        code.emit(RiscvInst::Slt {
            rd: temp,
            rs1: arg2_reg,
            rs2: arg1_reg,
        });
        code.emit(RiscvInst::Xori {
            rd: result_reg,
            rs1: temp,
            imm: 1,
        });
        Ok(())
    }

    /// Lower icmp_gt instruction: result = (arg1 > arg2) ? 1 : 0 (signed)
    pub(super) fn lower_icmp_gt(
        &mut self,
        code: &mut InstBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        let result_reg = self.get_result_reg(result, allocation)?;
        let arg1_reg = self.get_arg_reg(code, arg1, allocation, frame_layout, Gpr::T0)?;
        let arg2_reg = self.get_arg_reg(code, arg2, allocation, frame_layout, Gpr::T1)?;

        // arg1 > arg2 is equivalent to arg2 < arg1
        code.emit(RiscvInst::Slt {
            rd: result_reg,
            rs1: arg2_reg,
            rs2: arg1_reg,
        });
        Ok(())
    }

    /// Lower icmp_ge instruction: result = (arg1 >= arg2) ? 1 : 0 (signed)
    pub(super) fn lower_icmp_ge(
        &mut self,
        code: &mut InstBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        let result_reg = self.get_result_reg(result, allocation)?;
        let arg1_reg = self.get_arg_reg(code, arg1, allocation, frame_layout, Gpr::T0)?;
        let arg2_reg = self.get_arg_reg(code, arg2, allocation, frame_layout, Gpr::T1)?;

        // arg1 >= arg2 is equivalent to !(arg1 < arg2)
        // Use: slt temp, arg1, arg2; xori result, temp, 1
        let temp = Gpr::T2;
        code.emit(RiscvInst::Slt {
            rd: temp,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
        code.emit(RiscvInst::Xori {
            rd: result_reg,
            rs1: temp,
            imm: 1,
        });
        Ok(())
    }
}