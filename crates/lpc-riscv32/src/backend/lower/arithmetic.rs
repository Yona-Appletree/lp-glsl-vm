//! Lower arithmetic instructions.

use lpc_lpir::Value;

use super::Lowerer;
use crate::Gpr;

/// Lower IADD: result = arg1 + arg2
pub fn lower_iadd(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);

    lowerer
        .inst_buffer_mut()
        .push_add(result_reg, arg1_reg, arg2_reg);
}

/// Lower ISUB: result = arg1 - arg2
pub fn lower_isub(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);

    lowerer
        .inst_buffer_mut()
        .push_sub(result_reg, arg1_reg, arg2_reg);
}

/// Lower IMUL: result = arg1 * arg2
pub fn lower_imul(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);

    lowerer.inst_buffer_mut().emit(crate::Inst::Mul {
        rd: result_reg,
        rs1: arg1_reg,
        rs2: arg2_reg,
    });
}

/// Lower IDIV: result = arg1 / arg2 (signed division)
pub fn lower_idiv(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);

    lowerer.inst_buffer_mut().emit(crate::Inst::Div {
        rd: result_reg,
        rs1: arg1_reg,
        rs2: arg2_reg,
    });
}

/// Lower IREM: result = arg1 % arg2 (signed remainder)
pub fn lower_irem(lowerer: &mut Lowerer, result: Value, arg1: Value, arg2: Value) {
    let result_reg = lowerer.get_reg_for_value(result);
    let arg1_reg = lowerer.get_reg_for_value_required(arg1);
    let arg2_reg = lowerer.get_reg_for_value_required(arg2);

    lowerer.inst_buffer_mut().emit(crate::Inst::Rem {
        rd: result_reg,
        rs1: arg1_reg,
        rs2: arg2_reg,
    });
}
