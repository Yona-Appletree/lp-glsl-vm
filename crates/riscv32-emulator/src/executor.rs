//! Instruction executor for RISC-V 32-bit instructions.

extern crate alloc;

use alloc::string::String;
use crate::decoder::DecodedInstruction;
use crate::error::EmulatorError;
use crate::logging::InstructionLog;
use crate::memory::Memory;
use riscv32_encoder::Gpr;

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
    pub log: InstructionLog,
}

/// Helper to read register (x0 always returns 0)
fn read_reg(regs: &[i32; 32], reg: Gpr) -> i32 {
    if reg.num() == 0 {
        0
    } else {
        regs[reg.num() as usize]
    }
}

/// Helper to write register (x0 writes are ignored)
fn write_reg(regs: &mut [i32; 32], log: &mut InstructionLog, reg: Gpr, value: i32) {
    if reg.num() != 0 {
        let old_value = regs[reg.num() as usize];
        regs[reg.num() as usize] = value;
        log.regs_written.push((reg, old_value, value));
    }
}

/// Execute a decoded instruction.
pub fn execute_instruction(
    inst: DecodedInstruction,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    disassembly: String,
) -> Result<ExecutionResult, EmulatorError> {
    let mut log = InstructionLog::new(pc, 0, disassembly);
    let mut new_pc: Option<u32> = None;
    let mut should_halt = false;
    let mut syscall = false;

    match inst {
        DecodedInstruction::Add { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            log.regs_read.push((rs1, val1));
            log.regs_read.push((rs2, val2));
            let result = val1.wrapping_add(val2);
            write_reg(regs, &mut log, rd, result);
        }
        DecodedInstruction::Sub { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            log.regs_read.push((rs1, val1));
            log.regs_read.push((rs2, val2));
            let result = val1.wrapping_sub(val2);
            write_reg(regs, &mut log, rd, result);
        }
        DecodedInstruction::Mul { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            log.regs_read.push((rs1, val1));
            log.regs_read.push((rs2, val2));
            let result = val1.wrapping_mul(val2);
            write_reg(regs, &mut log, rd, result);
        }
        DecodedInstruction::Addi { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            log.regs_read.push((rs1, val1));
            let result = val1.wrapping_add(imm);
            write_reg(regs, &mut log, rd, result);
        }
        DecodedInstruction::Lw { rd, rs1, imm } => {
            let base = read_reg(regs, rs1);
            log.regs_read.push((rs1, base));
            let address = base.wrapping_add(imm) as u32;

            // Save register state for error context
            let error_regs = *regs;
            let value = memory.read_word(address).map_err(|mut e| {
                match &mut e {
                    EmulatorError::InvalidMemoryAccess { regs: err_regs, pc: err_pc, .. } => {
                        *err_regs = error_regs;
                        *err_pc = pc;
                    }
                    EmulatorError::UnalignedAccess { regs: err_regs, pc: err_pc, .. } => {
                        *err_regs = error_regs;
                        *err_pc = pc;
                    }
                    _ => {}
                }
                e
            })?;

            log.memory_reads.push((address, value));
            write_reg(regs, &mut log, rd, value);
        }
        DecodedInstruction::Sw { rs1, rs2, imm } => {
            let base = read_reg(regs, rs1);
            let value = read_reg(regs, rs2);
            log.regs_read.push((rs1, base));
            log.regs_read.push((rs2, value));
            let address = base.wrapping_add(imm) as u32;

            // Read old value before write
            let old_value = memory.read_word(address).unwrap_or(0);
            log.memory_reads.push((address, old_value));

            // Save register state for error context
            let error_regs = *regs;
            memory.write_word(address, value).map_err(|mut e| {
                match &mut e {
                    EmulatorError::InvalidMemoryAccess { regs: err_regs, pc: err_pc, .. } => {
                        *err_regs = error_regs;
                        *err_pc = pc;
                    }
                    EmulatorError::UnalignedAccess { regs: err_regs, pc: err_pc, .. } => {
                        *err_regs = error_regs;
                        *err_pc = pc;
                    }
                    _ => {}
                }
                e
            })?;

            log.memory_writes.push((address, old_value, value));
        }
        DecodedInstruction::Jal { rd, imm } => {
            let next_pc = pc.wrapping_add(4);
            write_reg(regs, &mut log, rd, next_pc as i32);
            let target = pc.wrapping_add(imm as u32);
            new_pc = Some(target);
            log.pc_change = Some((pc, target));
        }
        DecodedInstruction::Jalr { rd, rs1, imm } => {
            let base = read_reg(regs, rs1);
            log.regs_read.push((rs1, base));
            let next_pc = pc.wrapping_add(4);
            write_reg(regs, &mut log, rd, next_pc as i32);
            let target = (base.wrapping_add(imm) as u32) & !1; // Clear LSB
            new_pc = Some(target);
            log.pc_change = Some((pc, target));
        }
        DecodedInstruction::Beq { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            log.regs_read.push((rs1, val1));
            log.regs_read.push((rs2, val2));
            if val1 == val2 {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                log.pc_change = Some((pc, target));
            }
        }
        DecodedInstruction::Bne { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            log.regs_read.push((rs1, val1));
            log.regs_read.push((rs2, val2));
            if val1 != val2 {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                log.pc_change = Some((pc, target));
            }
        }
        DecodedInstruction::Blt { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            log.regs_read.push((rs1, val1));
            log.regs_read.push((rs2, val2));
            if val1 < val2 {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                log.pc_change = Some((pc, target));
            }
        }
        DecodedInstruction::Bge { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            log.regs_read.push((rs1, val1));
            log.regs_read.push((rs2, val2));
            if val1 >= val2 {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                log.pc_change = Some((pc, target));
            }
        }
        DecodedInstruction::Lui { rd, imm } => {
            let value = (imm << 12) as i32;
            write_reg(regs, &mut log, rd, value);
        }
        DecodedInstruction::Auipc { rd, imm } => {
            let value = (pc.wrapping_add(imm << 12)) as i32;
            write_reg(regs, &mut log, rd, value);
        }
        DecodedInstruction::Ecall => {
            syscall = true;
        }
        DecodedInstruction::Ebreak => {
            should_halt = true;
        }
    }

    Ok(ExecutionResult {
        new_pc,
        should_halt,
        syscall,
        log,
    })
}
