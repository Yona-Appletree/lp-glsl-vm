//! VCode text formatting for RISC-V 32-bit backend3

use alloc::{format, vec::Vec};
use core::fmt;

use crate::backend3::{
    types::{BlockIndex, Range, VReg},
    vcode::{LoweredBlock, VCode},
};
use crate::isa::riscv32::backend3::inst::Riscv32MachInst;

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
                    write!(f, "return {}", ret_vals.iter().map(|v| format!("{}", v)).collect::<alloc::vec::Vec<_>>().join(", "))
                }
            }
            Riscv32MachInst::Mul { rd, rs1, rs2 } => {
                write!(f, "mul {}, {}, {}", rd, rs1, rs2)
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
            Riscv32MachInst::Jal { rd, callee, args } => {
                write!(f, "jal {}, {}({})", rd, callee, args.iter().map(|v| format!("{}", v)).collect::<alloc::vec::Vec<_>>().join(", "))
            }
            Riscv32MachInst::Ecall { number, args } => {
                write!(f, "ecall {}({})", number, args.iter().map(|v| format!("{}", v)).collect::<alloc::vec::Vec<_>>().join(", "))
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
        }
    }
}

impl fmt::Display for VCode<Riscv32MachInst> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "vcode {{")?;
        writeln!(f, "  entry: {}", self.entry)?;
        writeln!(f)?;

        // Iterate through blocks in lowering order
        for (block_idx, lowered_block) in self.block_order.lowered_order.iter().enumerate() {
            let block_index = BlockIndex::new(block_idx as u32);

            // Format block header
            match lowered_block {
                LoweredBlock::Orig { .. } => {
                    // Get block parameters
                    let param_range = self
                        .block_params_range
                        .get(block_idx)
                        .unwrap_or_else(|| Range::new(0, 0));
                    let params: Vec<VReg> = self.block_params[param_range.start..param_range.end]
                        .to_vec();

                    if params.is_empty() {
                        write!(f, "  {}:\n", block_index)?;
                    } else {
                        write!(f, "  {}(", block_index)?;
                        for (i, param) in params.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{}", param)?;
                        }
                        write!(f, "):\n")?;
                    }
                }
                LoweredBlock::Edge { from, to, .. } => {
                    // For edge blocks, format as edge block
                    // Note: from and to are IR block entities, not VCode block indices
                    // We'll format them as-is for now
                    write!(f, "  edge {} -> {}:\n", from, to)?;
                }
            }

            // Get instruction range for this block
            let inst_range = self
                .block_ranges
                .get(block_idx)
                .unwrap_or_else(|| Range::new(0, 0));

            // Format instructions in this block
            for inst_idx in inst_range.start..inst_range.end {
                let inst = &self.insts[inst_idx];
                writeln!(f, "    {}", inst)?;
            }

            // Format successors and branch arguments
            let succ_range = self
                .block_succ_range
                .get(block_idx)
                .unwrap_or_else(|| Range::new(0, 0));
            let succs: Vec<BlockIndex> = self.block_succs[succ_range.start..succ_range.end]
                .to_vec();

            if !succs.is_empty() {
                // Get branch argument ranges for this block's successors
                let branch_arg_succ_range = self
                    .branch_block_arg_succ_range
                    .get(block_idx)
                    .unwrap_or_else(|| Range::new(0, 0));

                // For each successor, get its branch arguments
                for (succ_idx, succ) in succs.iter().enumerate() {
                    // The branch_arg_succ_range tells us which entries in branch_block_arg_range
                    // correspond to this block's successors
                    let arg_range_idx = branch_arg_succ_range.start + succ_idx;
                    let arg_range = if arg_range_idx < self.branch_block_arg_range.len() {
                        self.branch_block_arg_range.get(arg_range_idx)
                    } else {
                        None
                    };

                    // Get branch arguments for this successor
                    let args: Vec<VReg> = if let Some(arg_range) = arg_range {
                        self.branch_block_args[arg_range.start..arg_range.end].to_vec()
                    } else {
                        Vec::new()
                    };

                    if args.is_empty() {
                        writeln!(f, "    br {}", succ)?;
                    } else {
                        write!(f, "    br {}(", succ)?;
                        for (i, arg) in args.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{}", arg)?;
                        }
                        writeln!(f, ")")?;
                    }
                }
            }

            writeln!(f)?;
        }

        writeln!(f, "}}")?;
        Ok(())
    }
}

