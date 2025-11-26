//! RISC-V 32-bit instruction disassembly.

use alloc::{format, string::String, vec::Vec};

/// Disassemble a single RISC-V 32-bit instruction.
///
/// Returns a human-readable string like "add a0, a1, a2" or "jal ra, 16".
pub fn disassemble_instruction(inst: u32) -> String {
    let opcode = inst & 0x7f;
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
    let imm_u = (inst >> 12) & 0xfffff; // 20-bit upper immediate
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

    match opcode {
        0x33 => {
            // R-type (arithmetic)
            match (funct3, funct7) {
                (0x0, 0x0) => format!("add {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2)),
                (0x0, 0x20) => {
                    format!("sub {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2))
                }
                (0x0, 0x01) => {
                    format!("mul {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2))
                }
                _ => format!("unknown_r_type 0x{:08x}", inst),
            }
        }
        0x13 => {
            // I-type (immediate arithmetic)
            match funct3 {
                0x0 => format!("addi {}, {}, {}", gpr_name(rd), gpr_name(rs1), imm_i),
                _ => format!("unknown_i_type 0x{:08x}", inst),
            }
        }
        0x03 => {
            // I-type (load)
            match funct3 {
                0x2 => format!("lw {}, {}({})", gpr_name(rd), imm_i, gpr_name(rs1)),
                _ => format!("unknown_load 0x{:08x}", inst),
            }
        }
        0x23 => {
            // S-type (store)
            match funct3 {
                0x2 => format!("sw {}, {}({})", gpr_name(rs2), imm_s, gpr_name(rs1)),
                _ => format!("unknown_store 0x{:08x}", inst),
            }
        }
        0x37 => {
            // U-type (lui)
            format!("lui {}, 0x{:05x}", gpr_name(rd), imm_u)
        }
        0x17 => {
            // U-type (auipc)
            format!("auipc {}, 0x{:05x}", gpr_name(rd), imm_u)
        }
        0x6f => {
            // J-type (jal)
            format!("jal {}, {}", gpr_name(rd), imm_j)
        }
        0x67 => {
            // I-type (jalr)
            match funct3 {
                0x0 => format!("jalr {}, {}({})", gpr_name(rd), imm_i, gpr_name(rs1)),
                _ => format!("unknown_jalr 0x{:08x}", inst),
            }
        }
        0x63 => {
            // B-type (branch)
            match funct3 {
                0x0 => format!("beq {}, {}, {}", gpr_name(rs1), gpr_name(rs2), imm_b),
                0x1 => format!("bne {}, {}, {}", gpr_name(rs1), gpr_name(rs2), imm_b),
                0x4 => format!("blt {}, {}, {}", gpr_name(rs1), gpr_name(rs2), imm_b),
                0x5 => format!("bge {}, {}, {}", gpr_name(rs1), gpr_name(rs2), imm_b),
                _ => format!("unknown_branch 0x{:08x}", inst),
            }
        }
        0x73 => {
            // System instructions
            if inst == 0x00000073 {
                String::from("ecall")
            } else {
                format!("unknown_system 0x{:08x}", inst)
            }
        }
        _ => format!("unknown 0x{:08x} (opcode=0x{:02x})", inst, opcode),
    }
}

/// Disassemble a code buffer containing RISC-V instructions.
///
/// Returns a formatted string with one instruction per line, showing
/// the address/offset and the disassembled instruction.
pub fn disassemble_code(code: &[u8]) -> String {
    let mut result = String::new();
    let mut offset = 0;

    while offset + 4 <= code.len() {
        // Read 32-bit instruction (little-endian)
        let inst_bytes = [
            code[offset],
            code[offset + 1],
            code[offset + 2],
            code[offset + 3],
        ];
        let inst = u32::from_le_bytes(inst_bytes);

        let disasm = disassemble_instruction(inst);
        result.push_str(&format!("0x{:04x}: {}\n", offset, disasm));

        offset += 4;
    }

    // Handle remaining bytes (if any)
    if offset < code.len() {
        result.push_str(&format!("0x{:04x}: <incomplete instruction>\n", offset));
    }

    result
}

/// Get the name of a general-purpose register.
fn gpr_name(num: u8) -> &'static str {
    match num {
        0 => "zero",
        1 => "ra",
        2 => "sp",
        3 => "gp",
        4 => "tp",
        5 => "t0",
        6 => "t1",
        7 => "t2",
        8 => "s0",
        9 => "s1",
        10 => "a0",
        11 => "a1",
        12 => "a2",
        13 => "a3",
        14 => "a4",
        15 => "a5",
        16 => "a6",
        17 => "a7",
        18 => "s2",
        19 => "s3",
        20 => "s4",
        21 => "s5",
        22 => "s6",
        23 => "s7",
        24 => "s8",
        25 => "s9",
        26 => "s10",
        27 => "s11",
        28 => "t3",
        29 => "t4",
        30 => "t5",
        31 => "t6",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{encode::*, Gpr};

    #[test]
    fn test_disassemble_add() {
        let inst = add(Gpr::A0, Gpr::A1, Gpr::A2);
        let disasm = disassemble_instruction(inst);
        assert_eq!(disasm, "add a0, a1, a2");
    }

    #[test]
    fn test_disassemble_sub() {
        let inst = sub(Gpr::A0, Gpr::A1, Gpr::A2);
        let disasm = disassemble_instruction(inst);
        assert_eq!(disasm, "sub a0, a1, a2");
    }

    #[test]
    fn test_disassemble_addi() {
        let inst = addi(Gpr::A0, Gpr::A1, 5);
        let disasm = disassemble_instruction(inst);
        assert_eq!(disasm, "addi a0, a1, 5");
    }

    #[test]
    fn test_disassemble_addi_negative() {
        let inst = addi(Gpr::A0, Gpr::A1, -5);
        let disasm = disassemble_instruction(inst);
        assert_eq!(disasm, "addi a0, a1, -5");
    }

    #[test]
    fn test_disassemble_lui() {
        let inst = lui(Gpr::A0, 0x12345000);
        let disasm = disassemble_instruction(inst);
        assert!(disasm.contains("lui a0"));
        assert!(disasm.contains("0x12345"));
    }

    #[test]
    fn test_disassemble_ecall() {
        let inst = ecall();
        let disasm = disassemble_instruction(inst);
        assert_eq!(disasm, "ecall");
    }

    #[test]
    fn test_disassemble_code() {
        let mut code = Vec::new();
        code.extend_from_slice(&add(Gpr::A0, Gpr::A1, Gpr::A2).to_le_bytes());
        code.extend_from_slice(&addi(Gpr::A1, Gpr::A0, 10).to_le_bytes());
        code.extend_from_slice(&ecall().to_le_bytes());

        let disasm = disassemble_code(&code);
        assert!(disasm.contains("add a0, a1, a2"));
        assert!(disasm.contains("addi a1, a0, 10"));
        assert!(disasm.contains("ecall"));
    }
}
