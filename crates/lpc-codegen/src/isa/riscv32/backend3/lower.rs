//! RISC-V 32-specific lowering helpers

use crate::backend3::lower::Lower;
use crate::backend3::types::{VReg, Writable};
use crate::isa::riscv32::backend3::inst::Riscv32MachInst;
use lpc_lpir::RelSourceLoc;

impl Lower<Riscv32MachInst> {
    /// Lower an ADD instruction for RISC-V 32
    pub(crate) fn lower_add(&mut self, rd: Writable<VReg>, rs1: VReg, rs2: VReg, srcloc: RelSourceLoc) {
        let mach_inst = Riscv32MachInst::Add { rd, rs1, rs2 };
        self.vcode.push(mach_inst, srcloc);
    }

    /// Lower a SUB instruction for RISC-V 32
    pub(crate) fn lower_sub(&mut self, rd: Writable<VReg>, rs1: VReg, rs2: VReg, srcloc: RelSourceLoc) {
        let mach_inst = Riscv32MachInst::Sub { rd, rs1, rs2 };
        self.vcode.push(mach_inst, srcloc);
    }
}

