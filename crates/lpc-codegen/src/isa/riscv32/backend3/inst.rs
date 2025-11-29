//! RISC-V 32-bit machine instructions for backend3

use crate::backend3::{
    types::{VReg, Writable},
    vcode::{MachInst, OperandConstraint, OperandVisitor, PRegSet},
};

/// RISC-V 32-bit machine instruction with virtual registers
#[derive(Debug, Clone)]
pub enum Riscv32MachInst {
    /// ADD: rd = rs1 + rs2
    Add {
        rd: Writable<VReg>,
        rs1: VReg,
        rs2: VReg,
    },

    /// ADDI: rd = rs1 + imm
    Addi {
        rd: Writable<VReg>,
        rs1: VReg,
        imm: i32,
    },

    /// SUB: rd = rs1 - rs2
    Sub {
        rd: Writable<VReg>,
        rs1: VReg,
        rs2: VReg,
    },

    /// LUI: Load upper immediate (rd = imm << 12)
    Lui { rd: Writable<VReg>, imm: u32 },

    /// LW: Load word (rd = mem[rs1 + imm])
    Lw {
        rd: Writable<VReg>,
        rs1: VReg,
        imm: i32,
    },

    /// SW: Store word (mem[rs1 + imm] = rs2)
    Sw { rs1: VReg, rs2: VReg, imm: i32 },

    /// Move: rd = rs (register copy)
    /// This is used for phi moves in edge blocks.
    /// On RISC-V, this is typically implemented as ADD rd, rs, x0
    Move { rd: Writable<VReg>, rs: VReg },

    /// Return: return from function with values
    /// Return values are passed in ret_vals (up to 2 for RISC-V 32 ABI)
    /// Actual ABI handling (moving to a0/a1) happens during emission
    Return { ret_vals: alloc::vec::Vec<VReg> },

    /// MUL: rd = rs1 * rs2 (RISC-V M extension)
    Mul {
        rd: Writable<VReg>,
        rs1: VReg,
        rs2: VReg,
    },

    /// DIV: rd = rs1 / rs2 (signed, RISC-V M extension)
    Div {
        rd: Writable<VReg>,
        rs1: VReg,
        rs2: VReg,
    },

    /// REM: rd = rs1 % rs2 (signed, RISC-V M extension)
    Rem {
        rd: Writable<VReg>,
        rs1: VReg,
        rs2: VReg,
    },

    /// SLT: rd = (rs1 < rs2) ? 1 : 0 (signed)
    Slt {
        rd: Writable<VReg>,
        rs1: VReg,
        rs2: VReg,
    },

    /// SLTIU: rd = (rs1 < imm) ? 1 : 0 (unsigned)
    Sltiu {
        rd: Writable<VReg>,
        rs1: VReg,
        imm: i32,
    },

    /// SLTU: rd = (rs1 < rs2) ? 1 : 0 (unsigned)
    Sltu {
        rd: Writable<VReg>,
        rs1: VReg,
        rs2: VReg,
    },

    /// XORI: rd = rs1 ^ imm
    Xori {
        rd: Writable<VReg>,
        rs1: VReg,
        imm: i32,
    },

    /// JAL: rd = pc + 4; pc = pc + imm (function call)
    Jal {
        rd: Writable<VReg>,
        callee: alloc::string::String,
        args: alloc::vec::Vec<VReg>,
    },

    /// ECALL: system call
    /// After execution, return value is in a0 (x10) register
    Ecall {
        number: i32,
        args: alloc::vec::Vec<VReg>,
        result: Option<Writable<VReg>>, // Result register (receives a0 after ecall)
    },

    /// EBREAK: halt/breakpoint
    Ebreak,

    /// TRAP: unconditional trap with trap code
    Trap { code: lpc_lpir::TrapCode },

    /// TRAPZ: trap if condition is zero
    Trapz {
        condition: VReg,
        code: lpc_lpir::TrapCode,
    },

    /// TRAPNZ: trap if condition is non-zero
    Trapnz {
        condition: VReg,
        code: lpc_lpir::TrapCode,
    },

    /// BR: conditional branch
    /// The condition VReg is checked, and branch targets/successors are stored in VCode branch metadata
    Br { condition: VReg },

    /// JUMP: unconditional jump
    /// Branch targets/successors are stored in VCode branch metadata
    Jump,
}

/// RISC-V 32-bit emission information
///
/// This contains ISA-specific information needed during code emission (Phase 3).
/// For now, this is a placeholder that can be extended with flags and settings
/// as needed.
#[derive(Debug, Clone)]
pub struct Riscv32EmitInfo;

impl MachInst for Riscv32MachInst {
    type ABIMachineSpec = Riscv32ABI;
    type Info = Riscv32EmitInfo;

    fn get_operands(&mut self, collector: &mut impl OperandVisitor) {
        match self {
            Riscv32MachInst::Add { rd, rs1, rs2 } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
                collector.visit_use(*rs2, OperandConstraint::Any);
            }
            Riscv32MachInst::Addi { rd, rs1, imm: _ } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
                // Immediate is handled separately, not as an operand
            }
            Riscv32MachInst::Sub { rd, rs1, rs2 } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
                collector.visit_use(*rs2, OperandConstraint::Any);
            }
            Riscv32MachInst::Lui { rd, imm: _ } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                // Immediate is handled separately
            }
            Riscv32MachInst::Lw { rd, rs1, imm: _ } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
                // Immediate is handled separately
            }
            Riscv32MachInst::Sw { rs1, rs2, imm: _ } => {
                collector.visit_use(*rs1, OperandConstraint::Any);
                collector.visit_use(*rs2, OperandConstraint::Any);
                // Immediate is handled separately
            }
            Riscv32MachInst::Move { rd, rs } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs, OperandConstraint::Any);
            }
            Riscv32MachInst::Return { ret_vals } => {
                // Return values are uses (read before returning)
                for ret_val in ret_vals.iter() {
                    collector.visit_use(*ret_val, OperandConstraint::Any);
                }
            }
            Riscv32MachInst::Mul { rd, rs1, rs2 } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
                collector.visit_use(*rs2, OperandConstraint::Any);
            }
            Riscv32MachInst::Div { rd, rs1, rs2 } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
                collector.visit_use(*rs2, OperandConstraint::Any);
            }
            Riscv32MachInst::Rem { rd, rs1, rs2 } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
                collector.visit_use(*rs2, OperandConstraint::Any);
            }
            Riscv32MachInst::Slt { rd, rs1, rs2 } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
                collector.visit_use(*rs2, OperandConstraint::Any);
            }
            Riscv32MachInst::Sltiu { rd, rs1, imm: _ } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
            }
            Riscv32MachInst::Sltu { rd, rs1, rs2 } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
                collector.visit_use(*rs2, OperandConstraint::Any);
            }
            Riscv32MachInst::Xori { rd, rs1, imm: _ } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                collector.visit_use(*rs1, OperandConstraint::Any);
            }
            Riscv32MachInst::Jal { rd, args, .. } => {
                collector.visit_def(rd.to_reg(), OperandConstraint::Any);
                for arg in args.iter() {
                    collector.visit_use(*arg, OperandConstraint::Any);
                }
            }
            Riscv32MachInst::Ecall { args, result, .. } => {
                for arg in args.iter() {
                    collector.visit_use(*arg, OperandConstraint::Any);
                }
                if let Some(result) = result {
                    collector.visit_def(result.to_reg(), OperandConstraint::Any);
                }
            }
            Riscv32MachInst::Ebreak => {
                // No operands
            }
            Riscv32MachInst::Trap { .. } => {
                // No operands
            }
            Riscv32MachInst::Trapz { condition, .. } => {
                collector.visit_use(*condition, OperandConstraint::Any);
            }
            Riscv32MachInst::Trapnz { condition, .. } => {
                collector.visit_use(*condition, OperandConstraint::Any);
            }
            Riscv32MachInst::Br { condition } => {
                collector.visit_use(*condition, OperandConstraint::Any);
            }
            Riscv32MachInst::Jump => {
                // No operands (unconditional)
            }
        }
    }

    /// Get clobbered registers (for function calls, etc.)
    ///
    /// Returns None if no explicit clobbers, or Some(set) if there are clobbers.
    /// For RISC-V 32, function calls (JAL) clobber caller-saved registers.
    fn get_clobbers(&self) -> Option<PRegSet> {
        match self {
            Riscv32MachInst::Jal { .. } => {
                // Function calls clobber caller-saved registers
                // RISC-V 32 calling convention (System V ABI):
                // Caller-saved: t0-t6 (x5-x7, x28-x31), a0-a7 (x10-x17)
                // Note: PRegSet is currently BTreeSet<u32> as a placeholder.
                // When proper PRegSet is implemented (from regalloc2), this should
                // return the actual caller-saved register set from the ABI.
                use alloc::collections::BTreeSet;
                let mut clobbers = BTreeSet::new();
                // Placeholder: mark caller-saved registers
                // t0-t6: x5-x7, x28-x31
                clobbers.insert(5); // t0
                clobbers.insert(6); // t1
                clobbers.insert(7); // t2
                clobbers.insert(28); // t3
                clobbers.insert(29); // t4
                clobbers.insert(30); // t5
                clobbers.insert(31); // t6
                                     // a0-a7: x10-x17
                for i in 10..=17 {
                    clobbers.insert(i);
                }
                Some(clobbers)
            }
            _ => None,
        }
    }
}

/// RISC-V 32-bit ABI machine spec (placeholder)
///
/// This will be implemented in a future phase to provide ABI information
/// for register allocation and calling conventions.
#[derive(Debug, Clone)]
pub struct Riscv32ABI;
