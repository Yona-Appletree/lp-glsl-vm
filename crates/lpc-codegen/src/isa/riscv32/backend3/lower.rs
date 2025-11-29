//! RISC-V 32-specific lowering backend

use crate::backend3::lower::{Lower, LowerBackend};
use crate::backend3::constants::materialize_constant;
use crate::backend3::types::Writable;
use crate::isa::riscv32::backend3::inst::Riscv32MachInst;
use lpc_lpir::{Immediate, InstEntity, Opcode, RelSourceLoc};

/// RISC-V 32-bit lowering backend
#[derive(Debug, Clone, Copy)]
pub struct Riscv32LowerBackend;

impl LowerBackend for Riscv32LowerBackend {
    type MInst = Riscv32MachInst;

    fn lower_inst(
        &self,
        ctx: &mut Lower<Self::MInst>,
        inst: InstEntity,
        srcloc: RelSourceLoc,
    ) -> bool {
        // Get inst_data first and drop the func() borrow before using vcode
        let (opcode, args, results, imm) = {
            let func = ctx.func();
            let inst_data = match func.dfg.inst_data(inst) {
                Some(data) => (data.opcode.clone(), data.args.clone(), data.results.clone(), data.imm.clone()),
                None => return false,
            };
            inst_data
        };

        // Extract VRegs - func() borrow is dropped, so we can borrow value_to_vreg
        let (rs1_opt, rs2_opt, rd_opt) = {
            let value_to_vreg = ctx.value_to_vreg();
            (
                args.get(0).and_then(|v| value_to_vreg.get(v).copied()),
                args.get(1).and_then(|v| value_to_vreg.get(v).copied()),
                results.get(0).and_then(|v| value_to_vreg.get(v).copied()),
            )
        };
        
        match opcode {
            Opcode::Iadd => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(rd_vreg);
                        let mach_inst = Riscv32MachInst::Add { rd, rs1, rs2 };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Isub => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(rd_vreg);
                        let mach_inst = Riscv32MachInst::Sub { rd, rs1, rs2 };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Iconst => {
                if !results.is_empty() {
                    // Materialize constant
                    if let Some(imm_val) = &imm {
                        let value = match imm_val {
                            Immediate::I32(val) => *val,
                            Immediate::I64(val) => *val as i32, // Truncate to i32
                            _ => 0,
                        };
                        // Materialize constant with ISA-specific helpers
                        let vreg = materialize_constant(
                            &mut ctx.vcode,
                            value,
                            srcloc,
                            |rd, imm| Riscv32MachInst::Lui { rd, imm },
                            |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
                        );
                        // Now update value_to_vreg (vcode borrow is dropped, so we can borrow ctx mutably)
                        let result_value = results[0];
                        ctx.value_to_vreg_mut().insert(result_value, vreg);
                        return true;
                    }
                }
            }
            _ => {
                // Other opcodes not yet implemented
            }
        }

        false
    }

    fn create_move(&self, dst: crate::backend3::types::Writable<crate::backend3::types::VReg>, src: crate::backend3::types::VReg) -> Self::MInst {
        use crate::isa::riscv32::backend3::lower_helpers;
        lower_helpers::create_move(dst, src)
    }
}
