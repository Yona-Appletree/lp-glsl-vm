//! VCode text formatting for RISC-V 32-bit backend3

use alloc::{format, vec::Vec};
use core::fmt;

use crate::{
    backend3::{
        types::{BlockIndex, Range},
        vcode::{LoweredBlock, VCode},
    },
    isa::riscv32::backend3::inst::Riscv32MachInst,
};

impl fmt::Display for Riscv32MachInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Riscv32MachInst::Add { rd, rs1, rs2 } => {
                write!(f, "add {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Addi { rd, rs1, imm } => {
                write!(f, "addi {}, {}, {}", rd, rs1, imm)
            }
            Riscv32MachInst::Sub { rd, rs1, rs2 } => {
                write!(f, "sub {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Lui { rd, imm } => {
                write!(f, "lui {}, 0x{:x}", rd, imm)
            }
            Riscv32MachInst::Lw { rd, rs1, imm } => {
                write!(f, "lw {}, {}({})", rd, imm, rs1)
            }
            Riscv32MachInst::Sw { rs1, rs2, imm } => {
                write!(f, "sw {}, {}({})", rs2, imm, rs1)
            }
            Riscv32MachInst::Move { rd, rs } => {
                write!(f, "move {}, {}", rd, rs)
            }
            Riscv32MachInst::Return { ret_vals } => {
                if ret_vals.is_empty() {
                    write!(f, "return")
                } else {
                    write!(
                        f,
                        "return {}",
                        ret_vals
                            .iter()
                            .map(|v| format!("{}", v))
                            .collect::<alloc::vec::Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Riscv32MachInst::Mul { rd, rs1, rs2 } => {
                write!(f, "mul {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Mulh { rd, rs1, rs2 } => {
                write!(f, "mulh {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Div { rd, rs1, rs2 } => {
                write!(f, "div {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Rem { rd, rs1, rs2 } => {
                write!(f, "rem {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Slt { rd, rs1, rs2 } => {
                write!(f, "slt {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Sltiu { rd, rs1, imm } => {
                write!(f, "sltiu {}, {}, {}", rd, rs1, imm)
            }
            Riscv32MachInst::Sltu { rd, rs1, rs2 } => {
                write!(f, "sltu {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Xori { rd, rs1, imm } => {
                write!(f, "xori {}, {}, {}", rd, rs1, imm)
            }
            Riscv32MachInst::And { rd, rs1, rs2 } => {
                write!(f, "and {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Andi { rd, rs1, imm } => {
                write!(f, "andi {}, {}, {}", rd, rs1, imm)
            }
            Riscv32MachInst::Or { rd, rs1, rs2 } => {
                write!(f, "or {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Ori { rd, rs1, imm } => {
                write!(f, "ori {}, {}, {}", rd, rs1, imm)
            }
            Riscv32MachInst::Xor { rd, rs1, rs2 } => {
                write!(f, "xor {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Sll { rd, rs1, rs2 } => {
                write!(f, "sll {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Slli { rd, rs1, imm } => {
                write!(f, "slli {}, {}, {}", rd, rs1, imm)
            }
            Riscv32MachInst::Srl { rd, rs1, rs2 } => {
                write!(f, "srl {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Srli { rd, rs1, imm } => {
                write!(f, "srli {}, {}, {}", rd, rs1, imm)
            }
            Riscv32MachInst::Sra { rd, rs1, rs2 } => {
                write!(f, "sra {}, {}, {}", rd, rs1, rs2)
            }
            Riscv32MachInst::Srai { rd, rs1, imm } => {
                write!(f, "srai {}, {}, {}", rd, rs1, imm)
            }
            Riscv32MachInst::Jal {
                rd,
                callee,
                args,
                return_count: _,
            } => {
                write!(
                    f,
                    "jal {}, {}({})",
                    rd,
                    callee,
                    args.iter()
                        .map(|v| format!("{}", v))
                        .collect::<alloc::vec::Vec<_>>()
                        .join(", ")
                )
            }
            Riscv32MachInst::Ecall {
                number,
                args,
                result,
            } => {
                let args_str = args
                    .iter()
                    .map(|v| format!("{}", v))
                    .collect::<alloc::vec::Vec<_>>()
                    .join(", ");
                if let Some(result) = result {
                    write!(f, "ecall {}, {}({})", result, number, args_str)
                } else {
                    write!(f, "ecall {}({})", number, args_str)
                }
            }
            Riscv32MachInst::Ebreak => {
                write!(f, "ebreak")
            }
            Riscv32MachInst::Trap { code } => {
                write!(f, "trap {}", code)
            }
            Riscv32MachInst::Trapz { condition, code } => {
                write!(f, "trapz {}, {}", condition, code)
            }
            Riscv32MachInst::Trapnz { condition, code } => {
                write!(f, "trapnz {}, {}", condition, code)
            }
            Riscv32MachInst::Br { condition } => {
                write!(f, "brif {}", condition)
            }
            Riscv32MachInst::Jump => {
                write!(f, "jump")
            }
            Riscv32MachInst::Args { args } => {
                write!(
                    f,
                    "args {}",
                    args.iter()
                        .map(|ap| format!("{}->preg{}", ap.vreg, ap.preg.hw_enc()))
                        .collect::<alloc::vec::Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

impl VCode<Riscv32MachInst> {
    /// Format a BlockIndex with edge block information if applicable
    fn format_block_index(&self, block_idx: BlockIndex) -> alloc::string::String {
        let idx = block_idx.index();
        if idx < self.block_order.lowered_order.len() {
            match &self.block_order.lowered_order[idx] {
                LoweredBlock::Edge { from, to, .. } => {
                    // Format as block5_edge_1_3 (where 1 and 3 are IR block indices)
                    format!("block{}_edge_{}_{}", idx, from.index(), to.index())
                }
                LoweredBlock::Orig { .. } => {
                    format!("block{}", idx)
                }
            }
        } else {
            format!("block{}", idx)
        }
    }
}

impl fmt::Display for VCode<Riscv32MachInst> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "vcode {{")?;
        writeln!(f, "  entry: {}", self.format_block_index(self.entry))?;
        writeln!(f)?;

        // Iterate through blocks in lowering order
        for (block_idx, lowered_block) in self.block_order.lowered_order.iter().enumerate() {
            let block_index = BlockIndex::new(block_idx);

            // Format block header
            match lowered_block {
                LoweredBlock::Orig { .. } => {
                    // Get block parameters
                    let param_range = self
                        .block_params_range
                        .get(block_idx)
                        .unwrap_or_else(|| Range::new(0, 0));
                    let params: Vec<regalloc2::VReg> =
                        self.block_params[param_range.start..param_range.end].to_vec();

                    if params.is_empty() {
                        write!(f, "  block{}:\n", block_index.index())?;
                    } else {
                        write!(f, "  block{}(", block_index.index())?;
                        for (i, param) in params.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            // Convert VReg to Reg for display
                            let reg = crate::backend3::types::Reg::from_virtual_reg(*param);
                            write!(f, "{}", reg)?;
                        }
                        write!(f, "):\n")?;
                    }
                }
                LoweredBlock::Edge { from, to, .. } => {
                    // For edge blocks, format with descriptive name
                    write!(f, "  {}:\n", self.format_block_index(block_index))?;
                }
            }

            // Get instruction range for this block
            let inst_range = self
                .block_ranges
                .get(block_idx)
                .unwrap_or_else(|| Range::new(0, 0));

            // Get successors and branch arguments
            let succ_range = self
                .block_succ_range
                .get(block_idx)
                .unwrap_or_else(|| Range::new(0, 0));
            let succs: Vec<BlockIndex> =
                self.block_succs[succ_range.start..succ_range.end].to_vec();

            // Check if last instruction is a branch/jump
            let last_inst_is_branch = if !inst_range.is_empty() && !succs.is_empty() {
                let last_inst_idx = inst_range.end - 1;
                matches!(
                    &self.insts[last_inst_idx],
                    Riscv32MachInst::Br { .. } | Riscv32MachInst::Jump
                )
            } else {
                false
            };

            // Format instructions in this block
            for inst_idx in inst_range.start..inst_range.end {
                let inst = &self.insts[inst_idx];
                let is_last = inst_idx == inst_range.end - 1;

                // Skip Args instruction in entry block (it's an implementation detail)
                if matches!(inst, Riscv32MachInst::Args { .. }) && block_index == self.entry {
                    continue;
                }

                // If this is the last instruction and it's a branch, format it with successors
                if is_last && last_inst_is_branch {
                    match inst {
                        Riscv32MachInst::Br { condition } => {
                            write!(f, "    brif {}", condition)?;
                        }
                        Riscv32MachInst::Jump => {
                            write!(f, "    jump")?;
                        }
                        _ => {
                            writeln!(f, "    {}", inst)?;
                        }
                    }
                } else {
                    writeln!(f, "    {}", inst)?;
                }
            }

            // Format successors and branch arguments
            if !succs.is_empty() {
                // Get branch argument ranges for this block's successors
                let branch_arg_succ_range = self
                    .branch_block_arg_succ_range
                    .get(block_idx)
                    .unwrap_or_else(|| Range::new(0, 0));

                // Check if this is an edge block (edge blocks always jump to their target)
                let is_edge_block = matches!(lowered_block, LoweredBlock::Edge { .. });

                // If last instruction was a branch, format successors on the same line
                if last_inst_is_branch {
                    // Check if it's a jump (single successor) or brif (multiple successors)
                    let is_jump = matches!(&self.insts[inst_range.end - 1], Riscv32MachInst::Jump);

                    for (succ_idx, succ) in succs.iter().enumerate() {
                        // Get branch arguments for this successor
                        let arg_range_idx = branch_arg_succ_range.start + succ_idx;
                        let arg_range = if arg_range_idx < self.branch_block_arg_range.len() {
                            self.branch_block_arg_range.get(arg_range_idx)
                        } else {
                            None
                        };

                        let args: Vec<regalloc2::VReg> = if let Some(arg_range) = arg_range {
                            self.branch_block_args[arg_range.start..arg_range.end].to_vec()
                        } else {
                            Vec::new()
                        };

                        // For jump: no comma (just "jump block1")
                        // For brif: comma after condition, then comma-separated targets
                        if succ_idx > 0 || !is_jump {
                            write!(f, ", ")?;
                        } else {
                            write!(f, " ")?;
                        }
                        write!(f, "{}", self.format_block_index(*succ))?;
                        if !args.is_empty() {
                            write!(f, "(")?;
                            for (i, arg) in args.iter().enumerate() {
                                if i > 0 {
                                    write!(f, ", ")?;
                                }
                                // Convert VReg to Reg for display
                                let reg = crate::backend3::types::Reg::from_virtual_reg(*arg);
                                write!(f, "{}", reg)?;
                            }
                            write!(f, ")")?;
                        }
                    }
                    writeln!(f)?;
                } else if is_edge_block && succs.len() == 1 {
                    // Edge blocks with single successor: format as "jump blockX"
                    let succ = &succs[0];
                    let arg_range_idx = branch_arg_succ_range.start;
                    let arg_range = if arg_range_idx < self.branch_block_arg_range.len() {
                        self.branch_block_arg_range.get(arg_range_idx)
                    } else {
                        None
                    };

                    let args: Vec<regalloc2::VReg> = if let Some(arg_range) = arg_range {
                        self.branch_block_args[arg_range.start..arg_range.end].to_vec()
                    } else {
                        Vec::new()
                    };

                    write!(f, "    jump {}", self.format_block_index(*succ))?;
                    if !args.is_empty() {
                        write!(f, "(")?;
                        for (i, arg) in args.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            // Convert VReg to Reg for display
                            let reg = crate::backend3::types::Reg::from_virtual_reg(*arg);
                            write!(f, "{}", reg)?;
                        }
                        write!(f, ")")?;
                    }
                    writeln!(f)?;
                } else {
                    // Format successors separately (for blocks without branch instructions)
                    for (succ_idx, succ) in succs.iter().enumerate() {
                        let arg_range_idx = branch_arg_succ_range.start + succ_idx;
                        let arg_range = if arg_range_idx < self.branch_block_arg_range.len() {
                            self.branch_block_arg_range.get(arg_range_idx)
                        } else {
                            None
                        };

                        let args: Vec<regalloc2::VReg> = if let Some(arg_range) = arg_range {
                            self.branch_block_args[arg_range.start..arg_range.end].to_vec()
                        } else {
                            Vec::new()
                        };

                        if args.is_empty() {
                            writeln!(f, "    br {}", self.format_block_index(*succ))?;
                        } else {
                            write!(f, "    br {}(", self.format_block_index(*succ))?;
                            for (i, arg) in args.iter().enumerate() {
                                if i > 0 {
                                    write!(f, ", ")?;
                                }
                                // Convert VReg to Reg for display
                                let reg = crate::backend3::types::Reg::from_virtual_reg(*arg);
                                write!(f, "{}", reg)?;
                            }
                            writeln!(f, ")")?;
                        }
                    }
                }
            }

            writeln!(f)?;
        }

        writeln!(f, "}}")?;
        Ok(())
    }
}
