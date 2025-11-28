//! Converter from old Inst enum to new InstData structure.

use crate::{
    dfg::{Immediate, InstData},
    entity::{Block, EntityRef},
    inst::Inst,
};

/// Convert an old Inst enum to new InstData
///
/// This is a compatibility layer for the parser during migration.
pub fn inst_to_inst_data(inst: Inst) -> InstData {
    match inst {
        Inst::Iadd { result, arg1, arg2 } => {
            InstData::arithmetic(crate::dfg::Opcode::Iadd, result, arg1, arg2)
        }
        Inst::Isub { result, arg1, arg2 } => {
            InstData::arithmetic(crate::dfg::Opcode::Isub, result, arg1, arg2)
        }
        Inst::Imul { result, arg1, arg2 } => {
            InstData::arithmetic(crate::dfg::Opcode::Imul, result, arg1, arg2)
        }
        Inst::Idiv { result, arg1, arg2 } => {
            InstData::arithmetic(crate::dfg::Opcode::Idiv, result, arg1, arg2)
        }
        Inst::Irem { result, arg1, arg2 } => {
            InstData::arithmetic(crate::dfg::Opcode::Irem, result, arg1, arg2)
        }

        Inst::Icmp {
            result,
            cond,
            arg1,
            arg2,
        } => InstData::comparison(crate::dfg::Opcode::Icmp { cond }, result, arg1, arg2),
        Inst::Fcmp {
            result,
            cond,
            arg1,
            arg2,
        } => InstData::comparison(crate::dfg::Opcode::Fcmp { cond }, result, arg1, arg2),

        Inst::Iconst { result, value } => InstData::constant(result, Immediate::I64(value)),
        Inst::Fconst { result, value_bits } => {
            InstData::constant(result, Immediate::F32Bits(value_bits))
        }

        Inst::Jump { target, args } => {
            // Convert usize index to Block entity
            // Note: This is a temporary solution - ideally the parser would work with Block entities directly
            let target_block = Block::from_index(target as usize);
            InstData::jump(target_block, args)
        }

        Inst::Br {
            condition,
            target_true,
            args_true,
            target_false,
            args_false,
        } => {
            let target_true_block = Block::from_index(target_true as usize);
            let target_false_block = Block::from_index(target_false as usize);
            InstData::branch(
                condition,
                target_true_block,
                args_true,
                target_false_block,
                args_false,
            )
        }

        Inst::Return { values } => InstData::return_(values),

        Inst::Call {
            callee,
            args,
            results,
        } => InstData::call(callee, args, results),

        Inst::Syscall { number, args } => InstData::syscall(number, args),

        Inst::Load {
            result,
            address,
            ty,
        } => InstData::load(result, address, ty),

        Inst::Store { address, value, ty } => InstData::store(address, value, ty),

        Inst::Halt => InstData::halt(),

        Inst::Trap { code } => InstData::trap(code),
        Inst::Trapz { condition, code } => InstData::trapz(condition, code),
        Inst::Trapnz { condition, code } => InstData::trapnz(condition, code),
    }
}
