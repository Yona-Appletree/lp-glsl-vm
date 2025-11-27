//! Lower comparison instructions.

use crate::{Gpr, Inst};
use super::Lowerer;
use lpc_lpir::Value;

/// Lower ICMP_EQ: result = (arg1 == arg2) ? 1 : 0
pub fn lower_icmp_eq(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);
    
    // Use SUB to compare: if arg1 == arg2, then arg1 - arg2 == 0
    lowerer.inst_buffer_mut().push_sub(result_reg, arg1_reg, arg2_reg);
    // Now result_reg is 0 if equal, non-zero if not equal
    
    // Use SLTIU: if result_reg < 1 (i.e., == 0), then set to 1, else 0
    // SLTIU gives 1 if rs1 < imm (unsigned), 0 if rs1 >= imm
    // So if result_reg == 0, then result_reg < 1 is true, so result = 1
    // If result_reg != 0, then result_reg < 1 is false, so result = 0
    lowerer.inst_buffer_mut().emit(Inst::Sltiu {
        rd: result_reg,
        rs1: result_reg,
        imm: 1,
    });
}

/// Lower ICMP_NE: result = (arg1 != arg2) ? 1 : 0
pub fn lower_icmp_ne(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    // Similar to EQ, but invert the result
    lower_icmp_eq(lowerer, result, arg1, arg2);
    let result_reg = lowerer.get_reg_for_value_required(result);
    // XORI with 1 to invert: 0 -> 1, 1 -> 0
    lowerer.inst_buffer_mut().emit(Inst::Xori {
        rd: result_reg,
        rs1: result_reg,
        imm: 1,
    });
}

/// Lower ICMP_LT: result = (arg1 < arg2) ? 1 : 0 (signed)
pub fn lower_icmp_lt(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);
    
    lowerer.inst_buffer_mut().emit(Inst::Slt {
        rd: result_reg,
        rs1: arg1_reg,
        rs2: arg2_reg,
    });
}

/// Lower ICMP_LE: result = (arg1 <= arg2) ? 1 : 0 (signed)
pub fn lower_icmp_le(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    // arg1 <= arg2 is equivalent to !(arg2 < arg1)
    // So compute arg2 < arg1, then invert
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);
    
    lowerer.inst_buffer_mut().emit(Inst::Slt {
        rd: result_reg,
        rs1: arg2_reg,
        rs2: arg1_reg, // Swapped: arg2 < arg1
    });
    // Invert: 0 -> 1, 1 -> 0
    lowerer.inst_buffer_mut().emit(Inst::Xori {
        rd: result_reg,
        rs1: result_reg,
        imm: 1,
    });
}

/// Lower ICMP_GT: result = (arg1 > arg2) ? 1 : 0 (signed)
pub fn lower_icmp_gt(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    // arg1 > arg2 is equivalent to arg2 < arg1
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);
    
    lowerer.inst_buffer_mut().emit(Inst::Slt {
        rd: result_reg,
        rs1: arg2_reg,
        rs2: arg1_reg, // Swapped: arg2 < arg1 means arg1 > arg2
    });
}

/// Lower ICMP_GE: result = (arg1 >= arg2) ? 1 : 0 (signed)
pub fn lower_icmp_ge(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    // arg1 >= arg2 is equivalent to !(arg1 < arg2)
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);
    
    lowerer.inst_buffer_mut().emit(Inst::Slt {
        rd: result_reg,
        rs1: arg1_reg,
        rs2: arg2_reg,
    });
    // Invert: 0 -> 1, 1 -> 0
    lowerer.inst_buffer_mut().emit(Inst::Xori {
        rd: result_reg,
        rs1: result_reg,
        imm: 1,
    });
}
