//! Instruction executor for RISC-V 32-bit instructions.

use super::{
    error::EmulatorError,
    logging::{InstLog, SystemKind},
    memory::Memory,
};
use crate::{Gpr, Inst};

/// Result of executing a single instruction.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// New PC value (None means PC += 4)
    pub new_pc: Option<u32>,
    /// Whether execution should stop (EBREAK)
    pub should_halt: bool,
    /// Whether a syscall was encountered (ECALL)
    pub syscall: bool,
    /// Log entry for this instruction
    pub log: InstLog,
}

/// Helper to read register (x0 always returns 0)
fn read_reg(regs: &[i32; 32], reg: Gpr) -> i32 {
    if reg.num() == 0 {
        0
    } else {
        regs[reg.num() as usize]
    }
}

/// Execute a decoded instruction.
pub fn execute_instruction(
    inst: Inst,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let mut new_pc: Option<u32> = None;
    let mut should_halt = false;
    let mut syscall = false;
    let instruction_word = inst.encode();

    let log = match inst {
        Inst::Add { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1.wrapping_add(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            InstLog::Arithmetic {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: val1,
                rs2_val: Some(val2),
                rd_old,
                rd_new: result,
            }
        }
        Inst::Sub { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1.wrapping_sub(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            InstLog::Arithmetic {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: val1,
                rs2_val: Some(val2),
                rd_old,
                rd_new: result,
            }
        }
        Inst::Mul { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1.wrapping_mul(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            InstLog::Arithmetic {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: val1,
                rs2_val: Some(val2),
                rd_old,
                rd_new: result,
            }
        }
        Inst::Addi { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = val1.wrapping_add(imm);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            InstLog::Arithmetic {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: val1,
                rs2_val: None,
                rd_old,
                rd_new: result,
            }
        }
        Inst::Lw { rd, rs1, imm } => {
            let base = read_reg(regs, rs1);
            let address = base.wrapping_add(imm) as u32;

            // Save register state for error context
            let error_regs = *regs;
            let value = memory.read_word(address).map_err(|mut e| {
                match &mut e {
                    EmulatorError::InvalidMemoryAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = error_regs;
                        *err_pc = pc;
                    }
                    EmulatorError::UnalignedAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = error_regs;
                        *err_pc = pc;
                    }
                    _ => {}
                }
                e
            })?;

            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            InstLog::Load {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: base,
                addr: address,
                mem_val: value,
                rd_old,
                rd_new: value,
            }
        }
        Inst::Sw { rs1, rs2, imm } => {
            let base = read_reg(regs, rs1);
            let value = read_reg(regs, rs2);
            let address = base.wrapping_add(imm) as u32;

            // Read old value before write
            let old_value = memory.read_word(address).unwrap_or(0);

            // Save register state for error context
            let error_regs = *regs;
            memory.write_word(address, value).map_err(|mut e| {
                match &mut e {
                    EmulatorError::InvalidMemoryAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = error_regs;
                        *err_pc = pc;
                    }
                    EmulatorError::UnalignedAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = error_regs;
                        *err_pc = pc;
                    }
                    _ => {}
                }
                e
            })?;

            InstLog::Store {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rs1_val: base,
                rs2_val: value,
                addr: address,
                mem_old: old_value,
                mem_new: value,
            }
        }
        Inst::Jal { rd, imm } => {
            let next_pc = pc.wrapping_add(4);
            let rd_old = read_reg(regs, rd);
            let target = pc.wrapping_add(imm as u32);
            if rd.num() != 0 {
                regs[rd.num() as usize] = next_pc as i32;
            }
            new_pc = Some(target);

            InstLog::Jump {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rd_old,
                rd_new: if rd.num() == 0 {
                    None
                } else {
                    Some(next_pc as i32)
                },
                target_pc: target,
            }
        }
        Inst::Jalr { rd, rs1, imm } => {
            let base = read_reg(regs, rs1);
            let next_pc = pc.wrapping_add(4);
            let rd_old = read_reg(regs, rd);
            let target = (base.wrapping_add(imm) as u32) & !1; // Clear LSB
            if rd.num() != 0 {
                regs[rd.num() as usize] = next_pc as i32;
            }
            new_pc = Some(target);

            InstLog::Jump {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rd_old,
                rd_new: if rd.num() == 0 {
                    None
                } else {
                    Some(next_pc as i32)
                },
                target_pc: target,
            }
        }
        Inst::Beq { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let taken = val1 == val2;
            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };

            InstLog::Branch {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rs1_val: val1,
                rs2_val: val2,
                taken,
                target_pc,
            }
        }
        Inst::Bne { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let taken = val1 != val2;
            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };

            InstLog::Branch {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rs1_val: val1,
                rs2_val: val2,
                taken,
                target_pc,
            }
        }
        Inst::Blt { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let taken = val1 < val2;
            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };

            InstLog::Branch {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rs1_val: val1,
                rs2_val: val2,
                taken,
                target_pc,
            }
        }
        Inst::Bge { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let taken = val1 >= val2;
            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };

            InstLog::Branch {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rs1_val: val1,
                rs2_val: val2,
                taken,
                target_pc,
            }
        }
        Inst::Lui { rd, imm } => {
            let value = (imm << 12) as i32;
            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            InstLog::Immediate {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rd,
                rd_old,
                rd_new: value,
            }
        }
        Inst::Auipc { rd, imm } => {
            let value = (pc.wrapping_add(imm << 12)) as i32;
            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            InstLog::Immediate {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                rd,
                rd_old,
                rd_new: value,
            }
        }
        Inst::Slt { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = if val1 < val2 { 1 } else { 0 };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            InstLog::Arithmetic {
                cycle: 0,
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: val1,
                rs2_val: Some(val2),
                rd_old,
                rd_new: result,
            }
        }
        Inst::Slti { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = if val1 < imm { 1 } else { 0 };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            InstLog::Arithmetic {
                cycle: 0,
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: val1,
                rs2_val: None,
                rd_old,
                rd_new: result,
            }
        }
        Inst::Sltu { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1) as u32;
            let val2 = read_reg(regs, rs2) as u32;
            let rd_old = read_reg(regs, rd);
            let result = if val1 < val2 { 1 } else { 0 };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            InstLog::Arithmetic {
                cycle: 0,
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: val1 as i32,
                rs2_val: Some(val2 as i32),
                rd_old,
                rd_new: result,
            }
        }
        Inst::Sltiu { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1) as u32;
            let imm_u = imm as u32;
            let rd_old = read_reg(regs, rd);
            let result = if val1 < imm_u { 1 } else { 0 };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            InstLog::Arithmetic {
                cycle: 0,
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: val1 as i32,
                rs2_val: None,
                rd_old,
                rd_new: result,
            }
        }
        Inst::Xori { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = val1 ^ imm;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            InstLog::Arithmetic {
                cycle: 0,
                pc,
                instruction: instruction_word,
                rd,
                rs1_val: val1,
                rs2_val: None,
                rd_old,
                rd_new: result,
            }
        }
        Inst::Ecall => {
            syscall = true;
            InstLog::System {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                kind: SystemKind::Ecall,
            }
        }
        Inst::Ebreak => {
            should_halt = true;
            InstLog::System {
                cycle: 0, // Will be set by emu
                pc,
                instruction: instruction_word,
                kind: SystemKind::Ebreak,
            }
        }
    };

    Ok(ExecutionResult {
        new_pc,
        should_halt,
        syscall,
        log,
    })
}
