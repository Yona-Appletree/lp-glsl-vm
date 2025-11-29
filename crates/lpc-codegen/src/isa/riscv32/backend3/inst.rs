//! RISC-V 32-bit machine instructions for backend3

use crate::backend3::{
    types::{VReg, Writable},
    vcode::{MachInst, OperandConstraint, OperandVisitor},
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
}

impl MachInst for Riscv32MachInst {
    type ABIMachineSpec = Riscv32ABI;

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
        }
    }
}

/// RISC-V 32-bit ABI machine spec (placeholder)
///
/// This will be implemented in a future phase to provide ABI information
/// for register allocation and calling conventions.
#[derive(Debug, Clone)]
pub struct Riscv32ABI;
