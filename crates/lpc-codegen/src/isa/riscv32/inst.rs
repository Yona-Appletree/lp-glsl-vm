//! Structured RISC-V instruction representation.
//!
//! This module provides a structured representation of RISC-V instructions
//! as Rust enums, enabling type-safe pattern matching and testing.

use super::regs::Gpr;

/// A structured representation of a RISC-V instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inst {
    // Arithmetic instructions
    /// ADD: rd = rs1 + rs2
    Add { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// SUB: rd = rs1 - rs2
    Sub { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// MUL: rd = rs1 * rs2 (M extension)
    Mul { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// MULH: rd = high 32 bits of (rs1 * rs2) (signed, M extension)
    Mulh { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// DIV: rd = rs1 / rs2 (signed, M extension)
    Div { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// REM: rd = rs1 % rs2 (signed, M extension)
    Rem { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// ADDI: rd = rs1 + imm
    Addi { rd: Gpr, rs1: Gpr, imm: i32 },

    // Load/Store instructions
    /// LW: rd = mem[rs1 + imm]
    Lw { rd: Gpr, rs1: Gpr, imm: i32 },
    /// SW: mem[rs1 + imm] = rs2
    Sw { rs1: Gpr, rs2: Gpr, imm: i32 },

    // Control flow instructions
    /// JAL: rd = pc + 4; pc = pc + imm
    Jal { rd: Gpr, imm: i32 },
    /// JALR: rd = pc + 4; pc = rs1 + imm
    Jalr { rd: Gpr, rs1: Gpr, imm: i32 },
    /// BEQ: if rs1 == rs2, pc = pc + imm
    Beq { rs1: Gpr, rs2: Gpr, imm: i32 },
    /// BNE: if rs1 != rs2, pc = pc + imm
    Bne { rs1: Gpr, rs2: Gpr, imm: i32 },
    /// BLT: if rs1 < rs2 (signed), pc = pc + imm
    Blt { rs1: Gpr, rs2: Gpr, imm: i32 },
    /// BGE: if rs1 >= rs2 (signed), pc = pc + imm
    Bge { rs1: Gpr, rs2: Gpr, imm: i32 },

    // Comparison instructions
    /// SLT: rd = (rs1 < rs2) ? 1 : 0 (signed)
    Slt { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// SLTI: rd = (rs1 < imm) ? 1 : 0 (signed)
    Slti { rd: Gpr, rs1: Gpr, imm: i32 },
    /// SLTU: rd = (rs1 < rs2) ? 1 : 0 (unsigned)
    Sltu { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// SLTIU: rd = (rs1 < imm) ? 1 : 0 (unsigned)
    Sltiu { rd: Gpr, rs1: Gpr, imm: i32 },
    /// XORI: rd = rs1 ^ imm
    Xori { rd: Gpr, rs1: Gpr, imm: i32 },

    // Logical instructions
    /// AND: rd = rs1 & rs2
    And { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// ANDI: rd = rs1 & imm
    Andi { rd: Gpr, rs1: Gpr, imm: i32 },
    /// OR: rd = rs1 | rs2
    Or { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// ORI: rd = rs1 | imm
    Ori { rd: Gpr, rs1: Gpr, imm: i32 },
    /// XOR: rd = rs1 ^ rs2
    Xor { rd: Gpr, rs1: Gpr, rs2: Gpr },

    // Shift instructions
    /// SLL: rd = rs1 << rs2 (logical left shift)
    Sll { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// SLLI: rd = rs1 << imm (logical left shift immediate)
    Slli { rd: Gpr, rs1: Gpr, imm: i32 },
    /// SRL: rd = rs1 >> rs2 (logical right shift)
    Srl { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// SRLI: rd = rs1 >> imm (logical right shift immediate)
    Srli { rd: Gpr, rs1: Gpr, imm: i32 },
    /// SRA: rd = rs1 >> rs2 (arithmetic right shift)
    Sra { rd: Gpr, rs1: Gpr, rs2: Gpr },
    /// SRAI: rd = rs1 >> imm (arithmetic right shift immediate)
    Srai { rd: Gpr, rs1: Gpr, imm: i32 },

    // Immediate generation
    /// LUI: rd = imm << 12
    Lui { rd: Gpr, imm: u32 },
    /// AUIPC: rd = pc + (imm << 12)
    Auipc { rd: Gpr, imm: u32 },

    // System instructions
    /// ECALL: Environment call (syscall)
    Ecall,
    /// EBREAK: Environment break (halt/debug breakpoint)
    Ebreak,
}

impl Inst {
    /// Encode this instruction to its binary representation.
    pub fn encode(&self) -> u32 {
        use super::encode::*;
        match self {
            Inst::Add { rd, rs1, rs2 } => add(*rd, *rs1, *rs2),
            Inst::Sub { rd, rs1, rs2 } => sub(*rd, *rs1, *rs2),
            Inst::Mul { rd, rs1, rs2 } => mul(*rd, *rs1, *rs2),
            Inst::Mulh { rd, rs1, rs2 } => mulh(*rd, *rs1, *rs2),
            Inst::Div { rd, rs1, rs2 } => div(*rd, *rs1, *rs2),
            Inst::Rem { rd, rs1, rs2 } => rem(*rd, *rs1, *rs2),
            Inst::Addi { rd, rs1, imm } => addi(*rd, *rs1, *imm),
            Inst::Lw { rd, rs1, imm } => lw(*rd, *rs1, *imm),
            Inst::Sw { rs1, rs2, imm } => sw(*rs1, *rs2, *imm),
            Inst::Jal { rd, imm } => jal(*rd, *imm),
            Inst::Jalr { rd, rs1, imm } => jalr(*rd, *rs1, *imm),
            Inst::Beq { rs1, rs2, imm } => beq(*rs1, *rs2, *imm),
            Inst::Bne { rs1, rs2, imm } => bne(*rs1, *rs2, *imm),
            Inst::Blt { rs1, rs2, imm } => blt(*rs1, *rs2, *imm),
            Inst::Bge { rs1, rs2, imm } => bge(*rs1, *rs2, *imm),
            Inst::Slt { rd, rs1, rs2 } => slt(*rd, *rs1, *rs2),
            Inst::Slti { rd, rs1, imm } => slti(*rd, *rs1, *imm),
            Inst::Sltu { rd, rs1, rs2 } => sltu(*rd, *rs1, *rs2),
            Inst::Sltiu { rd, rs1, imm } => sltiu(*rd, *rs1, *imm),
            Inst::Xori { rd, rs1, imm } => xori(*rd, *rs1, *imm),
            Inst::And { rd, rs1, rs2 } => and(*rd, *rs1, *rs2),
            Inst::Andi { rd, rs1, imm } => andi(*rd, *rs1, *imm),
            Inst::Or { rd, rs1, rs2 } => or(*rd, *rs1, *rs2),
            Inst::Ori { rd, rs1, imm } => ori(*rd, *rs1, *imm),
            Inst::Xor { rd, rs1, rs2 } => xor(*rd, *rs1, *rs2),
            Inst::Sll { rd, rs1, rs2 } => sll(*rd, *rs1, *rs2),
            Inst::Slli { rd, rs1, imm } => slli(*rd, *rs1, *imm),
            Inst::Srl { rd, rs1, rs2 } => srl(*rd, *rs1, *rs2),
            Inst::Srli { rd, rs1, imm } => srli(*rd, *rs1, *imm),
            Inst::Sra { rd, rs1, rs2 } => sra(*rd, *rs1, *rs2),
            Inst::Srai { rd, rs1, imm } => srai(*rd, *rs1, *imm),
            Inst::Lui { rd, imm } => lui(*rd, *imm),
            Inst::Auipc { rd, imm } => auipc(*rd, *imm),
            Inst::Ecall => ecall(),
            Inst::Ebreak => ebreak(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::regs::Gpr, *};

    #[test]
    fn test_inst_encode_add() {
        let inst = Inst::Add {
            rd: Gpr::A0,
            rs1: Gpr::A1,
            rs2: Gpr::A2,
        };
        assert_eq!(inst.encode(), 0x00c58533);
    }

    #[test]
    fn test_inst_encode_addi() {
        let inst = Inst::Addi {
            rd: Gpr::A0,
            rs1: Gpr::A1,
            imm: 5,
        };
        assert_eq!(inst.encode(), 0x00558513);
    }

    #[test]
    fn test_inst_encode_addi_negative() {
        let inst = Inst::Addi {
            rd: Gpr::A0,
            rs1: Gpr::A1,
            imm: -5,
        };
        assert_eq!(inst.encode(), 0xffb58513);
    }

    #[test]
    fn test_inst_encode_lw() {
        let inst = Inst::Lw {
            rd: Gpr::A0,
            rs1: Gpr::A1,
            imm: 4,
        };
        assert_eq!(inst.encode(), 0x0045a503);
    }

    #[test]
    fn test_inst_encode_sw() {
        let inst = Inst::Sw {
            rs1: Gpr::A1,
            rs2: Gpr::A0,
            imm: 4,
        };
        assert_eq!(inst.encode(), 0x00a5a223);
    }

    #[test]
    fn test_inst_encode_jal() {
        let inst = Inst::Jal {
            rd: Gpr::Ra,
            imm: 0,
        };
        // jal ra, 0: opcode=0x6f, rd=1 (ra), imm=0
        // Encoding: 0x6f | (1 << 7) = 0x6f | 0x80 = 0xef
        assert_eq!(inst.encode(), 0x000000ef);
    }

    #[test]
    fn test_inst_encode_jalr() {
        let inst = Inst::Jalr {
            rd: Gpr::Zero,
            rs1: Gpr::Ra,
            imm: 0,
        };
        assert_eq!(inst.encode(), 0x00008067);
    }

    #[test]
    fn test_inst_encode_lui() {
        let inst = Inst::Lui {
            rd: Gpr::A0,
            imm: 0x12345000,
        };
        assert_eq!(inst.encode(), 0x12345537);
    }

    #[test]
    fn test_inst_encode_ecall() {
        let inst = Inst::Ecall;
        assert_eq!(inst.encode(), 0x00000073);
    }

    #[test]
    fn test_inst_encode_ebreak() {
        let inst = Inst::Ebreak;
        assert_eq!(inst.encode(), 0x00100073);
    }
}
