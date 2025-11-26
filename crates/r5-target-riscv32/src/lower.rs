//! Instruction lowering (IR → RISC-V).
//!
//! This module lowers IR instructions to RISC-V instructions using pre-computed
//! register allocation, spill/reload plan, and frame layout.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use r5_ir::{Function, Inst, Value};
use riscv32_encoder::{Gpr, Inst as RiscvInst};

use crate::{
    abi::AbiInfo,
    emit::CodeBuffer,
    frame::FrameLayout,
    liveness::InstPoint,
    regalloc::RegisterAllocation,
    spill_reload::{SpillReloadOp, SpillReloadPlan},
};

/// A relocation that needs to be fixed up.
#[derive(Debug, Clone)]
pub struct Relocation {
    /// Offset in the code buffer where the instruction is
    pub offset: usize,
    /// Name of the function being called
    pub callee: String,
}

/// Lower IR to RISC-V 32-bit code.
///
/// Uses pre-computed register allocation, spill/reload plan, and frame layout.
pub struct Lowerer {
    /// Module context for function calls (optional).
    module: Option<r5_ir::Module>,
    /// Function addresses (for call relocations).
    function_addresses: BTreeMap<String, u32>,
    /// Relocations that need to be fixed up (call sites).
    relocations: Vec<Relocation>,
}

impl Lowerer {
    /// Create a new lowerer.
    pub fn new() -> Self {
        Self {
            module: None,
            function_addresses: BTreeMap::new(),
            relocations: Vec::new(),
        }
    }

    /// Set the module context for function calls.
    pub fn set_module(&mut self, module: r5_ir::Module) {
        self.module = Some(module);
    }

    /// Set a function address for call relocations.
    pub fn set_function_address(&mut self, name: String, address: u32) {
        self.function_addresses.insert(name, address);
    }

    /// Get relocations that need to be fixed up.
    pub fn relocations(&self) -> &[Relocation] {
        &self.relocations
    }

    /// Clear relocations (for next function).
    pub fn clear_relocations(&mut self) {
        self.relocations.clear();
    }

    /// Lower a function to RISC-V 32-bit code.
    ///
    /// Uses pre-computed allocation, spill/reload plan, and frame layout.
    pub fn lower_function(
        &mut self,
        func: &Function,
        allocation: &RegisterAllocation,
        spill_reload: &SpillReloadPlan,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) -> CodeBuffer {
        let mut code = CodeBuffer::new();

        // 1. Generate prologue
        self.gen_prologue(&mut code, frame_layout, abi_info);

        // 2. Track block addresses for jumps/branches
        let mut block_addresses = BTreeMap::new();
        let _prologue_size = code.instruction_count();

        // 3. Lower each block
        for (block_idx, block) in func.blocks.iter().enumerate() {
            // Record block address
            block_addresses.insert(block_idx, code.instruction_count() as u32);

            // Lower block parameters (if any) - these are already in registers from entry
            // Block parameters are handled by the register allocator

            // Lower each instruction
            for (inst_idx, inst) in block.insts.iter().enumerate() {
                let point = InstPoint::new(block_idx, inst_idx + 1);

                // Emit spill/reload operations before instruction
                if let Some(ops) = spill_reload.before.get(&point) {
                    for op in ops {
                        self.emit_spill_reload(&mut code, op, frame_layout);
                    }
                }

                // Lower the instruction
                self.lower_inst(
                    &mut code,
                    inst,
                    allocation,
                    frame_layout,
                    abi_info,
                    &block_addresses,
                );

                // Emit spill/reload operations after instruction
                let after_point = InstPoint::new(block_idx, inst_idx + 2);
                if let Some(ops) = spill_reload.after.get(&after_point) {
                    for op in ops {
                        self.emit_spill_reload(&mut code, op, frame_layout);
                    }
                }
            }
        }

        // 4. Generate epilogue
        self.gen_epilogue(&mut code, frame_layout, abi_info);

        code
    }

    /// Generate function prologue.
    fn gen_prologue(
        &mut self,
        code: &mut CodeBuffer,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) {
        let frame_size = frame_layout.total_size();

        if frame_size > 0 {
            // First, adjust stack pointer for entire frame
            code.emit(RiscvInst::Addi {
                rd: Gpr::SP,
                rs1: Gpr::SP,
                imm: -(frame_size as i32),
            });

            // Save return address if we have calls (at offset 0 in setup area)
            if frame_layout.has_function_calls {
                // Save RA: sw ra, 0(sp) (or at setup_area_size - 4 if setup area > 0)
                let ra_offset = if frame_layout.setup_area_size > 0 {
                    frame_layout.setup_area_size as i32 - 4
                } else {
                    0
                };
                code.emit(RiscvInst::Sw {
                    rs1: Gpr::SP,
                    rs2: Gpr::RA,
                    imm: ra_offset,
                });
            }

            // Save callee-saved registers (at their computed offsets)
            for (_idx, reg) in abi_info.used_callee_saved.iter().enumerate() {
                if let Some(offset) = frame_layout.callee_saved_offset(*reg) {
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::SP,
                        rs2: *reg,
                        imm: offset,
                    });
                }
            }
        }
    }

    /// Generate function epilogue.
    fn gen_epilogue(
        &mut self,
        code: &mut CodeBuffer,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) {
        let frame_size = frame_layout.total_size();

        if frame_size > 0 {
            // Restore callee-saved registers (reverse order)
            for reg in abi_info.used_callee_saved.iter().rev() {
                if let Some(offset) = frame_layout.callee_saved_offset(*reg) {
                    code.emit(RiscvInst::Lw {
                        rd: *reg,
                        rs1: Gpr::SP,
                        imm: offset,
                    });
                }
            }

            // Restore return address if we saved it (before restoring SP)
            if frame_layout.has_function_calls {
                let ra_offset = if frame_layout.setup_area_size > 0 {
                    frame_layout.setup_area_size as i32 - 4
                } else {
                    0
                };
                code.emit(RiscvInst::Lw {
                    rd: Gpr::RA,
                    rs1: Gpr::SP,
                    imm: ra_offset,
                });
            }

            // Restore stack pointer: addi sp, sp, frame_size
            code.emit(RiscvInst::Addi {
                rd: Gpr::SP,
                rs1: Gpr::SP,
                imm: frame_size as i32,
            });
        }

        // Return: jalr x0, ra, 0
        code.emit(RiscvInst::Jalr {
            rd: Gpr::ZERO,
            rs1: Gpr::RA,
            imm: 0,
        });
    }

    /// Emit a spill or reload operation.
    fn emit_spill_reload(
        &mut self,
        code: &mut CodeBuffer,
        op: &SpillReloadOp,
        frame_layout: &FrameLayout,
    ) {
        match op {
            SpillReloadOp::Spill { reg, slot, .. } => {
                let offset = frame_layout.spill_slot_offset(*slot);
                code.emit(RiscvInst::Sw {
                    rs1: Gpr::SP,
                    rs2: *reg,
                    imm: offset,
                });
            }
            SpillReloadOp::Reload { reg, slot, .. } => {
                let offset = frame_layout.spill_slot_offset(*slot);
                code.emit(RiscvInst::Lw {
                    rd: *reg,
                    rs1: Gpr::SP,
                    imm: offset,
                });
            }
        }
    }

    /// Get register for a value, or None if spilled.
    fn get_register(&self, value: Value, allocation: &RegisterAllocation) -> Option<Gpr> {
        allocation.value_to_reg.get(&value).copied()
    }

    /// Get spill slot for a value, or None if in register.
    fn get_spill_slot(&self, value: Value, allocation: &RegisterAllocation) -> Option<u32> {
        allocation.value_to_slot.get(&value).copied()
    }

    /// Load a value into a register (handles spills).
    fn load_value_into_reg(
        &mut self,
        code: &mut CodeBuffer,
        value: Value,
        target_reg: Gpr,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) {
        if let Some(reg) = self.get_register(value, allocation) {
            // Value is in a register - move it if needed
            if reg != target_reg {
                code.emit(RiscvInst::Addi {
                    rd: target_reg,
                    rs1: reg,
                    imm: 0, // Move: addi rd, rs, 0
                });
            }
        } else if let Some(slot) = self.get_spill_slot(value, allocation) {
            // Value is spilled - reload it
            let offset = frame_layout.spill_slot_offset(slot);
            code.emit(RiscvInst::Lw {
                rd: target_reg,
                rs1: Gpr::SP,
                imm: offset,
            });
        } else {
            // Value not found - this shouldn't happen
            panic!("Value {:?} not found in allocation", value);
        }
    }

    /// Lower a single instruction.
    fn lower_inst(
        &mut self,
        code: &mut CodeBuffer,
        inst: &Inst,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
        block_addresses: &BTreeMap<usize, u32>,
    ) {
        match inst {
            Inst::Iconst { result, value } => {
                self.lower_iconst(code, *result, *value, allocation);
            }
            Inst::Iadd { result, arg1, arg2 } => {
                self.lower_iadd(code, *result, *arg1, *arg2, allocation, frame_layout);
            }
            Inst::Isub { result, arg1, arg2 } => {
                self.lower_isub(code, *result, *arg1, *arg2, allocation, frame_layout);
            }
            Inst::Imul { result, arg1, arg2 } => {
                self.lower_imul(code, *result, *arg1, *arg2, allocation, frame_layout);
            }
            Inst::Return { values } => {
                self.lower_return(code, values, allocation, frame_layout, abi_info);
            }
            Inst::Call {
                callee,
                args,
                results,
            } => {
                self.lower_call(
                    code,
                    callee,
                    args,
                    results,
                    allocation,
                    frame_layout,
                    abi_info,
                );
            }
            Inst::Jump { target } => {
                self.lower_jump(code, *target, block_addresses);
            }
            Inst::Br {
                condition,
                target_true,
                target_false,
            } => {
                self.lower_br(
                    code,
                    *condition,
                    *target_true,
                    *target_false,
                    allocation,
                    frame_layout,
                    block_addresses,
                );
            }
            Inst::Halt => {
                code.emit(RiscvInst::Ebreak);
            }
            Inst::Syscall { number, args } => {
                self.lower_syscall(code, *number, args, allocation, frame_layout, abi_info);
            }
            // TODO: Implement other instructions (Idiv, Irem, Load, Store, etc.)
            _ => {
                panic!("Unimplemented instruction: {:?}", inst);
            }
        }
    }

    /// Lower iconst instruction.
    fn lower_iconst(
        &mut self,
        code: &mut CodeBuffer,
        result: Value,
        value: i64,
        allocation: &RegisterAllocation,
    ) {
        let result_reg = self
            .get_register(result, allocation)
            .expect("iconst result must be in register");

        // Handle large constants (require lui + addi)
        if value >= -(1 << 11) && value < (1 << 11) {
            // Small constant: addi rd, x0, imm
            code.emit(RiscvInst::Addi {
                rd: result_reg,
                rs1: Gpr::ZERO,
                imm: value as i32,
            });
        } else {
            // Large constant: lui + addi
            let imm = value as u32;
            let lui_imm = (imm >> 12) & 0xfffff;
            let addi_imm = (imm & 0xfff) as i32;
            let final_lui_imm = if (addi_imm & 0x800) != 0 {
                // Sign extend: if addi_imm is negative, increment lui_imm
                lui_imm + 1
            } else {
                lui_imm
            };

            code.emit(RiscvInst::Lui {
                rd: result_reg,
                imm: final_lui_imm,
            });
            code.emit(RiscvInst::Addi {
                rd: result_reg,
                rs1: result_reg,
                imm: addi_imm,
            });
        }
    }

    /// Lower iadd instruction.
    fn lower_iadd(
        &mut self,
        code: &mut CodeBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) {
        let result_reg = self
            .get_register(result, allocation)
            .expect("iadd result must be in register");

        // Load operands into registers
        let arg1_reg = self.get_register(arg1, allocation).unwrap_or_else(|| {
            // Load spilled arg1 into result_reg or temp
            if result_reg == Gpr::T0 {
                // Can't use T0, use T1
                self.load_value_into_reg(code, arg1, Gpr::T1, allocation, frame_layout);
                Gpr::T1
            } else {
                self.load_value_into_reg(code, arg1, result_reg, allocation, frame_layout);
                result_reg
            }
        });

        let arg2_reg = self.get_register(arg2, allocation).unwrap_or_else(|| {
            // Load spilled arg2 into a temp register
            let temp = if arg1_reg == Gpr::T0 {
                Gpr::T1
            } else {
                Gpr::T0
            };
            self.load_value_into_reg(code, arg2, temp, allocation, frame_layout);
            temp
        });

        // If arg1 is in result_reg, we can use it directly
        // Otherwise, move arg1 to result_reg first
        if arg1_reg != result_reg {
            code.emit(RiscvInst::Addi {
                rd: result_reg,
                rs1: arg1_reg,
                imm: 0, // Move
            });
        }

        // Add arg2 to result_reg
        code.emit(RiscvInst::Add {
            rd: result_reg,
            rs1: result_reg,
            rs2: arg2_reg,
        });
    }

    /// Lower isub instruction.
    fn lower_isub(
        &mut self,
        code: &mut CodeBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) {
        let result_reg = self
            .get_register(result, allocation)
            .expect("isub result must be in register");

        let arg1_reg = self.get_register(arg1, allocation).unwrap_or_else(|| {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, arg1, temp, allocation, frame_layout);
            temp
        });
        let arg2_reg = self.get_register(arg2, allocation).unwrap_or_else(|| {
            let temp = Gpr::T1;
            self.load_value_into_reg(code, arg2, temp, allocation, frame_layout);
            temp
        });

        code.emit(RiscvInst::Sub {
            rd: result_reg,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
    }

    /// Lower imul instruction.
    fn lower_imul(
        &mut self,
        code: &mut CodeBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) {
        let result_reg = self
            .get_register(result, allocation)
            .expect("imul result must be in register");

        let arg1_reg = self.get_register(arg1, allocation).unwrap_or_else(|| {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, arg1, temp, allocation, frame_layout);
            temp
        });
        let arg2_reg = self.get_register(arg2, allocation).unwrap_or_else(|| {
            let temp = Gpr::T1;
            self.load_value_into_reg(code, arg2, temp, allocation, frame_layout);
            temp
        });

        code.emit(RiscvInst::Mul {
            rd: result_reg,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
    }

    /// Lower return instruction.
    fn lower_return(
        &mut self,
        code: &mut CodeBuffer,
        values: &[Value],
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) {
        // Move return values to return registers
        for (idx, value) in values.iter().enumerate() {
            if let Some(return_reg) = abi_info.return_regs.get(&idx) {
                self.load_value_into_reg(code, *value, *return_reg, allocation, frame_layout);
            }
            // TODO: Handle stack returns (> 8 return values)
        }
    }

    /// Lower call instruction.
    fn lower_call(
        &mut self,
        code: &mut CodeBuffer,
        callee: &str,
        args: &[Value],
        results: &[Value],
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) {
        // Move arguments to argument registers
        for (idx, arg) in args.iter().enumerate() {
            if let Some(arg_reg) = abi_info.param_regs.get(&idx) {
                self.load_value_into_reg(code, *arg, *arg_reg, allocation, frame_layout);
            }
            // TODO: Handle stack arguments (> 8 args)
        }

        // Emit call (jalr or jal depending on whether we know the address)
        if let Some(address) = self.function_addresses.get(callee) {
            // Direct call - use jal
            let current_pc = code.instruction_count() as u32 * 4;
            let offset = (*address as i32) - (current_pc as i32);
            code.emit(RiscvInst::Jal {
                rd: Gpr::RA,
                imm: offset,
            });
        } else {
            // Indirect call - need relocation
            let offset = code.instruction_count();
            self.relocations.push(Relocation {
                offset,
                callee: String::from(callee),
            });
            // Emit placeholder jal (will be fixed up later)
            code.emit(RiscvInst::Jal {
                rd: Gpr::RA,
                imm: 0, // Placeholder
            });
        }

        // Move results from return registers
        for (idx, result) in results.iter().enumerate() {
            if let Some(return_reg) = abi_info.return_regs.get(&idx) {
                if let Some(result_reg) = self.get_register(*result, allocation) {
                    if result_reg != *return_reg {
                        code.emit(RiscvInst::Addi {
                            rd: result_reg,
                            rs1: *return_reg,
                            imm: 0, // Move
                        });
                    }
                } else {
                    // Result is spilled - store return register to spill slot
                    if let Some(slot) = self.get_spill_slot(*result, allocation) {
                        let offset = frame_layout.spill_slot_offset(slot);
                        code.emit(RiscvInst::Sw {
                            rs1: Gpr::SP,
                            rs2: *return_reg,
                            imm: offset,
                        });
                    }
                }
            }
            // TODO: Handle stack returns
        }
    }

    /// Lower jump instruction.
    fn lower_jump(
        &mut self,
        code: &mut CodeBuffer,
        target: u32,
        block_addresses: &BTreeMap<usize, u32>,
    ) {
        let target_idx = target as usize;
        if let Some(target_addr) = block_addresses.get(&target_idx) {
            let current_pc = code.instruction_count() as u32 * 4;
            let offset = (*target_addr as i32) - (current_pc as i32);
            code.emit(RiscvInst::Jal {
                rd: Gpr::ZERO, // Discard return address
                imm: offset,
            });
        } else {
            // Target block not yet processed - use placeholder
            // This will be fixed up in a second pass
            code.emit(RiscvInst::Jal {
                rd: Gpr::ZERO,
                imm: 0, // Placeholder
            });
        }
    }

    /// Lower branch instruction.
    fn lower_br(
        &mut self,
        code: &mut CodeBuffer,
        condition: Value,
        target_true: u32,
        target_false: u32,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        block_addresses: &BTreeMap<usize, u32>,
    ) {
        // Load condition into a register
        let cond_reg = self.get_register(condition, allocation).unwrap_or_else(|| {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, condition, temp, allocation, frame_layout);
            temp
        });

        // Get target addresses
        let true_addr = block_addresses.get(&(target_true as usize));
        let false_addr = block_addresses.get(&(target_false as usize));
        let current_pc = code.instruction_count() as u32 * 4;

        // Emit branch: beq cond_reg, x0, false_target; jump true_target
        if let Some(false_addr) = false_addr {
            let offset = (*false_addr as i32) - (current_pc as i32);
            code.emit(RiscvInst::Beq {
                rs1: cond_reg,
                rs2: Gpr::ZERO,
                imm: offset,
            });
        }

        // Jump to true target
        if let Some(true_addr) = true_addr {
            let offset = (*true_addr as i32) - (current_pc as i32);
            code.emit(RiscvInst::Jal {
                rd: Gpr::ZERO,
                imm: offset,
            });
        }
    }

    /// Lower syscall instruction.
    fn lower_syscall(
        &mut self,
        code: &mut CodeBuffer,
        number: i32,
        args: &[Value],
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) {
        // Move arguments to a0-a7 registers
        for (idx, arg) in args.iter().enumerate() {
            if idx < 8 {
                let arg_reg = Gpr::new(10 + idx as u8); // a0-a7
                self.load_value_into_reg(code, *arg, arg_reg, allocation, frame_layout);
            }
            // TODO: Handle > 8 args (stack arguments)
        }

        // Move syscall number to a7 (last argument register)
        if number < (1 << 12) {
            // Small immediate: addi a7, zero, number
            code.emit(RiscvInst::Addi {
                rd: Gpr::A7,
                rs1: Gpr::ZERO,
                imm: number,
            });
        } else {
            // Large immediate: lui + addi
            let imm = number as u32;
            let lui_imm = (imm >> 12) & 0xfffff;
            let addi_imm = (imm & 0xfff) as i32;
            code.emit(RiscvInst::Lui {
                rd: Gpr::A7,
                imm: lui_imm,
            });
            code.emit(RiscvInst::Addi {
                rd: Gpr::A7,
                rs1: Gpr::A7,
                imm: addi_imm,
            });
        }

        // Emit ecall
        code.emit(RiscvInst::Ecall);
    }
}

impl Default for Lowerer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use r5_ir::parse_function;

    use super::*;
    use crate::{
        abi::Abi, frame::FrameLayout, liveness::compute_liveness, regalloc::allocate_registers,
        spill_reload::create_spill_reload_plan,
    };

    #[test]
    fn test_lower_simple_add() {
        // Simple function: v0 = iconst 1; v1 = iconst 2; v2 = iadd v0, v1; return v2
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = iconst 2
    v2 = iadd v0, v1
    return v2
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            allocation.spill_slot_count,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation);

        let mut lowerer = Lowerer::new();
        let code =
            lowerer.lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info);

        // Should have generated some code
        assert!(code.instruction_count() > 0);

        // Should have prologue, instructions, and epilogue
        let instructions = code.instructions();
        assert!(!instructions.is_empty());
    }

    #[test]
    fn test_lower_iconst() {
        // Function with iconst
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 42
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            allocation.spill_slot_count,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation);

        let mut lowerer = Lowerer::new();
        let code =
            lowerer.lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info);

        assert!(code.instruction_count() > 0);
    }

    #[test]
    fn test_lower_return() {
        // Function that returns a value
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 10
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            allocation.spill_slot_count,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation);

        let mut lowerer = Lowerer::new();
        let code =
            lowerer.lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info);

        assert!(code.instruction_count() > 0);

        // Should end with jalr (return)
        let instructions = code.instructions();
        assert!(matches!(instructions.last(), Some(RiscvInst::Jalr { .. })));
    }
}
