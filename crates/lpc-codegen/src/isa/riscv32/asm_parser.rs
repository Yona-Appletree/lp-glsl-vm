//! RISC-V 32-bit instruction assembler.
//!
//! This module provides functions to parse RISC-V assembly text
//! and convert it to binary instructions.

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{char, multispace0, multispace1},
    combinator::{map, map_res, opt, recognize},
    sequence::{delimited, pair, preceded, separated_pair, terminated},
    IResult,
};

use crate::{encode::*, regs::Gpr};

/// Parse a register name.
fn parse_register(input: &str) -> IResult<&str, Gpr> {
    map_res(
        take_while1(|c: char| c.is_alphanumeric() || c == '_'),
        |s: &str| Gpr::from_name(s),
    )(input)
}

/// Parse an integer immediate (decimal or hex).
fn parse_immediate(input: &str) -> IResult<&str, i32> {
    alt((
        // Hex: 0x123 or -0x123
        // Try hex first to avoid matching "0" as decimal when followed by "x"
        map_res(
            recognize(pair(
                opt(char('-')),
                preceded(tag("0x"), take_while1(|c: char| c.is_ascii_hexdigit())),
            )),
            |s: &str| {
                let (sign, hex_part) = if s.starts_with('-') {
                    (-1, &s[3..])
                } else {
                    (1, &s[2..])
                };
                // Parse as u32 first to handle large values, then convert to i32
                u32::from_str_radix(hex_part, 16)
                    .map(|v| {
                        let signed = v as i32;
                        sign * signed
                    })
                    .or_else(|_| {
                        // If it doesn't fit in i32, try parsing as u32 and then casting
                        u32::from_str_radix(hex_part, 16).map(|v| v as i32)
                    })
            },
        ),
        // Decimal: 123 or -123
        map_res(
            recognize(pair(
                opt(char('-')),
                take_while1(|c: char| c.is_ascii_digit()),
            )),
            |s: &str| s.parse::<i32>(),
        ),
    ))(input)
}

/// Parse a label name (identifier).
fn parse_label(input: &str) -> IResult<&str, String> {
    map(
        take_while1(|c: char| c.is_alphanumeric() || c == '_'),
        |s: &str| s.to_string(),
    )(input)
}

/// Parse a label or immediate offset for branches/jumps.
fn parse_target(input: &str) -> IResult<&str, Target> {
    // Check if input starts with a digit or minus sign (immediate) or letter/underscore (label)
    let first_char = input.chars().next();
    if first_char.map_or(false, |c| c.is_ascii_digit() || c == '-') {
        // Parse as immediate
        map(parse_immediate, Target::Offset)(input)
    } else {
        // Parse as label
        map(parse_label, Target::Label)(input)
    }
}

/// Target for branches/jumps - either a label or numeric offset.
#[derive(Debug, Clone)]
enum Target {
    Label(String),
    Offset(i32),
}

/// Parse an R-type instruction: add, sub, mul
fn parse_r_type(input: &str) -> IResult<&str, u32> {
    // Parse the opcode manually to avoid type inference issues
    let op = if input.starts_with("add") && !input[3..].starts_with('i') {
        "add"
    } else if input.starts_with("sub") {
        "sub"
    } else if input.starts_with("mul") {
        "mul"
    } else {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    };
    let (input, _) = terminated(tag(op), multispace1)(input)?;
    let (input, rd) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs1) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs2) = parse_register(input)?;

    let inst = match op {
        "add" => add(rd, rs1, rs2),
        "sub" => sub(rd, rs1, rs2),
        "mul" => mul(rd, rs1, rs2),
        _ => unreachable!(),
    };

    Ok((input, inst))
}

/// Parse an I-type immediate instruction: addi
fn parse_i_type_imm(input: &str) -> IResult<&str, u32> {
    let (input, _) = terminated(tag("addi"), multispace1)(input)?;
    let (input, rd) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs1) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, imm) = parse_immediate(input)?;

    Ok((input, addi(rd, rs1, imm)))
}

/// Parse a load instruction: lw rd, imm(rs1)
fn parse_load(input: &str) -> IResult<&str, u32> {
    let (input, _) = terminated(tag("lw"), multispace1)(input)?;
    let (input, rd) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, (imm, rs1)) = delimited(
        opt(char('(')),
        separated_pair(parse_immediate, char('('), parse_register),
        char(')'),
    )(input)?;

    Ok((input, lw(rd, rs1, imm)))
}

/// Parse a store instruction: sw rs2, imm(rs1)
fn parse_store(input: &str) -> IResult<&str, u32> {
    let (input, _) = terminated(tag("sw"), multispace1)(input)?;
    let (input, rs2) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, (imm, rs1)) = delimited(
        opt(char('(')),
        separated_pair(parse_immediate, char('('), parse_register),
        char(')'),
    )(input)?;

    Ok((input, sw(rs1, rs2, imm)))
}

/// Parse a U-type instruction: lui, auipc
fn parse_u_type(input: &str) -> IResult<&str, u32> {
    // Parse the opcode manually to avoid type inference issues
    let op = if input.starts_with("lui") {
        "lui"
    } else if input.starts_with("auipc") {
        "auipc"
    } else {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    };
    let (input, _) = terminated(tag(op), multispace1)(input)?;
    let (input, rd) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, imm) = parse_immediate(input)?;

    // For lui/auipc, the immediate is the upper 20 bits
    // We accept either the full 32-bit value or the upper 20 bits
    // Cast i32 to u32 (bitwise cast, handles negative values correctly)
    let imm_u = imm as u32;

    let inst = match op {
        "lui" => lui(rd, imm_u),
        "auipc" => auipc(rd, imm_u),
        _ => unreachable!(),
    };

    Ok((input, inst))
}

/// Parse a J-type instruction: jal
fn parse_jal(input: &str) -> IResult<&str, u32> {
    let (input, _) = terminated(tag("jal"), multispace1)(input)?;
    let (input, rd) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, target) = parse_target(input)?;

    // For now, we only support numeric offsets in single-instruction parsing
    // Multi-line assembly will resolve labels
    let imm = match target {
        Target::Offset(off) => off,
        Target::Label(_) => {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }
    };

    Ok((input, jal(rd, imm)))
}

/// Parse a jalr instruction: jalr rd, imm(rs1)
fn parse_jalr(input: &str) -> IResult<&str, u32> {
    let (input, _) = terminated(tag("jalr"), multispace1)(input)?;
    let (input, rd) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, (imm, rs1)) = delimited(
        opt(char('(')),
        separated_pair(parse_immediate, char('('), parse_register),
        char(')'),
    )(input)?;

    Ok((input, jalr(rd, rs1, imm)))
}

/// Parse a branch instruction: beq, bne, blt, bge
fn parse_branch(input: &str) -> IResult<&str, u32> {
    // Parse the opcode manually to avoid type inference issues
    let op = if input.starts_with("beq") {
        "beq"
    } else if input.starts_with("bne") {
        "bne"
    } else if input.starts_with("blt") {
        "blt"
    } else if input.starts_with("bge") {
        "bge"
    } else {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    };
    let (input, _) = terminated(tag(op), multispace1)(input)?;
    let (input, rs1) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs2) = terminated(parse_register, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, target) = parse_target(input)?;

    // For now, we only support numeric offsets in single-instruction parsing
    let imm = match target {
        Target::Offset(off) => off,
        Target::Label(_) => {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }
    };

    let inst = match op {
        "beq" => beq(rs1, rs2, imm),
        "bne" => bne(rs1, rs2, imm),
        "blt" => blt(rs1, rs2, imm),
        "bge" => bge(rs1, rs2, imm),
        _ => unreachable!(),
    };

    Ok((input, inst))
}

/// Parse a system instruction: ecall, ebreak
fn parse_system(input: &str) -> IResult<&str, u32> {
    // Parse the opcode manually to avoid type inference issues
    let op = if input.starts_with("ecall") {
        "ecall"
    } else if input.starts_with("ebreak") {
        "ebreak"
    } else {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    };
    let (input, _) = terminated(tag(op), multispace0)(input)?;
    let inst = match op {
        "ecall" => ecall(),
        "ebreak" => ebreak(),
        _ => unreachable!(),
    };
    Ok((input, inst))
}

/// Parse a single instruction.
fn parse_instruction_internal(input: &str) -> IResult<&str, u32> {
    let (input, _) = multispace0(input)?;
    alt((
        parse_system,
        parse_branch,
        parse_jalr,
        parse_jal,
        parse_u_type,
        parse_store,
        parse_load,
        parse_i_type_imm,
        parse_r_type,
    ))(input)
}

/// Assemble a single instruction from assembly text.
///
/// # Arguments
///
/// * `asm` - Assembly text for a single instruction (e.g., "addi a0, zero, 5")
///
/// # Returns
///
/// The encoded 32-bit instruction word, or an error string if parsing fails.
pub fn assemble_instruction(asm: &str) -> Result<u32, String> {
    match parse_instruction_internal(asm) {
        Ok(("", inst)) => Ok(inst),
        Ok((remaining, _)) => Err(format!(
            "Unexpected text after instruction: '{}'",
            remaining
        )),
        Err(e) => Err(format!("Parse error: {:?}", e)),
    }
}

/// Assemble multi-line assembly code.
///
/// # Arguments
///
/// * `asm` - Multi-line assembly text
/// * `labels` - Optional map of label name -> address for resolving labels
///
/// # Returns
///
/// Binary code as a byte vector, or an error string if parsing fails.
///
/// # Label Resolution
///
/// Labels can be defined with `label_name:` on their own line, or provided
/// in the labels map. Branch/jump instructions can reference labels by name.
/// If a label is not found, an error is returned.
pub fn assemble_code(asm: &str, labels: Option<&BTreeMap<String, u32>>) -> Result<Vec<u8>, String> {
    let mut code = Vec::new();
    let mut current_addr = 0u32;
    let mut label_map: BTreeMap<String, u32> = labels.cloned().unwrap_or_default();

    // First pass: collect label definitions and calculate addresses
    for line in asm.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Check for label definition: "label_name:"
        if let Some(label_end) = line.find(':') {
            let label_name = line[..label_end].trim().to_string();
            if !label_name.is_empty() {
                label_map.insert(label_name, current_addr);
            }
            // If there's code after the label, we'll handle it in the second pass
            let remaining = line[label_end + 1..].trim();
            if !remaining.is_empty() && !remaining.starts_with('#') {
                // Instruction on same line as label
                current_addr += 4;
            }
        } else {
            // Regular instruction
            current_addr += 4;
        }
    }

    // Second pass: assemble instructions
    current_addr = 0;
    for line in asm.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Check for label definition
        if let Some(label_end) = line.find(':') {
            let remaining = line[label_end + 1..].trim();
            if remaining.is_empty() || remaining.starts_with('#') {
                // Label on its own line, continue
                continue;
            }
            // Instruction on same line as label - parse it
            match parse_instruction_with_labels(remaining, current_addr, &label_map) {
                Ok(inst) => {
                    code.extend_from_slice(&inst.to_le_bytes());
                    current_addr += 4;
                }
                Err(e) => return Err(format!("Error at address 0x{:04x}: {}", current_addr, e)),
            }
        } else {
            // Regular instruction
            match parse_instruction_with_labels(line, current_addr, &label_map) {
                Ok(inst) => {
                    code.extend_from_slice(&inst.to_le_bytes());
                    current_addr += 4;
                }
                Err(e) => return Err(format!("Error at address 0x{:04x}: {}", current_addr, e)),
            }
        }
    }

    Ok(code)
}

/// Parse an instruction with label resolution.
fn parse_instruction_with_labels(
    input: &str,
    pc: u32,
    labels: &BTreeMap<String, u32>,
) -> Result<u32, String> {
    let input = input.trim();

    // Check instruction type by looking at first word
    let first_word_end = input.find(char::is_whitespace).unwrap_or(input.len());
    let op = &input[..first_word_end];

    match op {
        "beq" | "bne" | "blt" | "bge" => parse_branch_with_labels(input, pc, labels),
        "jal" => parse_jal_with_labels(input, pc, labels),
        _ => {
            // For other instructions, use the regular parser
            assemble_instruction(input)
        }
    }
}

/// Parse a branch instruction with label resolution.
fn parse_branch_with_labels(
    input: &str,
    pc: u32,
    labels: &BTreeMap<String, u32>,
) -> Result<u32, String> {
    // Parse the opcode manually to avoid type inference issues
    let op = if input.starts_with("beq") {
        "beq"
    } else if input.starts_with("bne") {
        "bne"
    } else if input.starts_with("blt") {
        "blt"
    } else if input.starts_with("bge") {
        "bge"
    } else {
        return Err(format!("Expected branch instruction"));
    };
    let (input, _) = terminated(tag(op), multispace1)(input)
        .map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?;
    let (input, rs1) = terminated(parse_register, opt(char(',')))(input)
        .map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?;
    let input = multispace0(input)
        .map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?
        .0;
    let (input, rs2) = terminated(parse_register, opt(char(',')))(input)
        .map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?;
    let input = multispace0(input)
        .map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?
        .0;
    let (_input, target) =
        parse_target(input).map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?;

    let imm = match target {
        Target::Offset(off) => off,
        Target::Label(name) => {
            let target_addr = labels
                .get(&name)
                .ok_or_else(|| format!("Unknown label: {}", name))?;
            // Calculate PC-relative offset
            let offset = (*target_addr as i32) - (pc as i32);
            offset
        }
    };

    let inst = match op {
        "beq" => beq(rs1, rs2, imm),
        "bne" => bne(rs1, rs2, imm),
        "blt" => blt(rs1, rs2, imm),
        "bge" => bge(rs1, rs2, imm),
        _ => unreachable!(),
    };

    Ok(inst)
}

/// Parse a jal instruction with label resolution.
fn parse_jal_with_labels(
    input: &str,
    pc: u32,
    labels: &BTreeMap<String, u32>,
) -> Result<u32, String> {
    let (input, _) = terminated(tag("jal"), multispace1)(input)
        .map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?;
    let (input, rd) = terminated(parse_register, opt(char(',')))(input)
        .map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?;
    let input = multispace0(input)
        .map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?
        .0;
    let (_input, target) =
        parse_target(input).map_err(|e: nom::Err<nom::error::Error<&str>>| format!("{:?}", e))?;

    let imm = match target {
        Target::Offset(off) => off,
        Target::Label(name) => {
            let target_addr = labels
                .get(&name)
                .ok_or_else(|| format!("Unknown label: {}", name))?;
            // Calculate PC-relative offset
            let offset = (*target_addr as i32) - (pc as i32);
            offset
        }
    };

    Ok(jal(rd, imm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemble_add() {
        let inst = assemble_instruction("add a0, a1, a2").unwrap();
        assert_eq!(inst, add(Gpr::A0, Gpr::A1, Gpr::A2));
    }

    #[test]
    fn test_assemble_addi() {
        let inst = assemble_instruction("addi a0, zero, 5").unwrap();
        assert_eq!(inst, addi(Gpr::A0, Gpr::Zero, 5));
    }

    #[test]
    fn test_assemble_addi_negative() {
        let inst = assemble_instruction("addi a0, zero, -5").unwrap();
        assert_eq!(inst, addi(Gpr::A0, Gpr::Zero, -5));
    }

    #[test]
    fn test_assemble_lw() {
        let inst = assemble_instruction("lw a0, 4(a1)").unwrap();
        assert_eq!(inst, lw(Gpr::A0, Gpr::A1, 4));
    }

    #[test]
    fn test_assemble_sw() {
        let inst = assemble_instruction("sw a0, 4(a1)").unwrap();
        assert_eq!(inst, sw(Gpr::A1, Gpr::A0, 4));
    }

    #[test]
    fn test_assemble_beq() {
        let inst = assemble_instruction("beq a0, a1, 8").unwrap();
        assert_eq!(inst, beq(Gpr::A0, Gpr::A1, 8));
    }

    #[test]
    fn test_assemble_jal() {
        let inst = assemble_instruction("jal ra, 16").unwrap();
        assert_eq!(inst, jal(Gpr::Ra, 16));
    }

    #[test]
    fn test_assemble_ecall() {
        let inst = assemble_instruction("ecall").unwrap();
        assert_eq!(inst, ecall());
    }

    #[test]
    fn test_assemble_code() {
        let asm = "addi a0, zero, 5\naddi a1, zero, 10\nadd a0, a0, a1\nebreak";
        let code = assemble_code(asm, None).unwrap();
        assert_eq!(code.len(), 16); // 4 instructions * 4 bytes
    }

    #[test]
    fn test_assemble_code_with_labels() {
        use alloc::collections::BTreeMap;
        let asm = "addi a0, zero, 5\nloop:\naddi a0, a0, 1\nbeq a0, a1, loop";
        let labels = BTreeMap::from([("loop".to_string(), 0x0008)]);
        let code = assemble_code(asm, Some(&labels)).unwrap();
        assert_eq!(code.len(), 12); // 3 instructions: addi, addi, beq (label on own line doesn't generate code)
    }

    #[test]
    fn test_assemble_sub() {
        let inst = assemble_instruction("sub a0, a1, a2").unwrap();
        assert_eq!(inst, sub(Gpr::A0, Gpr::A1, Gpr::A2));
    }

    #[test]
    fn test_assemble_mul() {
        let inst = assemble_instruction("mul a0, a1, a2").unwrap();
        assert_eq!(inst, mul(Gpr::A0, Gpr::A1, Gpr::A2));
    }

    #[test]
    fn test_assemble_lui() {
        // lui takes the upper 20 bits, so 0x12345 means 0x12345000
        let inst = assemble_instruction("lui a0, 0x12345000").unwrap();
        assert_eq!(inst, lui(Gpr::A0, 0x12345000));
    }

    #[test]
    fn test_assemble_bne() {
        let inst = assemble_instruction("bne a0, a1, 8").unwrap();
        assert_eq!(inst, bne(Gpr::A0, Gpr::A1, 8));
    }

    #[test]
    fn test_assemble_blt() {
        let inst = assemble_instruction("blt a0, a1, -4").unwrap();
        assert_eq!(inst, blt(Gpr::A0, Gpr::A1, -4));
    }

    #[test]
    fn test_assemble_bge() {
        let inst = assemble_instruction("bge a0, a1, 12").unwrap();
        assert_eq!(inst, bge(Gpr::A0, Gpr::A1, 12));
    }

    #[test]
    fn test_assemble_jalr() {
        let inst = assemble_instruction("jalr ra, 0(a0)").unwrap();
        assert_eq!(inst, jalr(Gpr::Ra, Gpr::A0, 0));
    }

    #[test]
    fn test_assemble_ebreak() {
        let inst = assemble_instruction("ebreak").unwrap();
        assert_eq!(inst, ebreak());
    }

    #[test]
    fn test_assemble_hex_immediate() {
        let inst = assemble_instruction("addi a0, zero, 0x10").unwrap();
        assert_eq!(inst, addi(Gpr::A0, Gpr::Zero, 16));
    }

    #[test]
    fn test_assemble_code_labels_inline() {
        let asm = "loop:\naddi a0, a0, 1\nbeq a0, a1, loop";
        let code = assemble_code(asm, None).unwrap();
        assert_eq!(code.len(), 8); // 2 instructions: addi, beq (label on own line doesn't generate code)
    }
}
