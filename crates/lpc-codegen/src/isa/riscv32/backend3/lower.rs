//! RISC-V 32-specific lowering backend

use crate::backend3::lower::{Lower, LowerBackend};
use crate::backend3::constants::materialize_constant;
use crate::backend3::types::Writable;
use crate::isa::riscv32::backend3::inst::Riscv32MachInst;
use lpc_lpir::{Immediate, InstEntity, IntCC, Opcode, RelSourceLoc};

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
                Some(data) => (
                    data.opcode.clone(),
                    data.args.clone(),
                    data.results.clone(),
                    data.imm.clone(),
                ),
                None => return false,
            };
            inst_data
        };
        
        // Extract condition code for Icmp
        let icmp_cond = match &opcode {
            Opcode::Icmp { cond } => Some(cond.clone()),
            _ => None,
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
            Opcode::Return => {
                // Extract return value VRegs from instruction args
                let ret_vals: alloc::vec::Vec<_> = {
                    let value_to_vreg = ctx.value_to_vreg();
                    args.iter()
                        .filter_map(|v| value_to_vreg.get(v).copied())
                        .collect()
                };
                let mach_inst = Riscv32MachInst::Return { ret_vals };
                ctx.vcode.push(mach_inst, srcloc);
                return true;
            }
            Opcode::Load => {
                // Load: result = mem[address]
                // Lower to: lw result, 0(address)
                if !results.is_empty() && args.len() >= 1 {
                    if let (Some(address_vreg), Some(result_vreg)) = (rs1_opt, rd_opt) {
                        let rd = Writable::new(result_vreg);
                        let mach_inst = Riscv32MachInst::Lw { rd, rs1: address_vreg, imm: 0 };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Store => {
                // Store: mem[address] = value
                // Lower to: sw value, 0(address)
                if args.len() >= 2 {
                    if let (Some(address_vreg), Some(value_vreg)) = (rs1_opt, rs2_opt) {
                        let mach_inst = Riscv32MachInst::Sw { rs1: address_vreg, rs2: value_vreg, imm: 0 };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Imul => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(rd_vreg);
                        let mach_inst = Riscv32MachInst::Mul { rd, rs1, rs2 };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Idiv => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(rd_vreg);
                        let mach_inst = Riscv32MachInst::Div { rd, rs1, rs2 };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Irem => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(rd_vreg);
                        let mach_inst = Riscv32MachInst::Rem { rd, rs1, rs2 };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Icmp { .. } => {
                // Lower icmp based on condition code
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(rd_vreg);
                        if let Some(cond) = icmp_cond {
                            match cond {
                                IntCC::Equal => {
                                    // eq: sub + sltiu pattern
                                    // Compute diff = rs1 - rs2
                                    let temp_vreg = ctx.vcode.alloc_vreg();
                                    let diff = Writable::new(temp_vreg);
                                    let sub_inst = Riscv32MachInst::Sub { rd: diff, rs1, rs2 };
                                    ctx.vcode.push(sub_inst, srcloc);
                                    // If diff < 1 (i.e., diff == 0), result = 1, else 0
                                    let sltiu_inst = Riscv32MachInst::Sltiu { rd, rs1: temp_vreg, imm: 1 };
                                    ctx.vcode.push(sltiu_inst, srcloc);
                                    return true;
                                }
                                IntCC::NotEqual => {
                                    // ne: same as eq, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg();
                                    let diff = Writable::new(temp_vreg);
                                    let sub_inst = Riscv32MachInst::Sub { rd: diff, rs1, rs2 };
                                    ctx.vcode.push(sub_inst, srcloc);
                                    let temp_result = ctx.vcode.alloc_vreg();
                                    let temp_result_writable = Writable::new(temp_result);
                                    let sltiu_inst = Riscv32MachInst::Sltiu { rd: temp_result_writable, rs1: temp_vreg, imm: 1 };
                                    ctx.vcode.push(sltiu_inst, srcloc);
                                    // Invert: xori with 1
                                    let xori_inst = Riscv32MachInst::Xori { rd, rs1: temp_result, imm: 1 };
                                    ctx.vcode.push(xori_inst, srcloc);
                                    return true;
                                }
                                IntCC::SignedLessThan => {
                                    // lt: slt directly
                                    let slt_inst = Riscv32MachInst::Slt { rd, rs1, rs2 };
                                    ctx.vcode.push(slt_inst, srcloc);
                                    return true;
                                }
                                IntCC::SignedLessThanOrEqual => {
                                    // le: slt with swapped operands, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg();
                                    let temp_writable = Writable::new(temp_vreg);
                                    let slt_inst = Riscv32MachInst::Slt { rd: temp_writable, rs1: rs2, rs2: rs1 };
                                    ctx.vcode.push(slt_inst, srcloc);
                                    let xori_inst = Riscv32MachInst::Xori { rd, rs1: temp_vreg, imm: 1 };
                                    ctx.vcode.push(xori_inst, srcloc);
                                    return true;
                                }
                                IntCC::SignedGreaterThan => {
                                    // gt: slt with swapped operands
                                    let slt_inst = Riscv32MachInst::Slt { rd, rs1: rs2, rs2: rs1 };
                                    ctx.vcode.push(slt_inst, srcloc);
                                    return true;
                                }
                                IntCC::SignedGreaterThanOrEqual => {
                                    // ge: slt, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg();
                                    let temp_writable = Writable::new(temp_vreg);
                                    let slt_inst = Riscv32MachInst::Slt { rd: temp_writable, rs1, rs2 };
                                    ctx.vcode.push(slt_inst, srcloc);
                                    let xori_inst = Riscv32MachInst::Xori { rd, rs1: temp_vreg, imm: 1 };
                                    ctx.vcode.push(xori_inst, srcloc);
                                    return true;
                                }
                                IntCC::UnsignedLessThan => {
                                    // ult: sltu directly
                                    let sltu_inst = Riscv32MachInst::Sltu { rd, rs1, rs2 };
                                    ctx.vcode.push(sltu_inst, srcloc);
                                    return true;
                                }
                                IntCC::UnsignedLessThanOrEqual => {
                                    // ule: sltu with swapped operands, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg();
                                    let temp_writable = Writable::new(temp_vreg);
                                    let sltu_inst = Riscv32MachInst::Sltu { rd: temp_writable, rs1: rs2, rs2: rs1 };
                                    ctx.vcode.push(sltu_inst, srcloc);
                                    let xori_inst = Riscv32MachInst::Xori { rd, rs1: temp_vreg, imm: 1 };
                                    ctx.vcode.push(xori_inst, srcloc);
                                    return true;
                                }
                                IntCC::UnsignedGreaterThan => {
                                    // ugt: sltu with swapped operands
                                    let sltu_inst = Riscv32MachInst::Sltu { rd, rs1: rs2, rs2: rs1 };
                                    ctx.vcode.push(sltu_inst, srcloc);
                                    return true;
                                }
                                IntCC::UnsignedGreaterThanOrEqual => {
                                    // uge: sltu, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg();
                                    let temp_writable = Writable::new(temp_vreg);
                                    let sltu_inst = Riscv32MachInst::Sltu { rd: temp_writable, rs1, rs2 };
                                    ctx.vcode.push(sltu_inst, srcloc);
                                    let xori_inst = Riscv32MachInst::Xori { rd, rs1: temp_vreg, imm: 1 };
                                    ctx.vcode.push(xori_inst, srcloc);
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            Opcode::Call { callee } => {
                // Extract argument VRegs
                let arg_vregs: alloc::vec::Vec<_> = {
                    let value_to_vreg = ctx.value_to_vreg();
                    args.iter()
                        .filter_map(|v| value_to_vreg.get(v).copied())
                        .collect()
                };
                
                // Get result VReg (if any)
                let result_vreg = if !results.is_empty() {
                    results.get(0).and_then(|v| {
                        let value_to_vreg = ctx.value_to_vreg();
                        value_to_vreg.get(v).copied()
                    })
                } else {
                    None
                };
                
                // Create JAL instruction
                // If no result, use x0 (zero register) as rd
                let rd = if let Some(result_vreg) = result_vreg {
                    Writable::new(result_vreg)
                } else {
                    // Use v0 as placeholder for x0 - will be handled during emission
                    let zero_vreg = ctx.vcode.alloc_vreg();
                    Writable::new(zero_vreg)
                };
                
                let mach_inst = Riscv32MachInst::Jal {
                    rd,
                    callee: callee.clone(),
                    args: arg_vregs,
                };
                ctx.vcode.push(mach_inst, srcloc);
                return true;
            }
            Opcode::Syscall => {
                // Extract syscall number from immediate
                let syscall_number = imm
                    .and_then(|i| match i {
                        Immediate::I32(n) => Some(n),
                        Immediate::I64(n) => Some(n as i32),
                        _ => None,
                    })
                    .unwrap_or(0);
                
                // Extract argument VRegs
                let arg_vregs: alloc::vec::Vec<_> = {
                    let value_to_vreg = ctx.value_to_vreg();
                    args.iter()
                        .filter_map(|v| value_to_vreg.get(v).copied())
                        .collect()
                };
                
                let mach_inst = Riscv32MachInst::Ecall {
                    number: syscall_number,
                    args: arg_vregs,
                };
                ctx.vcode.push(mach_inst, srcloc);
                return true;
            }
            Opcode::Halt => {
                let mach_inst = Riscv32MachInst::Ebreak;
                ctx.vcode.push(mach_inst, srcloc);
                return true;
            }
            Opcode::Trap { code } => {
                let mach_inst = Riscv32MachInst::Trap { code };
                ctx.vcode.push(mach_inst, srcloc);
                return true;
            }
            Opcode::Trapz { code } => {
                // Extract condition VReg
                if let Some(condition_vreg) = rs1_opt {
                    let mach_inst = Riscv32MachInst::Trapz {
                        condition: condition_vreg,
                        code,
                    };
                    ctx.vcode.push(mach_inst, srcloc);
                    return true;
                }
            }
            Opcode::Trapnz { code } => {
                // Extract condition VReg
                if let Some(condition_vreg) = rs1_opt {
                    let mach_inst = Riscv32MachInst::Trapnz {
                        condition: condition_vreg,
                        code,
                    };
                    ctx.vcode.push(mach_inst, srcloc);
                    return true;
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
