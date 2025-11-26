//! Comparison instruction lowering.

use r5_ir::Value;
use riscv32_encoder::{Gpr, Inst as RiscvInst};

use super::types::LoweringError;
use crate::{emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation};

impl super::Lowerer {
    /// Lower icmp_eq instruction: result = (arg1 == arg2) ? 1 : 0
    pub(super) fn lower_icmp_eq(
        &mut self,
        code: &mut CodeBuffer,
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
        code: &mut CodeBuffer,
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
            rs1: Gpr::ZERO,
            rs2: temp,
        });
        Ok(())
    }

    /// Lower icmp_lt instruction: result = (arg1 < arg2) ? 1 : 0 (signed)
    pub(super) fn lower_icmp_lt(
        &mut self,
        code: &mut CodeBuffer,
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
        code: &mut CodeBuffer,
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
        code: &mut CodeBuffer,
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
        code: &mut CodeBuffer,
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

#[cfg(test)]
mod tests {
    extern crate std;

    use crate::expect_ir_a0;

    #[test]
    fn test_icmp_le_true() {
        // Test: if n <= 1, return 42, else return 0
        // With n=0, should return 42
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 0
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 42
    v3 = iconst 0
    v4 = icmp_le v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 42);
    }

    #[test]
    fn test_icmp_le_false() {
        // Test: if n <= 1, return 42, else return 0
        // With n=10, should return 0
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 10
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 42
    v3 = iconst 0
    v4 = icmp_le v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 0);
    }

    #[test]
    fn test_icmp_le_equal() {
        // Test: if n <= 1, return 42, else return 0
        // With n=1, should return 42
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 1
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = iconst 42
    v3 = iconst 0
    v4 = icmp_le v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 42);
    }

    #[test]
    fn test_icmp_lt() {
        // Test: if n < 5, return 10, else return 20
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 3
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 5
    v2 = iconst 10
    v3 = iconst 20
    v4 = icmp_lt v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 10);
    }

    #[test]
    fn test_icmp_gt() {
        // Test: if n > 5, return 10, else return 20
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 7
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 5
    v2 = iconst 10
    v3 = iconst 20
    v4 = icmp_gt v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 10);
    }

    #[test]
    fn test_icmp_eq() {
        // Test: if n == 5, return 10, else return 20
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 5
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 5
    v2 = iconst 10
    v3 = iconst 20
    v4 = icmp_eq v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 10);
    }

    #[test]
    fn test_icmp_ne() {
        // Test: if n != 5, return 10, else return 20
        let ir = r#"
module {
entry: %bootstrap

function %bootstrap() -> i32 {
block0:
    v0 = iconst 7
    call %test(v0) -> v1
    halt
}

function %test(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 5
    v2 = iconst 10
    v3 = iconst 20
    v4 = icmp_ne v0, v1
    brif v4, block1, block2

block1:
    return v2

block2:
    return v3
}
}"#;

        expect_ir_a0(ir, 10);
    }
}
