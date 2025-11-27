//! Lower return instructions.

use super::Lowerer;
use lpc_lpir::Value;

/// Lower RETURN: return values
pub fn lower_return(lowerer: &mut Lowerer, values: &[Value]) {
    // TODO: Implement return lowering with:
    // 1. Return value preparation
    // 2. Multi-return handling (store to return area)
    // 3. Epilogue generation (already done in lower_function)
    
    // For now, just generate a return instruction
    // JALR x0, x1, 0 (return to caller)
    lowerer.inst_buffer_mut().emit(crate::Inst::Jalr {
        rd: crate::Gpr::Zero,
        rs1: crate::Gpr::Ra,
        imm: 0,
    });
}
