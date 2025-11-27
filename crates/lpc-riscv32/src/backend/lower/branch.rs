//! Lower control flow instructions.

use crate::Gpr;
use super::Lowerer;

/// Lower JUMP: jump to target block
pub fn lower_jump(lowerer: &mut Lowerer, target: u32) {
    // TODO: Compute actual block address/offset
    // For now, use a placeholder offset
    // JAL with rd=x0 (discard return address) for unconditional jump
    lowerer.inst_buffer_mut().emit(crate::Inst::Jal {
        rd: Gpr::Zero,
        imm: 0, // TODO: Compute actual offset
    });
}

/// Lower BR: if condition, jump to target_true, else target_false
pub fn lower_br(lowerer: &mut Lowerer, condition: lpc_lpir::Value, target_true: u32, target_false: u32) {
    let condition_reg = lowerer.get_reg_for_value_required(condition);
    
    // Compare condition with zero: if condition != 0, jump to target_true
    // Use BNE: if condition_reg != zero, jump to target_true
    // TODO: Compute actual block offsets
    lowerer.inst_buffer_mut().emit(crate::Inst::Bne {
        rs1: condition_reg,
        rs2: Gpr::Zero,
        imm: 0, // TODO: Compute offset to target_true
    });
    
    // Fall through to target_false (or jump to it)
    // TODO: Handle target_false
}
