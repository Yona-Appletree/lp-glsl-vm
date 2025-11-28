//! RISC-V 32-bit instruction decoding.
//!
//! This module provides functions to decode RISC-V 32-bit instructions
//! into their structured representation.

use super::{inst::Inst, regs::Gpr};

/// Decoded instruction fields.
///
/// Contains all the extracted fields from an instruction word.
#[derive(Debug, Clone, Copy)]
pub struct DecodedFields {
    pub opcode: u8,
    pub rd: u8,
    pub rs1: u8,
    pub rs2: u8,
    pub funct3: u8,
    pub funct7: u8,
    pub imm_i: i32,
    pub imm_s: i32,
    pub imm_b: i32,
    pub imm_j: i32,
    pub imm_u: u32,
}

/// Extract all fields from a 32-bit instruction word.
pub fn extract_fields(inst: u32) -> DecodedFields {
    let opcode = (inst & 0x7f) as u8;
    let rd = ((inst >> 7) & 0x1f) as u8;
    let funct3 = ((inst >> 12) & 0x7) as u8;
    let rs1 = ((inst >> 15) & 0x1f) as u8;
    let rs2 = ((inst >> 20) & 0x1f) as u8;
    let funct7 = ((inst >> 25) & 0x7f) as u8;

    // Extract immediates for different instruction types
    let imm_i = {
        let imm_raw = (inst >> 20) & 0xfff;
        // Sign-extend 12-bit immediate
        if (imm_raw & 0x800) != 0 {
            (imm_raw | 0xfffff000) as i32
        } else {
            imm_raw as i32
        }
    };

    let imm_s = {
        let imm_lo = (inst >> 7) & 0x1f;
        let imm_hi_raw = (inst >> 25) & 0x7f;
        // Sign-extend 7-bit high part
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
        // Sign-extend
        let imm = (imm_12 << 12) | (imm_11 << 11) | (imm_10_5 << 5) | (imm_4_1 << 1);
        if (imm & 0x1000) != 0 {
            imm | (-8192i32) // 0xffffe000 as i32
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
        // Sign-extend
        if (imm & 0x100000) != 0 {
            imm | (-2097152i32) // 0xffe00000 as i32
        } else {
            imm
        }
    };

    let imm_u = (inst >> 12) & 0xfffff; // 20-bit upper immediate

    DecodedFields {
        opcode,
        rd,
        rs1,
        rs2,
        funct3,
        funct7,
        imm_i,
        imm_s,
        imm_b,
        imm_j,
        imm_u,
    }
}

/// Decode a 32-bit instruction word into a structured representation.
pub fn decode_instruction(inst: u32) -> Result<Inst, alloc::string::String> {
    use alloc::format;

    let fields = extract_fields(inst);
    let rd = Gpr::new(fields.rd);
    let rs1 = Gpr::new(fields.rs1);
    let rs2 = Gpr::new(fields.rs2);

    match fields.opcode {
        0x33 => {
            // R-type (arithmetic)
            match (fields.funct3, fields.funct7) {
                (0x0, 0x0) => Ok(Inst::Add { rd, rs1, rs2 }),
                (0x0, 0x20) => Ok(Inst::Sub { rd, rs1, rs2 }),
                (0x0, 0x01) => Ok(Inst::Mul { rd, rs1, rs2 }),
                (0x4, 0x01) => Ok(Inst::Div { rd, rs1, rs2 }),
                (0x6, 0x01) => Ok(Inst::Rem { rd, rs1, rs2 }),
                (0x2, 0x0) => Ok(Inst::Slt { rd, rs1, rs2 }),
                (0x3, 0x0) => Ok(Inst::Sltu { rd, rs1, rs2 }),
                _ => Err(format!(
                    "Unknown R-type instruction: funct3=0x{:x}, funct7=0x{:x}",
                    fields.funct3, fields.funct7
                )),
            }
        }
        0x13 => {
            // I-type (immediate arithmetic)
            match fields.funct3 {
                0x0 => Ok(Inst::Addi {
                    rd,
                    rs1,
                    imm: fields.imm_i,
                }),
                0x2 => Ok(Inst::Slti {
                    rd,
                    rs1,
                    imm: fields.imm_i,
                }),
                0x3 => Ok(Inst::Sltiu {
                    rd,
                    rs1,
                    imm: fields.imm_i,
                }),
                0x4 => Ok(Inst::Xori {
                    rd,
                    rs1,
                    imm: fields.imm_i,
                }),
                _ => Err(format!(
                    "Unknown I-type arithmetic instruction: funct3=0x{:x}",
                    fields.funct3
                )),
            }
        }
        0x03 => {
            // I-type (load)
            match fields.funct3 {
                0x2 => Ok(Inst::Lw {
                    rd,
                    rs1,
                    imm: fields.imm_i,
                }),
                _ => Err(format!(
                    "Unknown load instruction: funct3=0x{:x}",
                    fields.funct3
                )),
            }
        }
        0x23 => {
            // S-type (store)
            match fields.funct3 {
                0x2 => Ok(Inst::Sw {
                    rs1,
                    rs2,
                    imm: fields.imm_s,
                }),
                _ => Err(format!(
                    "Unknown store instruction: funct3=0x{:x}",
                    fields.funct3
                )),
            }
        }
        0x37 => {
            // U-type (lui)
            Ok(Inst::Lui {
                rd,
                imm: fields.imm_u,
            })
        }
        0x17 => {
            // U-type (auipc)
            Ok(Inst::Auipc {
                rd,
                imm: fields.imm_u,
            })
        }
        0x6f => {
            // J-type (jal)
            Ok(Inst::Jal {
                rd,
                imm: fields.imm_j,
            })
        }
        0x67 => {
            // I-type (jalr)
            match fields.funct3 {
                0x0 => Ok(Inst::Jalr {
                    rd,
                    rs1,
                    imm: fields.imm_i,
                }),
                _ => Err(format!(
                    "Unknown jalr instruction: funct3=0x{:x}",
                    fields.funct3
                )),
            }
        }
        0x63 => {
            // B-type (branch)
            match fields.funct3 {
                0x0 => Ok(Inst::Beq {
                    rs1,
                    rs2,
                    imm: fields.imm_b,
                }),
                0x1 => Ok(Inst::Bne {
                    rs1,
                    rs2,
                    imm: fields.imm_b,
                }),
                0x4 => Ok(Inst::Blt {
                    rs1,
                    rs2,
                    imm: fields.imm_b,
                }),
                0x5 => Ok(Inst::Bge {
                    rs1,
                    rs2,
                    imm: fields.imm_b,
                }),
                _ => Err(format!(
                    "Unknown branch instruction: funct3=0x{:x}",
                    fields.funct3
                )),
            }
        }
        0x73 => {
            // System instructions
            if inst == 0x00000073 {
                Ok(Inst::Ecall)
            } else if inst == 0x00100073 {
                Ok(Inst::Ebreak)
            } else {
                Err(format!("Unknown system instruction: 0x{:08x}", inst))
            }
        }
        _ => Err(format!("Unknown opcode: 0x{:02x}", fields.opcode)),
    }
}
