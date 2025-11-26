//! Instruction decoder for RISC-V 32-bit instructions.

extern crate alloc;

use alloc::{format, string::String};
use riscv32_encoder::Gpr;

/// Decoded instruction representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodedInstruction {
    // Arithmetic
    Add { rd: Gpr, rs1: Gpr, rs2: Gpr },
    Sub { rd: Gpr, rs1: Gpr, rs2: Gpr },
    Mul { rd: Gpr, rs1: Gpr, rs2: Gpr },
    Addi { rd: Gpr, rs1: Gpr, imm: i32 },

    // Load/Store
    Lw { rd: Gpr, rs1: Gpr, imm: i32 },
    Sw { rs1: Gpr, rs2: Gpr, imm: i32 },

    // Control flow
    Jal { rd: Gpr, imm: i32 },
    Jalr { rd: Gpr, rs1: Gpr, imm: i32 },
    Beq { rs1: Gpr, rs2: Gpr, imm: i32 },
    Bne { rs1: Gpr, rs2: Gpr, imm: i32 },
    Blt { rs1: Gpr, rs2: Gpr, imm: i32 },
    Bge { rs1: Gpr, rs2: Gpr, imm: i32 },

    // Immediate generation
    Lui { rd: Gpr, imm: u32 },
    Auipc { rd: Gpr, imm: u32 },

    // System
    Ecall,
    Ebreak,
}

/// Decode a 32-bit instruction word into a structured representation.
pub fn decode_instruction(inst: u32) -> Result<DecodedInstruction, String> {
    let opcode = (inst & 0x7f) as u8;
    let rd = Gpr::new(((inst >> 7) & 0x1f) as u8);
    let funct3 = ((inst >> 12) & 0x7) as u8;
    let rs1 = Gpr::new(((inst >> 15) & 0x1f) as u8);
    let rs2 = Gpr::new(((inst >> 20) & 0x1f) as u8);
    let funct7 = ((inst >> 25) & 0x7f) as u8;

    // Extract immediates
    let imm_i = {
        let imm_raw = (inst >> 20) & 0xfff;
        if (imm_raw & 0x800) != 0 {
            (imm_raw | 0xfffff000) as i32
        } else {
            imm_raw as i32
        }
    };

    let imm_s = {
        let imm_lo = (inst >> 7) & 0x1f;
        let imm_hi_raw = (inst >> 25) & 0x7f;
        let imm_hi = if (imm_hi_raw & 0x40) != 0 {
            (imm_hi_raw | 0xffffff80) as i32
        } else {
            imm_hi_raw as i32
        };
        (imm_hi << 5) | (imm_lo as i32)
    };

    let imm_b = {
        let imm_12 = ((inst >> 31) & 0x1) as i32;
        let imm_10_5 = ((inst >> 25) & 0x3f) as i32;
        let imm_4_1 = ((inst >> 8) & 0xf) as i32;
        let imm_11 = ((inst >> 7) & 0x1) as i32;
        let imm = (imm_12 << 12) | (imm_11 << 11) | (imm_10_5 << 5) | (imm_4_1 << 1);
        if (imm & 0x1000) != 0 {
            imm | (-8192i32)
        } else {
            imm
        }
    };

    let imm_j = {
        let imm_20 = ((inst >> 31) & 0x1) as i32;
        let imm_10_1 = ((inst >> 21) & 0x3ff) as i32;
        let imm_11 = ((inst >> 20) & 0x1) as i32;
        let imm_19_12 = ((inst >> 12) & 0xff) as i32;
        let imm = (imm_20 << 20) | (imm_19_12 << 12) | (imm_11 << 11) | (imm_10_1 << 1);
        if (imm & 0x100000) != 0 {
            imm | (-2097152i32)
        } else {
            imm
        }
    };

    let imm_u = (inst >> 12) & 0xfffff;

    match opcode {
        0x33 => {
            // R-type (arithmetic)
            match (funct3, funct7) {
                (0x0, 0x0) => Ok(DecodedInstruction::Add { rd, rs1, rs2 }),
                (0x0, 0x20) => Ok(DecodedInstruction::Sub { rd, rs1, rs2 }),
                (0x0, 0x01) => Ok(DecodedInstruction::Mul { rd, rs1, rs2 }),
                _ => Err(format!("Unknown R-type instruction: funct3=0x{:x}, funct7=0x{:x}", funct3, funct7)),
            }
        }
        0x13 => {
            // I-type (immediate arithmetic)
            match funct3 {
                0x0 => Ok(DecodedInstruction::Addi { rd, rs1, imm: imm_i }),
                _ => Err(format!("Unknown I-type arithmetic instruction: funct3=0x{:x}", funct3)),
            }
        }
        0x03 => {
            // I-type (load)
            match funct3 {
                0x2 => Ok(DecodedInstruction::Lw { rd, rs1, imm: imm_i }),
                _ => Err(format!("Unknown load instruction: funct3=0x{:x}", funct3)),
            }
        }
        0x23 => {
            // S-type (store)
            match funct3 {
                0x2 => Ok(DecodedInstruction::Sw { rs1, rs2, imm: imm_s }),
                _ => Err(format!("Unknown store instruction: funct3=0x{:x}", funct3)),
            }
        }
        0x37 => {
            // U-type (lui)
            Ok(DecodedInstruction::Lui { rd, imm: imm_u })
        }
        0x17 => {
            // U-type (auipc)
            Ok(DecodedInstruction::Auipc { rd, imm: imm_u })
        }
        0x6f => {
            // J-type (jal)
            Ok(DecodedInstruction::Jal { rd, imm: imm_j })
        }
        0x67 => {
            // I-type (jalr)
            match funct3 {
                0x0 => Ok(DecodedInstruction::Jalr { rd, rs1, imm: imm_i }),
                _ => Err(format!("Unknown jalr instruction: funct3=0x{:x}", funct3)),
            }
        }
        0x63 => {
            // B-type (branch)
            match funct3 {
                0x0 => Ok(DecodedInstruction::Beq { rs1, rs2, imm: imm_b }),
                0x1 => Ok(DecodedInstruction::Bne { rs1, rs2, imm: imm_b }),
                0x4 => Ok(DecodedInstruction::Blt { rs1, rs2, imm: imm_b }),
                0x5 => Ok(DecodedInstruction::Bge { rs1, rs2, imm: imm_b }),
                _ => Err(format!("Unknown branch instruction: funct3=0x{:x}", funct3)),
            }
        }
        0x73 => {
            // System instructions
            if inst == 0x00000073 {
                Ok(DecodedInstruction::Ecall)
            } else if inst == 0x00100073 {
                Ok(DecodedInstruction::Ebreak)
            } else {
                Err(format!("Unknown system instruction: 0x{:08x}", inst))
            }
        }
        _ => Err(format!("Unknown opcode: 0x{:02x}", opcode)),
    }
}

