//! RISC-V 32-bit machine instructions for backend3

use regalloc2::{OperandConstraint, PReg};

use crate::backend3::{
    types::{Reg, VReg, Writable},
    vcode::{MachInst, MachTerminator, OperandVisitor, PRegSet},
};

/// Argument pair for Args instruction
///
/// Maps a virtual register (function parameter) to a physical register (ABI argument register).
#[derive(Debug, Clone)]
pub struct ArgPair {
    /// Virtual register representing the function parameter
    pub vreg: Reg,
    /// Physical register where the argument is passed (e.g., a0, a1, etc.)
    pub preg: PReg,
}

/// RISC-V 32-bit machine instruction with unified Reg type
#[derive(Debug, Clone)]
pub enum Riscv32MachInst {
    /// ADD: rd = rs1 + rs2
    Add {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// ADDI: rd = rs1 + imm
    Addi {
        rd: Writable<Reg>,
        rs1: Reg,
        imm: i32,
    },

    /// SUB: rd = rs1 - rs2
    Sub {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// LUI: Load upper immediate (rd = imm << 12)
    Lui { rd: Writable<Reg>, imm: u32 },

    /// LW: Load word (rd = mem[rs1 + imm])
    Lw {
        rd: Writable<Reg>,
        rs1: Reg,
        imm: i32,
    },

    /// SW: Store word (mem[rs1 + imm] = rs2)
    Sw { rs1: Reg, rs2: Reg, imm: i32 },

    /// Move: rd = rs (register copy)
    /// This is used for phi moves in edge blocks.
    /// On RISC-V, this is typically implemented as ADD rd, rs, x0
    Move { rd: Writable<Reg>, rs: Reg },

    /// Return: return from function with values
    /// Return values are passed in ret_vals (up to 2 for RISC-V 32 ABI)
    /// Actual ABI handling (moving to a0/a1) happens during emission
    Return { ret_vals: alloc::vec::Vec<Reg> },

    /// MUL: rd = rs1 * rs2 (RISC-V M extension)
    Mul {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// DIV: rd = rs1 / rs2 (signed, RISC-V M extension)
    Div {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// REM: rd = rs1 % rs2 (signed, RISC-V M extension)
    Rem {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// SLT: rd = (rs1 < rs2) ? 1 : 0 (signed)
    Slt {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// SLTIU: rd = (rs1 < imm) ? 1 : 0 (unsigned)
    Sltiu {
        rd: Writable<Reg>,
        rs1: Reg,
        imm: i32,
    },

    /// SLTU: rd = (rs1 < rs2) ? 1 : 0 (unsigned)
    Sltu {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// XORI: rd = rs1 ^ imm
    Xori {
        rd: Writable<Reg>,
        rs1: Reg,
        imm: i32,
    },

    /// AND: rd = rs1 & rs2
    And {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// ANDI: rd = rs1 & imm
    Andi {
        rd: Writable<Reg>,
        rs1: Reg,
        imm: i32,
    },

    /// OR: rd = rs1 | rs2
    Or {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// ORI: rd = rs1 | imm
    Ori {
        rd: Writable<Reg>,
        rs1: Reg,
        imm: i32,
    },

    /// XOR: rd = rs1 ^ rs2
    Xor {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// SLL: Shift left logical (rd = rs1 << rs2)
    Sll {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// SLLI: Shift left logical immediate (rd = rs1 << imm)
    Slli {
        rd: Writable<Reg>,
        rs1: Reg,
        imm: i32,
    },

    /// SRL: Shift right logical (rd = rs1 >>> rs2, unsigned)
    Srl {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// SRLI: Shift right logical immediate (rd = rs1 >>> imm, unsigned)
    Srli {
        rd: Writable<Reg>,
        rs1: Reg,
        imm: i32,
    },

    /// SRA: Shift right arithmetic (rd = rs1 >> rs2, signed)
    Sra {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },

    /// SRAI: Shift right arithmetic immediate (rd = rs1 >> imm, signed)
    Srai {
        rd: Writable<Reg>,
        rs1: Reg,
        imm: i32,
    },

    /// JAL: rd = pc + 4; pc = pc + imm (function call)
    Jal {
        rd: Writable<Reg>,
        callee: alloc::string::String,
        args: alloc::vec::Vec<Reg>,
    },

    /// ECALL: system call
    /// After execution, return value is in a0 (x10) register
    Ecall {
        number: i32,
        args: alloc::vec::Vec<Reg>,
        result: Option<Writable<Reg>>, // Result register (receives a0 after ecall)
    },

    /// EBREAK: halt/breakpoint
    Ebreak,

    /// TRAP: unconditional trap with trap code
    Trap { code: lpc_lpir::TrapCode },

    /// TRAPZ: trap if condition is zero
    Trapz {
        condition: Reg,
        code: lpc_lpir::TrapCode,
    },

    /// TRAPNZ: trap if condition is non-zero
    Trapnz {
        condition: Reg,
        code: lpc_lpir::TrapCode,
    },

    /// BR: conditional branch
    /// The condition Reg is checked, and branch targets/successors are stored in VCode branch metadata
    Br { condition: Reg },

    /// JUMP: unconditional jump
    /// Branch targets/successors are stored in VCode branch metadata
    Jump,

    /// Args: define function parameters with fixed physical registers
    ///
    /// This pseudo-instruction defines entry block parameters (function arguments)
    /// by mapping them to ABI argument registers. It emits no code during emission,
    /// but tells regalloc2 that each VReg must be allocated to the specified physical register.
    Args { args: alloc::vec::Vec<ArgPair> },
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
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Addi { rd, rs1, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                // Immediate is handled separately, not as an operand
            }
            Riscv32MachInst::Sub { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Lui { rd, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                // Immediate is handled separately
            }
            Riscv32MachInst::Lw { rd, rs1, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                // Immediate is handled separately
            }
            Riscv32MachInst::Sw { rs1, rs2, imm: _ } => {
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
                // Immediate is handled separately
            }
            Riscv32MachInst::Move { rd, rs } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs), OperandConstraint::Any);
            }
            Riscv32MachInst::Return { ret_vals } => {
                // Return values are uses (read before returning)
                for ret_val in ret_vals.iter() {
                    collector.visit_use(VReg::from(*ret_val), OperandConstraint::Any);
                }
            }
            Riscv32MachInst::Mul { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Div { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Rem { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Slt { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Sltiu { rd, rs1, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
            }
            Riscv32MachInst::Sltu { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Xori { rd, rs1, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
            }
            Riscv32MachInst::And { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Andi { rd, rs1, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
            }
            Riscv32MachInst::Or { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Ori { rd, rs1, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
            }
            Riscv32MachInst::Xor { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Sll { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Slli { rd, rs1, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
            }
            Riscv32MachInst::Srl { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Srli { rd, rs1, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
            }
            Riscv32MachInst::Sra { rd, rs1, rs2 } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs2), OperandConstraint::Any);
            }
            Riscv32MachInst::Srai { rd, rs1, imm: _ } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                collector.visit_use(VReg::from(*rs1), OperandConstraint::Any);
            }
            Riscv32MachInst::Jal { rd, args, .. } => {
                collector.visit_def(VReg::from(rd.to_reg()), OperandConstraint::Any);
                for arg in args.iter() {
                    collector.visit_use(VReg::from(*arg), OperandConstraint::Any);
                }
            }
            Riscv32MachInst::Ecall { args, result, .. } => {
                for arg in args.iter() {
                    collector.visit_use(VReg::from(*arg), OperandConstraint::Any);
                }
                if let Some(result) = result {
                    collector.visit_def(VReg::from(result.to_reg()), OperandConstraint::Any);
                }
            }
            Riscv32MachInst::Ebreak => {
                // No operands
            }
            Riscv32MachInst::Trap { .. } => {
                // No operands
            }
            Riscv32MachInst::Trapz { condition, .. } => {
                collector.visit_use(VReg::from(*condition), OperandConstraint::Any);
            }
            Riscv32MachInst::Trapnz { condition, .. } => {
                collector.visit_use(VReg::from(*condition), OperandConstraint::Any);
            }
            Riscv32MachInst::Br { condition } => {
                collector.visit_use(VReg::from(*condition), OperandConstraint::Any);
            }
            Riscv32MachInst::Jump => {
                // No operands (unconditional)
            }
            Riscv32MachInst::Args { args } => {
                // For each ArgPair, define the VReg with a fixed physical register constraint
                for arg_pair in args.iter() {
                    let vreg = VReg::from(arg_pair.vreg);
                    collector.visit_def(vreg, OperandConstraint::FixedReg(arg_pair.preg));
                }
            }
        }
    }

    fn is_term(&self) -> MachTerminator {
        match self {
            Riscv32MachInst::Return { .. } => MachTerminator::Ret,
            Riscv32MachInst::Br { .. } | Riscv32MachInst::Jump => MachTerminator::Branch,
            Riscv32MachInst::Trap { .. }
            | Riscv32MachInst::Trapz { .. }
            | Riscv32MachInst::Trapnz { .. }
            | Riscv32MachInst::Ebreak => MachTerminator::Ret, // Traps terminate execution
            Riscv32MachInst::Args { .. } => MachTerminator::None, // Args is not a terminator
            _ => MachTerminator::None,
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
                // TODO: Properly convert to regalloc2::PRegSet
                // For now, return None - clobbers will be handled by ABI
                None
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
