//! RISC-V 32-specific lowering backend

use lpc_lpir::{Immediate, InstEntity, IntCC, Opcode, RelSourceLoc, Value};
use regalloc2::RegClass;

use crate::{
    backend3::{
        constants::materialize_constant,
        lower::{Lower, LowerBackend},
        types::{Reg, VReg, Writable},
        vcode::MachInst,
    },
    isa::riscv32::backend3::inst::Riscv32MachInst,
};

/// RISC-V 32-bit lowering backend
#[derive(Debug, Clone, Copy)]
pub struct Riscv32LowerBackend;

/// Extract base register and offset from an address value.
///
/// If the address is the result of an iadd(base, const), returns (base_vreg, offset).
/// Otherwise, returns (address_vreg, 0).
fn extract_address_components<I: MachInst>(
    ctx: &Lower<I>,
    address_value: Value,
) -> (VReg, i32) {
    let func = ctx.func();
    let value_to_vreg = ctx.value_to_vreg();

    // Get the VReg for the address value
    let address_vreg = match value_to_vreg.get(&address_value) {
        Some(&vreg) => vreg,
        None => {
            // Address value not found - this shouldn't happen, but return address as-is
            // This case should be rare and indicates a bug in the IR
            // We'll use a dummy VReg with Int class - this will cause an error later if used
            return (VReg::new(0, RegClass::Int), 0);
        }
    };

    // Check if this value is defined by an iadd instruction
    if let Some(def_inst) = func.value_def(address_value) {
        if let Some(inst_data) = func.dfg.inst_data(def_inst) {
            if let Opcode::Iadd = inst_data.opcode {
                // This is an iadd - check if one operand is a constant
                if inst_data.args.len() >= 2 {
                    let arg0 = inst_data.args[0];
                    let arg1 = inst_data.args[1];

                    // Check if arg0 is a constant
                    if let Some(const_inst) = func.value_def(arg0) {
                        if let Some(const_inst_data) = func.dfg.inst_data(const_inst) {
                            if let Opcode::Iconst = const_inst_data.opcode {
                                if let Some(imm) = &const_inst_data.imm {
                                    let offset = match imm {
                                        Immediate::I32(val) => *val,
                                        Immediate::I64(val) => *val as i32,
                                        _ => 0,
                                    };
                                    // arg1 is the base, arg0 is the constant
                                    if let Some(&base_vreg) = value_to_vreg.get(&arg1) {
                                        return (base_vreg, offset);
                                    }
                                }
                            }
                        }
                    }

                    // Check if arg1 is a constant
                    if let Some(const_inst) = func.value_def(arg1) {
                        if let Some(const_inst_data) = func.dfg.inst_data(const_inst) {
                            if let Opcode::Iconst = const_inst_data.opcode {
                                if let Some(imm) = &const_inst_data.imm {
                                    let offset = match imm {
                                        Immediate::I32(val) => *val,
                                        Immediate::I64(val) => *val as i32,
                                        _ => 0,
                                    };
                                    // arg0 is the base, arg1 is the constant
                                    if let Some(&base_vreg) = value_to_vreg.get(&arg0) {
                                        return (base_vreg, offset);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Not an iadd with constant, or couldn't extract - return address as-is with offset 0
    (address_vreg, 0)
}

impl LowerBackend for Riscv32LowerBackend {
    type MInst = Riscv32MachInst;

    fn emit_info(&self) -> <Self::MInst as crate::backend3::vcode::MachInst>::Info {
        use crate::isa::riscv32::backend3::inst::Riscv32EmitInfo;
        Riscv32EmitInfo
    }

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
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        let mach_inst = Riscv32MachInst::Add {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Isub => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        let mach_inst = Riscv32MachInst::Sub {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
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
                        use crate::isa::riscv32::backend3::regs::zero_reg;
                        let vreg = materialize_constant(
                            &mut ctx.vcode,
                            value,
                            srcloc,
                            |rd, imm| Riscv32MachInst::Lui { rd, imm },
                            |rd, rs1, imm| Riscv32MachInst::Addi { rd, rs1, imm },
                            || zero_reg(),
                        );
                        // Record constant in constants map
                        use crate::backend3::vcode::Constant;
                        if crate::backend3::constants::fits_in_12_bits(value) {
                            ctx.vcode.record_constant(vreg, Constant::Inline(value));
                        } else {
                            ctx.vcode.record_constant(vreg, Constant::Large(value));
                        }
                        // Now update value_to_vreg (vcode borrow is dropped, so we can borrow ctx mutably)
                        let result_value = results[0];
                        ctx.value_to_vreg_mut().insert(result_value, vreg);
                        return true;
                    }
                }
            }
            Opcode::Return => {
                // Extract return value VRegs from instruction args
                let ret_vals: alloc::vec::Vec<Reg> = {
                    let value_to_vreg = ctx.value_to_vreg();
                    args.iter()
                        .filter_map(|v| value_to_vreg.get(v).copied().map(Reg::from_virtual_reg))
                        .collect()
                };
                let mach_inst = Riscv32MachInst::Return { ret_vals };
                ctx.vcode.push(mach_inst, srcloc);
                return true;
            }
            Opcode::Load => {
                // Load: result = mem[address]
                // Lower to: lw result, imm(base)
                // Try to extract base and offset from address if it's iadd(base, const)
                if !results.is_empty() && args.len() >= 1 {
                    if let (Some(address_value), Some(result_vreg)) = (args.get(0), rd_opt) {
                        let (base_vreg, offset) = extract_address_components(ctx, *address_value);
                        let rd = Writable::new(Reg::from_virtual_reg(result_vreg));
                        let rs1_reg = Reg::from_virtual_reg(base_vreg);
                        let mach_inst = Riscv32MachInst::Lw {
                            rd,
                            rs1: rs1_reg,
                            imm: offset,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Store => {
                // Store: mem[address] = value
                // Lower to: sw value, imm(base)
                // Try to extract base and offset from address if it's iadd(base, const)
                if args.len() >= 2 {
                    if let (Some(address_value), Some(value_vreg)) = (args.get(0), rs2_opt) {
                        let (base_vreg, offset) = extract_address_components(ctx, *address_value);
                        let rs1_reg = Reg::from_virtual_reg(base_vreg);
                        let rs2_reg = Reg::from_virtual_reg(value_vreg);
                        let mach_inst = Riscv32MachInst::Sw {
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                            imm: offset,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::StackAlloc { size } => {
                // StackAlloc: allocate stack space and return address
                // Lower to:
                //   1. Adjust SP: addi sp, sp, -aligned_size
                //   2. Materialize address: mv result, sp
                if !results.is_empty() {
                    if let Some(result_vreg) = rd_opt {
                        use crate::isa::riscv32::backend3::regs::stack_pointer;

                        // Round up to 4-byte alignment (RISC-V 32-bit requirement)
                        let aligned_size = ((size + 3) & !3) as i32;

                        // Get SP register
                        let sp_reg = stack_pointer();

                        // Adjust SP: addi sp, sp, -aligned_size
                        let sp_writable = Writable::new(sp_reg);
                        let adjust_inst = Riscv32MachInst::Addi {
                            rd: sp_writable,
                            rs1: sp_reg,
                            imm: -aligned_size,
                        };
                        ctx.vcode.push(adjust_inst, srcloc);

                        // Materialize address: mv result, sp
                        // Use Move instruction (which is ADD with zero)
                        let rd = Writable::new(Reg::from_virtual_reg(result_vreg));
                        let move_inst = Riscv32MachInst::Move { rd, rs: sp_reg };
                        ctx.vcode.push(move_inst, srcloc);

                        return true;
                    }
                }
            }
            Opcode::Imul => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        let mach_inst = Riscv32MachInst::Mul {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Imulh => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        let mach_inst = Riscv32MachInst::Mulh {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Idiv => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        let mach_inst = Riscv32MachInst::Div {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Irem => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        let mach_inst = Riscv32MachInst::Rem {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Icmp { .. } => {
                // Lower icmp based on condition code
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        if let Some(cond) = icmp_cond {
                            match cond {
                                IntCC::Equal => {
                                    // eq: sub + sltiu pattern
                                    // Compute diff = rs1 - rs2
                                    let temp_vreg = ctx.vcode.alloc_vreg(RegClass::Int);
                                    let diff = Writable::new(Reg::from_virtual_reg(temp_vreg));
                                    let sub_inst = Riscv32MachInst::Sub {
                                        rd: diff,
                                        rs1: rs1_reg,
                                        rs2: rs2_reg,
                                    };
                                    ctx.vcode.push(sub_inst, srcloc);
                                    // If diff < 1 (i.e., diff == 0), result = 1, else 0
                                    let temp_reg = Reg::from_virtual_reg(temp_vreg);
                                    let sltiu_inst = Riscv32MachInst::Sltiu {
                                        rd,
                                        rs1: temp_reg,
                                        imm: 1,
                                    };
                                    ctx.vcode.push(sltiu_inst, srcloc);
                                    return true;
                                }
                                IntCC::NotEqual => {
                                    // ne: same as eq, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg(RegClass::Int);
                                    let diff = Writable::new(Reg::from_virtual_reg(temp_vreg));
                                    let sub_inst = Riscv32MachInst::Sub {
                                        rd: diff,
                                        rs1: rs1_reg,
                                        rs2: rs2_reg,
                                    };
                                    ctx.vcode.push(sub_inst, srcloc);
                                    let temp_result = ctx.vcode.alloc_vreg(RegClass::Int);
                                    let temp_result_writable =
                                        Writable::new(Reg::from_virtual_reg(temp_result));
                                    let temp_reg = Reg::from_virtual_reg(temp_vreg);
                                    let sltiu_inst = Riscv32MachInst::Sltiu {
                                        rd: temp_result_writable,
                                        rs1: temp_reg,
                                        imm: 1,
                                    };
                                    ctx.vcode.push(sltiu_inst, srcloc);
                                    // Invert: xori with 1
                                    let temp_result_reg = Reg::from_virtual_reg(temp_result);
                                    let xori_inst = Riscv32MachInst::Xori {
                                        rd,
                                        rs1: temp_result_reg,
                                        imm: 1,
                                    };
                                    ctx.vcode.push(xori_inst, srcloc);
                                    return true;
                                }
                                IntCC::SignedLessThan => {
                                    // lt: slt directly
                                    let slt_inst = Riscv32MachInst::Slt {
                                        rd,
                                        rs1: rs1_reg,
                                        rs2: rs2_reg,
                                    };
                                    ctx.vcode.push(slt_inst, srcloc);
                                    return true;
                                }
                                IntCC::SignedLessThanOrEqual => {
                                    // le: slt with swapped operands, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg(RegClass::Int);
                                    let temp_writable =
                                        Writable::new(Reg::from_virtual_reg(temp_vreg));
                                    let slt_inst = Riscv32MachInst::Slt {
                                        rd: temp_writable,
                                        rs1: rs2_reg,
                                        rs2: rs1_reg,
                                    };
                                    ctx.vcode.push(slt_inst, srcloc);
                                    let temp_reg = Reg::from_virtual_reg(temp_vreg);
                                    let xori_inst = Riscv32MachInst::Xori {
                                        rd,
                                        rs1: temp_reg,
                                        imm: 1,
                                    };
                                    ctx.vcode.push(xori_inst, srcloc);
                                    return true;
                                }
                                IntCC::SignedGreaterThan => {
                                    // gt: slt with swapped operands
                                    let slt_inst = Riscv32MachInst::Slt {
                                        rd,
                                        rs1: rs2_reg,
                                        rs2: rs1_reg,
                                    };
                                    ctx.vcode.push(slt_inst, srcloc);
                                    return true;
                                }
                                IntCC::SignedGreaterThanOrEqual => {
                                    // ge: slt, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg(RegClass::Int);
                                    let temp_writable =
                                        Writable::new(Reg::from_virtual_reg(temp_vreg));
                                    let slt_inst = Riscv32MachInst::Slt {
                                        rd: temp_writable,
                                        rs1: rs1_reg,
                                        rs2: rs2_reg,
                                    };
                                    ctx.vcode.push(slt_inst, srcloc);
                                    let temp_reg = Reg::from_virtual_reg(temp_vreg);
                                    let xori_inst = Riscv32MachInst::Xori {
                                        rd,
                                        rs1: temp_reg,
                                        imm: 1,
                                    };
                                    ctx.vcode.push(xori_inst, srcloc);
                                    return true;
                                }
                                IntCC::UnsignedLessThan => {
                                    // ult: sltu directly
                                    let sltu_inst = Riscv32MachInst::Sltu {
                                        rd,
                                        rs1: rs1_reg,
                                        rs2: rs2_reg,
                                    };
                                    ctx.vcode.push(sltu_inst, srcloc);
                                    return true;
                                }
                                IntCC::UnsignedLessThanOrEqual => {
                                    // ule: sltu with swapped operands, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg(RegClass::Int);
                                    let temp_writable =
                                        Writable::new(Reg::from_virtual_reg(temp_vreg));
                                    let sltu_inst = Riscv32MachInst::Sltu {
                                        rd: temp_writable,
                                        rs1: rs2_reg,
                                        rs2: rs1_reg,
                                    };
                                    ctx.vcode.push(sltu_inst, srcloc);
                                    let temp_reg = Reg::from_virtual_reg(temp_vreg);
                                    let xori_inst = Riscv32MachInst::Xori {
                                        rd,
                                        rs1: temp_reg,
                                        imm: 1,
                                    };
                                    ctx.vcode.push(xori_inst, srcloc);
                                    return true;
                                }
                                IntCC::UnsignedGreaterThan => {
                                    // ugt: sltu with swapped operands
                                    let sltu_inst = Riscv32MachInst::Sltu {
                                        rd,
                                        rs1: rs2_reg,
                                        rs2: rs1_reg,
                                    };
                                    ctx.vcode.push(sltu_inst, srcloc);
                                    return true;
                                }
                                IntCC::UnsignedGreaterThanOrEqual => {
                                    // uge: sltu, then invert
                                    let temp_vreg = ctx.vcode.alloc_vreg(RegClass::Int);
                                    let temp_writable =
                                        Writable::new(Reg::from_virtual_reg(temp_vreg));
                                    let sltu_inst = Riscv32MachInst::Sltu {
                                        rd: temp_writable,
                                        rs1: rs1_reg,
                                        rs2: rs2_reg,
                                    };
                                    ctx.vcode.push(sltu_inst, srcloc);
                                    let temp_reg = Reg::from_virtual_reg(temp_vreg);
                                    let xori_inst = Riscv32MachInst::Xori {
                                        rd,
                                        rs1: temp_reg,
                                        imm: 1,
                                    };
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
                let arg_vregs: alloc::vec::Vec<Reg> = {
                    let value_to_vreg = ctx.value_to_vreg();
                    args.iter()
                        .filter_map(|v| value_to_vreg.get(v).copied().map(Reg::from_virtual_reg))
                        .collect()
                };

                // Extract ALL result VRegs (for multi-return support)
                let result_vregs: alloc::vec::Vec<Reg> = {
                    let value_to_vreg = ctx.value_to_vreg();
                    results
                        .iter()
                        .filter_map(|v| value_to_vreg.get(v).copied().map(Reg::from_virtual_reg))
                        .collect()
                };

                // Create JAL instruction
                // First result goes in rd (for backward compatibility)
                // If no result, use x0 (zero register) as rd
                let rd = if let Some(&first_result) = result_vregs.first() {
                    Writable::new(first_result)
                } else {
                    // Use zero_reg() for x0
                    use crate::isa::riscv32::backend3::regs::zero_reg;
                    Writable::new(zero_reg())
                };

                let mach_inst = Riscv32MachInst::Jal {
                    rd,
                    callee: callee.clone(),
                    args: arg_vregs,
                    return_count: results.len(),
                    result_vregs,
                };
                let inst_idx = ctx.vcode.push(mach_inst, srcloc);

                // Record relocation for function call
                use crate::backend3::{symbols::Symbol, types::InsnIndex, vcode::RelocKind};
                ctx.vcode.record_reloc(
                    InsnIndex::new(inst_idx),
                    RelocKind::FunctionCall,
                    Symbol::local(callee.clone()),
                );

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
                let arg_vregs: alloc::vec::Vec<Reg> = {
                    let value_to_vreg = ctx.value_to_vreg();
                    args.iter()
                        .filter_map(|v| value_to_vreg.get(v).copied().map(Reg::from_virtual_reg))
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

                let mach_inst = Riscv32MachInst::Ecall {
                    number: syscall_number,
                    args: arg_vregs,
                    result: result_vreg.map(|v| Writable::new(Reg::from_virtual_reg(v))),
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
                    let condition_reg = Reg::from_virtual_reg(condition_vreg);
                    let mach_inst = Riscv32MachInst::Trapz {
                        condition: condition_reg,
                        code,
                    };
                    ctx.vcode.push(mach_inst, srcloc);
                    return true;
                }
            }
            Opcode::Trapnz { code } => {
                // Extract condition VReg
                if let Some(condition_vreg) = rs1_opt {
                    let condition_reg = Reg::from_virtual_reg(condition_vreg);
                    let mach_inst = Riscv32MachInst::Trapnz {
                        condition: condition_reg,
                        code,
                    };
                    ctx.vcode.push(mach_inst, srcloc);
                    return true;
                }
            }
            Opcode::Iand => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        // TODO: Check if rs2 is a constant and use Andi if it fits in 12 bits
                        let mach_inst = Riscv32MachInst::And {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Ior => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        // TODO: Check if rs2 is a constant and use Ori if it fits in 12 bits
                        let mach_inst = Riscv32MachInst::Or {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Ixor => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        // TODO: Check if rs2 is a constant and use Xori if it fits in 12 bits
                        let mach_inst = Riscv32MachInst::Xor {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Inot => {
                if args.len() >= 1 && !results.is_empty() {
                    if let (Some(rs1), Some(rd_vreg)) = (rs1_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        // NOT is implemented as XOR with -1 (all bits set)
                        // Use Xori with -1 immediate (fits in 12 bits: -2048 to 2047)
                        let mach_inst = Riscv32MachInst::Xori {
                            rd,
                            rs1: rs1_reg,
                            imm: -1,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Ishl => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        // TODO: Check if rs2 is a constant (0-31) and use Slli if it fits in 5 bits
                        let mach_inst = Riscv32MachInst::Sll {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Ishr => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        // TODO: Check if rs2 is a constant (0-31) and use Srli if it fits in 5 bits
                        let mach_inst = Riscv32MachInst::Srl {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
                        return true;
                    }
                }
            }
            Opcode::Iashr => {
                if args.len() >= 2 && !results.is_empty() {
                    if let (Some(rs1), Some(rs2), Some(rd_vreg)) = (rs1_opt, rs2_opt, rd_opt) {
                        let rd = Writable::new(Reg::from_virtual_reg(rd_vreg));
                        let rs1_reg = Reg::from_virtual_reg(rs1);
                        let rs2_reg = Reg::from_virtual_reg(rs2);
                        // TODO: Check if rs2 is a constant (0-31) and use Srai if it fits in 5 bits
                        let mach_inst = Riscv32MachInst::Sra {
                            rd,
                            rs1: rs1_reg,
                            rs2: rs2_reg,
                        };
                        ctx.vcode.push(mach_inst, srcloc);
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

    fn create_move(
        &self,
        dst: crate::backend3::types::Writable<crate::backend3::types::VReg>,
        src: crate::backend3::types::VReg,
    ) -> Self::MInst {
        use crate::backend3::types::Reg;
        let dst_reg = Reg::from_virtual_reg(dst.to_reg());
        let src_reg = Reg::from_virtual_reg(src);
        Riscv32MachInst::Move {
            rd: Writable::new(dst_reg),
            rs: src_reg,
        }
    }

    fn create_branch(&self, condition: crate::backend3::types::VReg) -> Self::MInst {
        use crate::backend3::types::Reg;
        let condition_reg = Reg::from_virtual_reg(condition);
        Riscv32MachInst::Br {
            condition: condition_reg,
        }
    }

    fn create_jump(&self) -> Self::MInst {
        Riscv32MachInst::Jump
    }

    fn emit_entry_block_setup(
        &self,
        ctx: &mut crate::backend3::lower::Lower<Self::MInst>,
        entry_block: lpc_lpir::BlockEntity,
        srcloc: lpc_lpir::RelSourceLoc,
    ) {
        use crate::{
            backend3::types::Reg,
            isa::riscv32::backend3::{
                abi::Riscv32ABI,
                inst::{ArgPair, Riscv32MachInst},
                regs::frame_pointer,
            },
        };

        // Get function parameters from entry block
        // Collect parameters first to avoid borrow checker issues
        let params: alloc::vec::Vec<_> = ctx
            .func()
            .block_data(entry_block)
            .map(|bd| bd.params.clone())
            .unwrap_or_default();

        if !params.is_empty() {
            // Get ABI argument registers
            let arg_regs = Riscv32ABI::arg_regs();

            // Create ArgPairs: map each parameter Value to its VReg and ABI register
            // For parameters beyond ABI argument registers, emit LW instructions to load from stack
            let mut arg_pairs = alloc::vec::Vec::new();
            let mut stack_loads = alloc::vec::Vec::new();

            for (idx, param_value) in params.iter().enumerate() {
                if let Some(&vreg) = ctx.value_to_vreg().get(param_value) {
                    if let Some(&preg) = arg_regs.get(idx) {
                        // Parameter fits in ABI argument register
                        arg_pairs.push(ArgPair {
                            vreg: Reg::from_virtual_reg(vreg),
                            preg,
                        });
                    } else {
                        // Parameter beyond ABI argument registers - passed on stack
                        // Emit LW instruction to load from stack
                        // Stack parameters start at offset 0 from frame pointer
                        // Each parameter is 4 bytes (i32), and we skip the first 8 (in registers)
                        let stack_offset = (idx - arg_regs.len()) * 4;

                        let fp_reg = frame_pointer();
                        let vreg_writable = Reg::from_virtual_reg(vreg);

                        stack_loads.push(Riscv32MachInst::Lw {
                            rd: crate::backend3::types::Writable::new(vreg_writable),
                            rs1: fp_reg,
                            imm: stack_offset as i32,
                        });
                    }
                }
            }

            // Emit LW instructions for stack parameters first
            for lw_inst in stack_loads {
                ctx.vcode.push(lw_inst, srcloc);
            }

            // Emit Args instruction if we have register parameters
            if !arg_pairs.is_empty() {
                let args_inst = Riscv32MachInst::Args { args: arg_pairs };
                ctx.vcode.push(args_inst, srcloc);
            }
        }
    }
}
