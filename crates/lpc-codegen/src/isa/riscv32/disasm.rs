//! RISC-V 32-bit instruction disassembly.

use alloc::{collections::BTreeMap, format, string::String};

/// Disassemble a single RISC-V 32-bit instruction.
///
/// Returns a human-readable string like "add a0, a1, a2" or "jal ra, 16".
pub fn disassemble_instruction(inst: u32) -> String {
    disassemble_instruction_with_labels(inst, 0, None)
}

/// Disassemble a single RISC-V 32-bit instruction with label support.
///
/// # Arguments
///
/// * `inst` - The 32-bit instruction word
/// * `pc` - Program counter (address) of this instruction
/// * `labels` - Optional map of address -> label name, and reverse map for target lookups
///
/// Returns a human-readable string with labels substituted for offsets when available.
fn disassemble_instruction_with_labels(
    inst: u32,
    pc: u32,
    labels: Option<(&BTreeMap<u32, String>, &BTreeMap<u32, String>)>,
) -> String {
    use super::decode::extract_fields;

    let fields = extract_fields(inst);
    let opcode = fields.opcode;
    let rd = fields.rd;
    let funct3 = fields.funct3;
    let rs1 = fields.rs1;
    let rs2 = fields.rs2;
    let funct7 = fields.funct7;
    let imm_i = fields.imm_i;
    let imm_u = fields.imm_u;
    let imm_s = fields.imm_s;
    let imm_b = fields.imm_b;
    let imm_j = fields.imm_j;

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
                (0x4, 0x01) => {
                    format!("div {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2))
                }
                (0x6, 0x01) => {
                    format!("rem {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2))
                }
                (0x2, 0x0) => {
                    format!("slt {}, {}, {}", gpr_name(rd), gpr_name(rs1), gpr_name(rs2))
                }
                (0x3, 0x0) => {
                    format!(
                        "sltu {}, {}, {}",
                        gpr_name(rd),
                        gpr_name(rs1),
                        gpr_name(rs2)
                    )
                }
                _ => format!("unknown_r_type 0x{:08x}", inst),
            }
        }
        0x13 => {
            // I-type (immediate arithmetic)
            match funct3 {
                0x0 => format!("addi {}, {}, {}", gpr_name(rd), gpr_name(rs1), imm_i),
                0x2 => format!("slti {}, {}, {}", gpr_name(rd), gpr_name(rs1), imm_i),
                0x3 => format!("sltiu {}, {}, {}", gpr_name(rd), gpr_name(rs1), imm_i),
                0x4 => format!("xori {}, {}, {}", gpr_name(rd), gpr_name(rs1), imm_i),
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
            let target = pc.wrapping_add(imm_j as u32);
            let label = if let Some((_, rev)) = labels {
                if let Some(name) = rev.get(&target) {
                    name.clone()
                } else {
                    format!("label_{}", target / 4)
                }
            } else {
                format!("label_{}", target / 4)
            };
            format!("jal {}, {}", gpr_name(rd), label)
        }
        0x67 => {
            // I-type (jalr)
            match funct3 {
                0x0 => {
                    // For jalr, we can't determine the target statically, so just use the immediate
                    format!("jalr {}, {}({})", gpr_name(rd), imm_i, gpr_name(rs1))
                }
                _ => format!("unknown_jalr 0x{:08x}", inst),
            }
        }
        0x63 => {
            // B-type (branch)
            let target = pc.wrapping_add(imm_b as u32);
            let label = if let Some((_, rev)) = labels {
                if let Some(name) = rev.get(&target) {
                    name.clone()
                } else {
                    format!("label_{}", target / 4)
                }
            } else {
                format!("label_{}", target / 4)
            };
            match funct3 {
                0x0 => format!("beq {}, {}, {}", gpr_name(rs1), gpr_name(rs2), label),
                0x1 => format!("bne {}, {}, {}", gpr_name(rs1), gpr_name(rs2), label),
                0x4 => format!("blt {}, {}, {}", gpr_name(rs1), gpr_name(rs2), label),
                0x5 => format!("bge {}, {}, {}", gpr_name(rs1), gpr_name(rs2), label),
                _ => format!("unknown_branch 0x{:08x}", inst),
            }
        }
        0x73 => {
            // System instructions
            if inst == 0x00000073 {
                String::from("ecall")
            } else if inst == 0x00100073 {
                String::from("ebreak")
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
    disassemble_code_with_labels(code, None)
}

/// Disassemble a code buffer containing RISC-V instructions with label support.
///
/// # Arguments
///
/// * `code` - Binary code buffer to disassemble
/// * `labels` - Optional map of address -> label name
///
/// Returns a formatted string with labels printed at their addresses and
/// used in branch/jump instructions. Auto-generates indexed labels for
/// branch/jump targets if not provided.
pub fn disassemble_code_with_labels(code: &[u8], labels: Option<&BTreeMap<u32, String>>) -> String {
    let mut result = String::new();
    let mut offset = 0;

    // Build reverse map (address -> label) for efficient lookups
    let label_map = labels.map(|map| {
        let mut rev_map = BTreeMap::new();
        for (addr, name) in map.iter() {
            rev_map.insert(*addr, name.clone());
        }
        rev_map
    });

    // Collect all branch/jump targets to auto-generate labels
    let mut auto_labels = BTreeMap::new();
    let mut label_counter = 0;

    // First pass: identify all branch/jump targets
    let mut temp_offset = 0;
    while temp_offset + 4 <= code.len() {
        let inst_bytes = [
            code[temp_offset],
            code[temp_offset + 1],
            code[temp_offset + 2],
            code[temp_offset + 3],
        ];
        let inst = u32::from_le_bytes(inst_bytes);

        // Extract target addresses for branches and jumps
        let fields = super::decode::extract_fields(inst);
        match fields.opcode {
            0x6f => {
                // JAL
                let target = (temp_offset as u32).wrapping_add(fields.imm_j as u32);
                if label_map
                    .as_ref()
                    .map_or(true, |m| !m.contains_key(&target))
                {
                    auto_labels.entry(target).or_insert_with(|| {
                        label_counter += 1;
                        format!("label_{}", label_counter - 1)
                    });
                }
            }
            0x63 => {
                // Branch
                let target = (temp_offset as u32).wrapping_add(fields.imm_b as u32);
                if label_map
                    .as_ref()
                    .map_or(true, |m| !m.contains_key(&target))
                {
                    auto_labels.entry(target).or_insert_with(|| {
                        label_counter += 1;
                        format!("label_{}", label_counter - 1)
                    });
                }
            }
            _ => {}
        }

        temp_offset += 4;
    }

    // Merge provided labels with auto-generated ones
    let mut all_labels = BTreeMap::new();
    if let Some(provided) = labels {
        for (addr, name) in provided.iter() {
            all_labels.insert(*addr, name.clone());
        }
    }
    for (addr, name) in auto_labels.iter() {
        all_labels.entry(*addr).or_insert_with(|| name.clone());
    }

    // Build reverse map for instruction disassembly
    let rev_map = all_labels.clone();

    // Second pass: disassemble with labels
    while offset + 4 <= code.len() {
        let offset_u32 = offset as u32;
        // Check if there's a label at this address
        if let Some(label_name) = all_labels.get(&offset_u32) {
            result.push_str(&format!("{}:\n", label_name));
        }

        // Read 32-bit instruction (little-endian)
        let inst_bytes = [
            code[offset],
            code[offset + 1],
            code[offset + 2],
            code[offset + 3],
        ];
        let inst = u32::from_le_bytes(inst_bytes);

        let labels_for_inst = if labels.is_some() || !rev_map.is_empty() {
            Some((&all_labels, &rev_map))
        } else {
            None
        };
        let disasm = disassemble_instruction_with_labels(inst, offset_u32, labels_for_inst);
        result.push_str(&format!("0x{:04x}: {}\n", offset, disasm));

        offset += 4;
    }

    // Handle remaining bytes (if any)
    if offset < code.len() {
        let offset_u32 = offset as u32;
        if let Some(label_name) = all_labels.get(&offset_u32) {
            result.push_str(&format!("{}:\n", label_name));
        }
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
    use super::super::{encode::*, regs::Gpr};

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
        use alloc::vec::Vec;

        let mut code = Vec::new();
        code.extend_from_slice(&add(Gpr::A0, Gpr::A1, Gpr::A2).to_le_bytes());
        code.extend_from_slice(&addi(Gpr::A1, Gpr::A0, 10).to_le_bytes());
        code.extend_from_slice(&ecall().to_le_bytes());

        let disasm = disassemble_code(&code);
        assert!(disasm.contains("add a0, a1, a2"));
        assert!(disasm.contains("addi a1, a0, 10"));
        assert!(disasm.contains("ecall"));
    }

    #[test]
    fn test_disassemble_code_with_labels() {
        use alloc::{collections::BTreeMap, string::ToString, vec::Vec};

        let mut code = Vec::new();
        code.extend_from_slice(&addi(Gpr::A0, Gpr::Zero, 5).to_le_bytes());
        code.extend_from_slice(&addi(Gpr::A1, Gpr::Zero, 10).to_le_bytes());
        code.extend_from_slice(&beq(Gpr::A0, Gpr::A1, 8).to_le_bytes());
        code.extend_from_slice(&addi(Gpr::A0, Gpr::A0, 1).to_le_bytes());

        let labels = BTreeMap::from([(0x0008, "loop".to_string())]);
        let disasm = disassemble_code_with_labels(&code, Some(&labels));

        assert!(disasm.contains("loop:"));
        assert!(disasm.contains("beq"));
    }

    #[test]
    fn test_disassemble_code_auto_labels() {
        use alloc::vec::Vec;

        let mut code = Vec::new();
        code.extend_from_slice(&addi(Gpr::A0, Gpr::Zero, 5).to_le_bytes());
        code.extend_from_slice(&jal(Gpr::Ra, 8).to_le_bytes());
        code.extend_from_slice(&addi(Gpr::A0, Gpr::A0, 1).to_le_bytes());

        let disasm = disassemble_code_with_labels(&code, None);
        // Should auto-generate label_2 for the jal target
        assert!(disasm.contains("label_"));
        assert!(disasm.contains("jal"));
    }

    #[test]
    fn test_round_trip_assemble_disassemble() {
        use super::super::asm_parser::assemble_code;

        let asm = "addi a0, zero, 5\naddi a1, zero, 10\nadd a0, a0, a1\nebreak";
        let code = assemble_code(asm, None).unwrap();
        let disasm = disassemble_code(&code);

        // Check that all instructions are present
        assert!(disasm.contains("addi a0, zero, 5"));
        assert!(disasm.contains("addi a1, zero, 10"));
        assert!(disasm.contains("add a0, a0, a1"));
        assert!(disasm.contains("ebreak"));
    }

    #[test]
    fn test_round_trip_with_labels() {
        use alloc::{collections::BTreeMap, string::ToString};

        use super::super::asm_parser::assemble_code;

        let asm = "addi a0, zero, 5\nloop:\naddi a0, a0, 1\nbeq a0, a1, loop";
        let code = assemble_code(asm, None).unwrap();

        // Disassemble with labels
        let labels = BTreeMap::from([(0x0004, "loop".to_string())]);
        let disasm = disassemble_code_with_labels(&code, Some(&labels));

        assert!(disasm.contains("loop:"));
        assert!(disasm.contains("beq"));
    }
}
