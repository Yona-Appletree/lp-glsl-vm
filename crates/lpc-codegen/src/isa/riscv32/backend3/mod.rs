//! Backend3: Cranelift-style backend architecture
//!
//! RISC-V 32-specific implementations for backend3.
//! See docs/plans/17-backend3.md for the design.

pub mod inst;
pub mod lower;

/// Type alias for RISC-V 32-bit machine instructions
pub type MachInst = inst::Riscv32MachInst;

/// Helper functions for creating RISC-V 32 instructions during lowering
pub mod lower_helpers {
    use crate::{
        backend3::types::{VReg, Writable},
        isa::riscv32::backend3::inst::Riscv32MachInst,
    };

    /// Create an ADD instruction
    pub fn create_add(rd: Writable<VReg>, rs1: VReg, rs2: VReg) -> Riscv32MachInst {
        Riscv32MachInst::Add { rd, rs1, rs2 }
    }

    /// Create a SUB instruction
    pub fn create_sub(rd: Writable<VReg>, rs1: VReg, rs2: VReg) -> Riscv32MachInst {
        Riscv32MachInst::Sub { rd, rs1, rs2 }
    }

    /// Create an ADDI instruction
    pub fn create_addi(rd: Writable<VReg>, rs1: VReg, imm: i32) -> Riscv32MachInst {
        Riscv32MachInst::Addi { rd, rs1, imm }
    }

    /// Create a LUI instruction
    pub fn create_lui(rd: Writable<VReg>, imm: u32) -> Riscv32MachInst {
        Riscv32MachInst::Lui { rd, imm }
    }
}
