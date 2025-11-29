//! VCode text parser for RISC-V 32-bit backend3

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{char, multispace0, multispace1},
    combinator::{map, map_res, opt, recognize},
    multi::separated_list0,
    sequence::{delimited, pair, preceded, separated_pair, terminated, tuple},
    IResult,
};

use crate::backend3::{
    types::{BlockIndex, Range, Ranges, VReg, Writable},
    vcode::{BlockLoweringOrder, Callee, LoweredBlock, VCode},
    vcode_builder::VCodeBuilder,
};
use crate::isa::riscv32::backend3::inst::{Riscv32ABI, Riscv32MachInst};
use lpc_lpir::RelSourceLoc;

/// Parse a VReg identifier (e.g., "v0", "v1")
fn parse_vreg(input: &str) -> IResult<&str, VReg> {
    map_res(
        recognize(pair(char('v'), take_while1(|c: char| c.is_ascii_digit()))),
        |s: &str| -> Result<VReg, alloc::string::String> {
            let num = s[1..].parse::<u32>()
                .map_err(|_| format!("Invalid VReg number: {}", s))?;
            Ok(VReg::new(num))
        },
    )(input)
}

/// Parse a BlockIndex identifier (e.g., "block0", "block1")
fn parse_block_index(input: &str) -> IResult<&str, BlockIndex> {
    map_res(
        recognize(pair(tag("block"), take_while1(|c: char| c.is_ascii_digit()))),
        |s: &str| -> Result<BlockIndex, alloc::string::String> {
            let num = s[5..].parse::<u32>()
                .map_err(|_| format!("Invalid block number: {}", s))?;
            Ok(BlockIndex::new(num))
        },
    )(input)
}

/// Parse an integer immediate (decimal or hex)
fn parse_immediate(input: &str) -> IResult<&str, i32> {
    alt((
        // Hex: 0x123 or -0x123
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
                u32::from_str_radix(hex_part, 16)
                    .map(|v| sign * (v as i32))
                    .map_err(|_| format!("Invalid hex number: {}", s))
            },
        ),
        // Decimal: 123 or -123
        map_res(
            recognize(pair(opt(char('-')), take_while1(|c: char| c.is_ascii_digit()))),
            |s: &str| s.parse::<i32>().map_err(|_| format!("Invalid number: {}", s)),
        ),
    ))(input)
}

/// Parse an ADD instruction: add v0, v1, v2
fn parse_add(input: &str) -> IResult<&str, Riscv32MachInst> {
    let (input, _) = terminated(tag("add"), multispace1)(input)?;
    let (input, rd) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs1) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs2) = parse_vreg(input)?;

    Ok((
        input,
        Riscv32MachInst::Add {
            rd: Writable::new(rd),
            rs1,
            rs2,
        },
    ))
}

/// Parse an ADDI instruction: addi v0, v1, 42
fn parse_addi(input: &str) -> IResult<&str, Riscv32MachInst> {
    let (input, _) = terminated(tag("addi"), multispace1)(input)?;
    let (input, rd) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs1) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, imm) = parse_immediate(input)?;

    Ok((
        input,
        Riscv32MachInst::Addi {
            rd: Writable::new(rd),
            rs1,
            imm,
        },
    ))
}

/// Parse a SUB instruction: sub v0, v1, v2
fn parse_sub(input: &str) -> IResult<&str, Riscv32MachInst> {
    let (input, _) = terminated(tag("sub"), multispace1)(input)?;
    let (input, rd) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs1) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs2) = parse_vreg(input)?;

    Ok((
        input,
        Riscv32MachInst::Sub {
            rd: Writable::new(rd),
            rs1,
            rs2,
        },
    ))
}

/// Parse a LUI instruction: lui v0, 0x12345
fn parse_lui(input: &str) -> IResult<&str, Riscv32MachInst> {
    let (input, _) = terminated(tag("lui"), multispace1)(input)?;
    let (input, rd) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, imm) = parse_immediate(input)?;

    Ok((
        input,
        Riscv32MachInst::Lui {
            rd: Writable::new(rd),
            imm: imm as u32, // Cast i32 to u32 (bitwise cast)
        },
    ))
}

/// Parse a LW instruction: lw v0, 4(v1)
fn parse_lw(input: &str) -> IResult<&str, Riscv32MachInst> {
    let (input, _) = terminated(tag("lw"), multispace1)(input)?;
    let (input, rd) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, (imm, rs1)) = delimited(
        opt(char('(')),
        separated_pair(parse_immediate, char('('), parse_vreg),
        char(')'),
    )(input)?;

    Ok((
        input,
        Riscv32MachInst::Lw {
            rd: Writable::new(rd),
            rs1,
            imm,
        },
    ))
}

/// Parse a SW instruction: sw v1, 4(v0)
fn parse_sw(input: &str) -> IResult<&str, Riscv32MachInst> {
    let (input, _) = terminated(tag("sw"), multispace1)(input)?;
    let (input, rs2) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, (imm, rs1)) = delimited(
        opt(char('(')),
        separated_pair(parse_immediate, char('('), parse_vreg),
        char(')'),
    )(input)?;

    Ok((
        input,
        Riscv32MachInst::Sw { rs1, rs2, imm },
    ))
}

/// Parse a Move instruction: move v0, v1
fn parse_move(input: &str) -> IResult<&str, Riscv32MachInst> {
    let (input, _) = terminated(tag("move"), multispace1)(input)?;
    let (input, rd) = terminated(parse_vreg, opt(char(',')))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, rs) = parse_vreg(input)?;

    Ok((
        input,
        Riscv32MachInst::Move {
            rd: Writable::new(rd),
            rs,
        },
    ))
}

/// Parse a single instruction
fn parse_instruction(input: &str) -> IResult<&str, Riscv32MachInst> {
    let (input, _) = multispace0(input)?;
    alt((
        parse_move,
        parse_sw,
        parse_lw,
        parse_lui,
        parse_addi,
        parse_sub,
        parse_add,
    ))(input)
}

/// Parse a branch: br block1 or br block1(v2)
fn parse_branch(input: &str) -> IResult<&str, (BlockIndex, Vec<VReg>)> {
    let (input, _) = terminated(tag("br"), multispace1)(input)?;
    let (input, target) = parse_block_index(input)?;
    let (input, args) = opt(delimited(
        char('('),
        separated_list0(char(','), preceded(multispace0, parse_vreg)),
        char(')'),
    ))(input)?;

    Ok((input, (target, args.unwrap_or_default())))
}

/// Parse a block header: block0: or block0(v0, v1):
fn parse_block_header(input: &str) -> IResult<&str, (BlockIndex, Vec<VReg>)> {
    let (input, _) = multispace0(input)?;
    let (input, block_idx) = parse_block_index(input)?;
    let (input, params) = opt(delimited(
        char('('),
        separated_list0(char(','), preceded(multispace0, parse_vreg)),
        char(')'),
    ))(input)?;
    let (input, _) = char(':')(input)?;

    Ok((input, (block_idx, params.unwrap_or_default())))
}

/// Parse an edge block header: edge block1 -> block2:
fn parse_edge_block_header(input: &str) -> IResult<&str, (BlockIndex, BlockIndex)> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("edge")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, from) = parse_block_index(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("->")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, to) = parse_block_index(input)?;
    let (input, _) = char(':')(input)?;

    Ok((input, (from, to)))
}

/// Parse a block (regular or edge)
fn parse_block(input: &str) -> IResult<&str, ParsedBlock> {
    let (input, is_edge) = opt(tag("edge"))(input)?;
    if is_edge.is_some() {
        let (input, (from, to)) = parse_edge_block_header(input)?;
        let (input, _) = multispace0(input)?;
        let (input, insts) = separated_list0(
            multispace1,
            terminated(parse_instruction, opt(multispace0)),
        )(input)?;
        let (input, branches) = separated_list0(
            multispace1,
            terminated(parse_branch, opt(multispace0)),
        )(input)?;

        Ok((
            input,
            ParsedBlock::Edge {
                from,
                to,
                insts,
                branches,
            },
        ))
    } else {
        let (input, (block_idx, params)) = parse_block_header(input)?;
        let (input, _) = multispace0(input)?;
        let (input, insts) = separated_list0(
            multispace1,
            terminated(parse_instruction, opt(multispace0)),
        )(input)?;
        let (input, branches) = separated_list0(
            multispace1,
            terminated(parse_branch, opt(multispace0)),
        )(input)?;

        Ok((
            input,
            ParsedBlock::Regular {
                block_idx,
                params,
                insts,
                branches,
            },
        ))
    }
}

/// Parsed block structure
enum ParsedBlock {
    Regular {
        block_idx: BlockIndex,
        params: Vec<VReg>,
        insts: Vec<Riscv32MachInst>,
        branches: Vec<(BlockIndex, Vec<VReg>)>,
    },
    Edge {
        from: BlockIndex,
        to: BlockIndex,
        insts: Vec<Riscv32MachInst>,
        branches: Vec<(BlockIndex, Vec<VReg>)>,
    },
}

/// Parse VCode text format
pub fn parse_vcode(text: &str) -> Result<VCode<Riscv32MachInst>, String> {
    let parse_result = tuple((
        tag("vcode"),
        multispace0,
        char('{'),
        multispace0,
        tag("entry:"),
        multispace1,
        parse_block_index,
        multispace0,
        separated_list0(
            multispace1,
            terminated(parse_block, opt(multispace0)),
        ),
        multispace0,
        char('}'),
        multispace0,
    ))(text.trim());

    let (remaining, (_, _, _, _, _, _, entry, _, blocks, _, _, _)) = parse_result
        .map_err(|e: nom::Err<nom::error::Error<&str>>| format!("Parse error: {:?}", e))?;

    if !remaining.is_empty() {
        return Err(format!("Unexpected text after vcode: '{}'", remaining));
    }

    // Build VCode from parsed blocks
    build_vcode(entry, blocks)
}

/// Build VCode from parsed blocks
fn build_vcode(
    entry: BlockIndex,
    blocks: Vec<ParsedBlock>,
) -> Result<VCode<Riscv32MachInst>, String> {
    let mut builder = VCodeBuilder::new();
    let mut block_order = BlockLoweringOrder {
        lowered_order: Vec::new(),
        lowered_succs: Vec::new(),
        block_to_index: BTreeMap::new(),
        cold_blocks: alloc::collections::BTreeSet::new(),
        indirect_targets: alloc::collections::BTreeSet::new(),
    };

    // Build blocks and collect instructions
    for block in blocks {
        match block {
            ParsedBlock::Regular {
                block_idx,
                params,
                insts,
                branches,
            } => {
                // Start block
                builder.start_block(block_idx, params.clone());

                // Add instructions
                let srcloc = RelSourceLoc::new(0);
                for inst in insts {
                    builder.push(inst, srcloc);
                }

                // Record branches (successors)
                let mut succs = Vec::new();
                let mut args_per_succ = Vec::new();

                for (target, args) in branches {
                    succs.push(target);
                    args_per_succ.push(args);
                }

                builder.end_block();
                builder.add_branch_args(&succs, &args_per_succ);

                // Add to block order
                block_order.lowered_order.push(LoweredBlock::Orig {
                    block: lpc_lpir::BlockEntity::new(block_idx.index()),
                });
                block_order.lowered_succs.push(succs);
            }
            ParsedBlock::Edge {
                from,
                to,
                insts,
                branches,
            } => {
                // Edge blocks are handled similarly but marked as edge blocks
                let edge_block_idx = BlockIndex::new(block_order.lowered_order.len() as u32);
                builder.start_block(edge_block_idx, Vec::new());

                let srcloc = RelSourceLoc::new(0);
                for inst in insts {
                    builder.push(inst, srcloc);
                }

                let mut succs = Vec::new();
                let mut args_per_succ = Vec::new();

                for (target, args) in branches {
                    succs.push(target);
                    args_per_succ.push(args);
                }

                builder.end_block();
                builder.add_branch_args(&succs, &args_per_succ);

                block_order.lowered_order.push(LoweredBlock::Edge {
                    from: lpc_lpir::BlockEntity::new(from.index()),
                    to: lpc_lpir::BlockEntity::new(to.index()),
                    succ_idx: 0, // Simplified - would need proper tracking
                });
                block_order.lowered_succs.push(succs);
            }
        }
    }

    let abi = Callee { abi: Riscv32ABI };
    Ok(builder.build(entry, block_order, abi))
}

