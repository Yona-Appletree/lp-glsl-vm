//! RISC-V 32-bit instruction encoding.
//!
//! This module provides functions to encode RISC-V instructions
//! into their 32-bit binary representation.

use super::regs::Gpr;

/// Encode an R-type instruction.
///
/// Format: `opcode rd rs1 rs2 funct3 funct7`
fn encode_r(opcode: u8, rd: Gpr, rs1: Gpr, rs2: Gpr, funct3: u8, funct7: u8) -> u32 {
    let opcode = opcode as u32;
    let rd = rd.num() as u32;
    let funct3 = funct3 as u32;
    let rs1 = rs1.num() as u32;
    let rs2 = rs2.num() as u32;
    let funct7 = funct7 as u32;

    opcode | (rd << 7) | (funct3 << 12) | (rs1 << 15) | (rs2 << 20) | (funct7 << 25)
}

/// Encode an I-type instruction.
///
/// Format: `opcode rd rs1 imm[11:0] funct3`
fn encode_i(opcode: u8, rd: Gpr, rs1: Gpr, imm: i32, funct3: u8) -> u32 {
    let opcode = opcode as u32;
    let rd = rd.num() as u32;
    let funct3 = funct3 as u32;
    let rs1 = rs1.num() as u32;
    let imm = (imm as u32) & 0xfff; // 12-bit immediate

    opcode | (rd << 7) | (funct3 << 12) | (rs1 << 15) | (imm << 20)
}

/// Encode an S-type instruction.
///
/// Format: `opcode imm[4:0] rs1 rs2 imm[11:5] funct3`
fn encode_s(opcode: u8, rs1: Gpr, rs2: Gpr, imm: i32, funct3: u8) -> u32 {
    let opcode = opcode as u32;
    let funct3 = funct3 as u32;
    let rs1 = rs1.num() as u32;
    let rs2 = rs2.num() as u32;
    let imm = (imm as u32) & 0xfff; // 12-bit immediate

    let imm_lo = imm & 0x1f; // bits [4:0]
    let imm_hi = (imm >> 5) & 0x7f; // bits [11:5]

    opcode | (imm_lo << 7) | (funct3 << 12) | (rs1 << 15) | (rs2 << 20) | (imm_hi << 25)
}

/// Encode a U-type instruction.
///
/// Format: `opcode rd imm[31:12]`
fn encode_u(opcode: u8, rd: Gpr, imm: u32) -> u32 {
    let opcode = opcode as u32;
    let rd = rd.num() as u32;
    let imm_hi = (imm >> 12) & 0xfffff; // bits [31:12]

    opcode | (rd << 7) | (imm_hi << 12)
}

/// Encode a J-type instruction.
///
/// Format: `opcode rd imm[20|10:1|11|19:12]`
fn encode_j(opcode: u8, rd: Gpr, imm: i32) -> u32 {
    let opcode = opcode as u32;
    let rd = rd.num() as u32;
    let imm = imm as u32;

    // J-type immediate encoding:
    // [20] [10:1] [11] [19:12]
    let imm_20 = (imm >> 20) & 0x1;
    let imm_10_1 = (imm >> 1) & 0x3ff;
    let imm_11 = (imm >> 11) & 0x1;
    let imm_19_12 = (imm >> 12) & 0xff;

    opcode | (rd << 7) | (imm_20 << 31) | (imm_10_1 << 21) | (imm_11 << 20) | (imm_19_12 << 12)
}

/// Encode a B-type instruction.
///
/// Format: `opcode imm[12|10:5] rs1 rs2 imm[4:1|11] funct3`
fn encode_b(opcode: u8, rs1: Gpr, rs2: Gpr, imm: i32, funct3: u8) -> u32 {
    let opcode = opcode as u32;
    let funct3 = funct3 as u32;
    let rs1 = rs1.num() as u32;
    let rs2 = rs2.num() as u32;
    let imm = imm as u32;

    // B-type immediate encoding:
    // [12] [10:5] [4:1] [11]
    let imm_12 = (imm >> 12) & 0x1;
    let imm_10_5 = (imm >> 5) & 0x3f;
    let imm_4_1 = (imm >> 1) & 0xf;
    let imm_11 = (imm >> 11) & 0x1;

    opcode
        | (imm_12 << 31)
        | (imm_10_5 << 25)
        | (funct3 << 12)
        | (rs1 << 15)
        | (rs2 << 20)
        | (imm_4_1 << 8)
        | (imm_11 << 7)
}

// Arithmetic instructions

/// ADD: rd = rs1 + rs2
pub fn add(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x0, 0x0)
}

/// SUB: rd = rs1 - rs2
pub fn sub(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x0, 0x20)
}

/// MUL: rd = rs1 * rs2 (M extension)
pub fn mul(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x0, 0x01)
}

/// MULH: rd = high 32 bits of (rs1 * rs2) (signed, M extension)
pub fn mulh(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x1, 0x01)
}

/// DIV: rd = rs1 / rs2 (signed, M extension)
pub fn div(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x4, 0x01)
}

/// REM: rd = rs1 % rs2 (signed, M extension)
pub fn rem(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x6, 0x01)
}

/// ADDI: rd = rs1 + imm
pub fn addi(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x0)
}

// Load/Store instructions

/// LW: rd = mem[rs1 + imm]
pub fn lw(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x03, rd, rs1, imm, 0x2)
}

/// SW: mem[rs1 + imm] = rs2
pub fn sw(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_s(0x23, rs1, rs2, imm, 0x2)
}

// Control flow instructions

/// JAL: rd = pc + 4; pc = pc + imm
pub fn jal(rd: Gpr, imm: i32) -> u32 {
    encode_j(0x6f, rd, imm)
}

/// JALR: rd = pc + 4; pc = rs1 + imm
pub fn jalr(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x67, rd, rs1, imm, 0x0)
}

/// BEQ: if rs1 == rs2, pc = pc + imm
pub fn beq(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x0)
}

/// BNE: if rs1 != rs2, pc = pc + imm
pub fn bne(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x1)
}

/// BLT: if rs1 < rs2 (signed), pc = pc + imm
pub fn blt(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x4)
}

/// BGE: if rs1 >= rs2 (signed), pc = pc + imm
pub fn bge(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x5)
}

// Comparison instructions

/// SLT: rd = (rs1 < rs2) ? 1 : 0 (signed)
pub fn slt(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x2, 0x0)
}

/// SLTI: rd = (rs1 < imm) ? 1 : 0 (signed)
pub fn slti(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x2)
}

/// SLTU: rd = (rs1 < rs2) ? 1 : 0 (unsigned)
pub fn sltu(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x3, 0x0)
}

/// SLTIU: rd = (rs1 < imm) ? 1 : 0 (unsigned)
pub fn sltiu(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x3)
}

/// XORI: rd = rs1 ^ imm
pub fn xori(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x4)
}

// Logical instructions

/// AND: rd = rs1 & rs2
pub fn and(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x7, 0x0)
}

/// ANDI: rd = rs1 & imm
pub fn andi(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x7)
}

/// OR: rd = rs1 | rs2
pub fn or(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x6, 0x0)
}

/// ORI: rd = rs1 | imm
pub fn ori(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x6)
}

/// XOR: rd = rs1 ^ rs2
pub fn xor(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x4, 0x0)
}

// Shift instructions

/// SLL: rd = rs1 << rs2 (logical left shift)
pub fn sll(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x1, 0x0)
}

/// SLLI: rd = rs1 << imm (logical left shift immediate)
/// Note: imm[11:5] must be 0, only imm[4:0] is used for shift amount
pub fn slli(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    // For SLLI, imm[11:5] must be 0, so we only use imm[4:0]
    let imm = imm & 0x1f; // Mask to 5 bits
    encode_i(0x13, rd, rs1, imm, 0x1)
}

/// SRL: rd = rs1 >> rs2 (logical right shift)
pub fn srl(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x5, 0x0)
}

/// SRLI: rd = rs1 >> imm (logical right shift immediate)
/// Note: imm[11:5] must be 0, only imm[4:0] is used for shift amount
pub fn srli(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    // For SRLI, imm[11:5] must be 0, so we only use imm[4:0]
    let imm = imm & 0x1f; // Mask to 5 bits
    encode_i(0x13, rd, rs1, imm, 0x5)
}

/// SRA: rd = rs1 >> rs2 (arithmetic right shift)
pub fn sra(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x5, 0x20)
}

/// SRAI: rd = rs1 >> imm (arithmetic right shift immediate)
/// Note: imm[11:5] must be 0x20, only imm[4:0] is used for shift amount
pub fn srai(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    // For SRAI, imm[11:5] must be 0x20, so we encode it specially
    // imm[4:0] is the shift amount
    let imm_lo = imm & 0x1f; // bits [4:0]
    let imm_hi = 0x20; // bits [11:5] must be 0x20
    encode_i_with_imm_hi(0x13, rd, rs1, imm_lo, imm_hi, 0x5)
}

/// Encode an I-type instruction with explicit imm[11:5] (for SRAI)
fn encode_i_with_imm_hi(opcode: u8, rd: Gpr, rs1: Gpr, imm_lo: i32, imm_hi: u8, funct3: u8) -> u32 {
    let opcode = opcode as u32;
    let rd = rd.num() as u32;
    let funct3 = funct3 as u32;
    let rs1 = rs1.num() as u32;
    let imm_lo = (imm_lo as u32) & 0x1f; // bits [4:0]
    let imm_hi = imm_hi as u32; // bits [11:5]

    opcode | (rd << 7) | (funct3 << 12) | (rs1 << 15) | (imm_lo << 20) | (imm_hi << 25)
}

// Immediate generation

/// LUI: rd = imm << 12
pub fn lui(rd: Gpr, imm: u32) -> u32 {
    encode_u(0x37, rd, imm)
}

/// AUIPC: rd = pc + (imm << 12)
pub fn auipc(rd: Gpr, imm: u32) -> u32 {
    encode_u(0x17, rd, imm)
}

// System instructions

/// ECALL: Environment call (syscall)
/// Encoding: opcode=0x73, funct3=0, rs1=0, rd=0, imm=0
pub fn ecall() -> u32 {
    0x00000073
}

/// EBREAK: Environment break (halt/debug breakpoint)
/// Encoding: opcode=0x73, funct3=0, rs1=0, rd=0, imm=1
pub fn ebreak() -> u32 {
    0x00100073
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        // add a0, a1, a2
        // Expected: 0x00c58533
        let inst = add(Gpr::A0, Gpr::A1, Gpr::A2);
        assert_eq!(inst, 0x00c58533);
    }

    #[test]
    fn test_sub() {
        // sub a0, a1, a2
        // Expected: 0x40c58533
        let inst = sub(Gpr::A0, Gpr::A1, Gpr::A2);
        assert_eq!(inst, 0x40c58533);
    }

    #[test]
    fn test_mul() {
        // mul a0, a1, a2
        // Expected: 0x02c58533
        let inst = mul(Gpr::A0, Gpr::A1, Gpr::A2);
        assert_eq!(inst, 0x02c58533);
    }

    #[test]
    fn test_addi() {
        // addi a0, a1, 5
        // Expected: 0x00558513
        let inst = addi(Gpr::A0, Gpr::A1, 5);
        assert_eq!(inst, 0x00558513);
    }

    #[test]
    fn test_addi_negative() {
        // addi a0, a1, -5
        // Expected: 0xffb58513
        let inst = addi(Gpr::A0, Gpr::A1, -5);
        assert_eq!(inst, 0xffb58513);
    }

    #[test]
    fn test_lui() {
        // lui a0, 0x12345
        // Expected: 0x12345537
        let inst = lui(Gpr::A0, 0x12345000);
        assert_eq!(inst, 0x12345537);
    }

    #[test]
    fn test_jalr() {
        // jalr zero, ra, 0
        // Expected: 0x00008067
        let inst = jalr(Gpr::Zero, Gpr::Ra, 0);
        assert_eq!(inst, 0x00008067);
    }

    #[test]
    fn test_lw() {
        // lw a0, 4(a1)
        // Expected: 0x0045a503
        let inst = lw(Gpr::A0, Gpr::A1, 4);
        assert_eq!(inst, 0x0045a503);
    }

    #[test]
    fn test_sw() {
        // sw a0, 4(a1)
        // Expected: 0x00a5a223
        let inst = sw(Gpr::A1, Gpr::A0, 4);
        assert_eq!(inst, 0x00a5a223);
    }

    #[test]
    fn test_beq() {
        // beq a0, a1, 8
        // Expected: 0x00b50463 (imm[4:1] = 4 for imm=8)
        let inst = beq(Gpr::A0, Gpr::A1, 8);
        assert_eq!(inst, 0x00b50463);
    }
}
