//! Helper functions for lowering.

use lpc_lpir::Value;

use super::Lowerer;
use crate::{Gpr, Inst};

/// Lower a load instruction.
pub fn lower_load(lowerer: &mut Lowerer, result: Value, address: Value) {
    let result_reg = lowerer.get_reg_for_value(result);
    let address_reg = lowerer.get_reg_for_value_required(address);

    // Load word: lw rd, 0(rs1)
    lowerer
        .inst_buffer_mut()
        .push_lw(result_reg, address_reg, 0);
}

/// Lower a store instruction.
pub fn lower_store(lowerer: &mut Lowerer, address: Value, value: Value) {
    let address_reg = lowerer.get_reg_for_value_required(address);
    let value_reg = lowerer.get_reg_for_value_required(value);

    // Store word: sw rs2, 0(rs1)
    lowerer.inst_buffer_mut().push_sw(address_reg, value_reg, 0);
}

/// Lower a syscall instruction.
pub fn lower_syscall(lowerer: &mut Lowerer, number: i32, args: &[Value]) {
    // Load syscall number into a7
    // TODO: Handle large constants properly
    lowerer
        .inst_buffer_mut()
        .push_addi(Gpr::A7, Gpr::Zero, number);

    // Load arguments into a0-a6
    for (i, arg) in args.iter().take(7).enumerate() {
        let arg_reg = lowerer.get_reg_for_value_required(*arg);
        let target_reg = match i {
            0 => Gpr::A0,
            1 => Gpr::A1,
            2 => Gpr::A2,
            3 => Gpr::A3,
            4 => Gpr::A4,
            5 => Gpr::A5,
            6 => Gpr::A6,
            _ => unreachable!(),
        };

        if arg_reg != target_reg {
            lowerer
                .inst_buffer_mut()
                .push_add(target_reg, arg_reg, Gpr::Zero);
        }
    }

    // Execute syscall: ecall
    lowerer.inst_buffer_mut().emit(Inst::Ecall);
}

/// Lower a halt instruction (ebreak).
pub fn lower_halt(lowerer: &mut Lowerer) {
    lowerer.inst_buffer_mut().emit(Inst::Ebreak);
}
