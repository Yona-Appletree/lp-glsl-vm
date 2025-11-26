//! Instruction lowering (IR → RISC-V).
//!
//! This module lowers IR instructions to RISC-V instructions using pre-computed
//! register allocation, spill/reload plan, and frame layout.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use r5_ir::{Function, Inst, Value};
use riscv32_encoder::{Gpr, Inst as RiscvInst};

use crate::{
    abi::{Abi, AbiInfo},
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

/// Lowering error.
#[derive(Debug, Clone)]
pub enum LoweringError {
    /// Value not found in register allocation
    ValueNotAllocated { value: Value },
    /// Unimplemented instruction
    UnimplementedInstruction { inst: Inst },
    /// Result value must be in register (internal error)
    ResultNotInRegister { value: Value },
}

impl core::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoweringError::ValueNotAllocated { value } => {
                write!(f, "Value {:?} not found in allocation", value)
            }
            LoweringError::UnimplementedInstruction { inst } => {
                write!(f, "Unimplemented instruction: {:?}", inst)
            }
            LoweringError::ResultNotInRegister { value } => {
                write!(f, "Result value {:?} must be in register", value)
            }
        }
    }
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
    ) -> Result<CodeBuffer, LoweringError> {
        let mut code = CodeBuffer::new();

        // 1. Generate prologue
        self.gen_prologue(&mut code, func, allocation, frame_layout, abi_info);

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
                )?;

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

        Ok(code)
    }

    /// Generate function prologue.
    fn gen_prologue(
        &mut self,
        code: &mut CodeBuffer,
        func: &Function,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) {
        let frame_size = frame_layout.total_size();

        // Step 1: Load incoming stack arguments (before SP adjustment)
        // Stack args are at positive offsets from SP before prologue
        if let Some(entry_block) = func.blocks.first() {
            let mut stack_args_to_spill = Vec::new();
            for (idx, param) in entry_block.params.iter().enumerate() {
                if let Some(stack_offset) = abi_info.param_stack_offsets.get(&idx) {
                    // This parameter is on the stack
                    if let Some(allocated_reg) = allocation.value_to_reg.get(param) {
                        // Load directly into allocated register
                        code.emit(RiscvInst::Lw {
                            rd: *allocated_reg,
                            rs1: Gpr::SP,
                            imm: *stack_offset, // Positive offset
                        });
                    } else {
                        // Will be spilled - load into temp register, store after SP adjustment
                        let temp_reg = Gpr::T0;
                        code.emit(RiscvInst::Lw {
                            rd: temp_reg,
                            rs1: Gpr::SP,
                            imm: *stack_offset, // Positive offset
                        });
                        // Store temp_reg and param for later
                        if let Some(slot) = allocation.value_to_slot.get(param) {
                            stack_args_to_spill.push((temp_reg, *slot));
                        }
                    }
                }
            }

            // Step 2: Adjust SP for entire frame
            if frame_size > 0 {
                code.emit(RiscvInst::Addi {
                    rd: Gpr::SP,
                    rs1: Gpr::SP,
                    imm: -(frame_size as i32),
                });

                // Step 3: Store spilled stack args to their spill slots
                for (temp_reg, slot) in stack_args_to_spill {
                    let offset = frame_layout.spill_slot_offset(slot);
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::SP,
                        rs2: temp_reg,
                        imm: offset,
                    });
                }

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
        } else if frame_size > 0 {
            // No entry block, but still need to adjust SP
            code.emit(RiscvInst::Addi {
                rd: Gpr::SP,
                rs1: Gpr::SP,
                imm: -(frame_size as i32),
            });

            // Save return address if we have calls
            if frame_layout.has_function_calls {
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

            // Save callee-saved registers
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
    ) -> Result<(), LoweringError> {
        if let Some(reg) = self.get_register(value, allocation) {
            // Value is in a register - move it if needed
            if reg != target_reg {
                code.emit(RiscvInst::Addi {
                    rd: target_reg,
                    rs1: reg,
                    imm: 0, // Move: addi rd, rs, 0
                });
            }
            Ok(())
        } else if let Some(slot) = self.get_spill_slot(value, allocation) {
            // Value is spilled - reload it
            let offset = frame_layout.spill_slot_offset(slot);
            code.emit(RiscvInst::Lw {
                rd: target_reg,
                rs1: Gpr::SP,
                imm: offset,
            });
            Ok(())
        } else {
            // Value not found - this shouldn't happen with correct allocation
            Err(LoweringError::ValueNotAllocated { value })
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
    ) -> Result<(), LoweringError> {
        match inst {
            Inst::Iconst { result, value } => {
                self.lower_iconst(code, *result, *value, allocation)?;
            }
            Inst::Iadd { result, arg1, arg2 } => {
                self.lower_iadd(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::Isub { result, arg1, arg2 } => {
                self.lower_isub(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::Imul { result, arg1, arg2 } => {
                self.lower_imul(code, *result, *arg1, *arg2, allocation, frame_layout)?;
            }
            Inst::Return { values } => {
                self.lower_return(code, values, allocation, frame_layout, abi_info)?;
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
                )?;
            }
            Inst::Jump { target } => {
                self.lower_jump(code, *target, block_addresses)?;
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
                )?;
            }
            Inst::Halt => {
                code.emit(RiscvInst::Ebreak);
            }
            Inst::Syscall { number, args } => {
                self.lower_syscall(code, *number, args, allocation, frame_layout)?;
            }
            // Known limitation: Some instructions are not yet implemented
            // (Idiv, Irem, Load, Store, etc.). These will return an error.
            _ => {
                return Err(LoweringError::UnimplementedInstruction { inst: inst.clone() });
            }
        }
        Ok(())
    }

    /// Lower iconst instruction.
    fn lower_iconst(
        &mut self,
        code: &mut CodeBuffer,
        result: Value,
        value: i64,
        allocation: &RegisterAllocation,
    ) -> Result<(), LoweringError> {
        // For result values, they must be in registers
        // If not in allocation at all, return ValueNotAllocated
        // If in allocation but not in register (spilled), return ResultNotInRegister
        let result_reg = if let Some(reg) = self.get_register(result, allocation) {
            reg
        } else if !allocation.value_to_reg.contains_key(&result)
            && !allocation.value_to_slot.contains_key(&result)
        {
            // Result is not in allocation at all
            return Err(LoweringError::ValueNotAllocated { value: result });
        } else {
            // Result is in allocation but not in register (spilled) - result values should always be in registers
            return Err(LoweringError::ResultNotInRegister { value: result });
        };

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
        Ok(())
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
    ) -> Result<(), LoweringError> {
        // For result values, they must be in registers
        // If not in allocation at all, return ValueNotAllocated
        // If in allocation but not in register (spilled), return ResultNotInRegister
        let result_reg = if let Some(reg) = self.get_register(result, allocation) {
            reg
        } else if !allocation.value_to_reg.contains_key(&result)
            && !allocation.value_to_slot.contains_key(&result)
        {
            // Result is not in allocation at all
            return Err(LoweringError::ValueNotAllocated { value: result });
        } else {
            // Result is in allocation but not in register (spilled) - result values should always be in registers
            return Err(LoweringError::ResultNotInRegister { value: result });
        };

        // Load operands into registers
        let arg1_reg = if let Some(reg) = self.get_register(arg1, allocation) {
            reg
        } else {
            // Load spilled arg1 into result_reg or temp
            if result_reg == Gpr::T0 {
                // Can't use T0, use T1
                self.load_value_into_reg(code, arg1, Gpr::T1, allocation, frame_layout)?;
                Gpr::T1
            } else {
                self.load_value_into_reg(code, arg1, result_reg, allocation, frame_layout)?;
                result_reg
            }
        };

        let arg2_reg = if let Some(reg) = self.get_register(arg2, allocation) {
            reg
        } else {
            // Load spilled arg2 into a temp register
            let temp = if arg1_reg == Gpr::T0 {
                Gpr::T1
            } else {
                Gpr::T0
            };
            self.load_value_into_reg(code, arg2, temp, allocation, frame_layout)?;
            temp
        };

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
        Ok(())
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
    ) -> Result<(), LoweringError> {
        // For result values, check if in allocation first
        // If not in allocation at all, return ValueNotAllocated
        // If in allocation but not in register (spilled), return ResultNotInRegister
        let result_reg = if let Some(reg) = self.get_register(result, allocation) {
            reg
        } else if allocation.value_to_slot.contains_key(&result) {
            // Result is spilled - result values should always be in registers
            return Err(LoweringError::ResultNotInRegister { value: result });
        } else {
            // Result is not in allocation at all
            return Err(LoweringError::ValueNotAllocated { value: result });
        };

        let arg1_reg = if let Some(reg) = self.get_register(arg1, allocation) {
            reg
        } else {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, arg1, temp, allocation, frame_layout)?;
            temp
        };
        let arg2_reg = if let Some(reg) = self.get_register(arg2, allocation) {
            reg
        } else {
            let temp = Gpr::T1;
            self.load_value_into_reg(code, arg2, temp, allocation, frame_layout)?;
            temp
        };

        code.emit(RiscvInst::Sub {
            rd: result_reg,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
        Ok(())
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
    ) -> Result<(), LoweringError> {
        // For result values, check if in allocation first
        // If not in allocation at all, return ValueNotAllocated
        // If in allocation but not in register (spilled), return ResultNotInRegister
        let result_reg = if let Some(reg) = self.get_register(result, allocation) {
            reg
        } else if allocation.value_to_slot.contains_key(&result) {
            // Result is spilled - result values should always be in registers
            return Err(LoweringError::ResultNotInRegister { value: result });
        } else {
            // Result is not in allocation at all
            return Err(LoweringError::ValueNotAllocated { value: result });
        };

        let arg1_reg = if let Some(reg) = self.get_register(arg1, allocation) {
            reg
        } else {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, arg1, temp, allocation, frame_layout)?;
            temp
        };
        let arg2_reg = if let Some(reg) = self.get_register(arg2, allocation) {
            reg
        } else {
            let temp = Gpr::T1;
            self.load_value_into_reg(code, arg2, temp, allocation, frame_layout)?;
            temp
        };

        code.emit(RiscvInst::Mul {
            rd: result_reg,
            rs1: arg1_reg,
            rs2: arg2_reg,
        });
        Ok(())
    }

    /// Lower return instruction.
    fn lower_return(
        &mut self,
        code: &mut CodeBuffer,
        values: &[Value],
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
        abi_info: &AbiInfo,
    ) -> Result<(), LoweringError> {
        // Move return values to return registers (first 8)
        for (idx, value) in values.iter().enumerate() {
            if let Some(return_reg) = abi_info.return_regs.get(&idx) {
                self.load_value_into_reg(code, *value, *return_reg, allocation, frame_layout)?;
            }
        }

        // Store stack return values (index >= 8) to stack
        // These are stored at positive offsets from SP (before epilogue)
        for (idx, value) in values.iter().enumerate() {
            if idx >= 8 {
                if let Some(stack_offset) = abi_info.return_stack_offsets.get(&idx) {
                    // Load value into temp register
                    let temp_reg = Gpr::T0;
                    self.load_value_into_reg(code, *value, temp_reg, allocation, frame_layout)?;

                    // Store to stack (offset relative to SP before epilogue)
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::SP,
                        rs2: temp_reg,
                        imm: *stack_offset, // Positive offset
                    });
                }
            }
        }
        Ok(())
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
    ) -> Result<(), LoweringError> {
        // Step 1: Move register arguments (a0-a7)
        // Track which argument values were preserved (because they're used after the call)
        let mut preserved_args: Vec<(Value, Gpr)> = Vec::new();

        for (idx, arg) in args.iter().enumerate() {
            if idx < 8 {
                if let Some(arg_reg) = Abi::arg_reg(idx) {
                    // Check if this value is used after the call
                    // A value is used after if it's in results OR if it's allocated
                    // (allocated values are tracked and likely used later)
                    // Simple heuristic: if it's allocated and not just used as argument, preserve it
                    let used_after_call =
                        results.contains(arg) || allocation.value_to_reg.contains_key(arg);

                    // Check if value is already in the argument register
                    if let Some(current_reg) = self.get_register(*arg, allocation) {
                        if current_reg == arg_reg && used_after_call {
                            // Value is in arg_reg and used after call - need to preserve it
                            // Save to a temporary register before the call
                            // Use T2 as temp (T0/T1 might be used for other things)
                            let temp_reg = Gpr::T2;
                            code.emit(RiscvInst::Addi {
                                rd: temp_reg,
                                rs1: current_reg,
                                imm: 0, // Copy: addi rd, rs, 0
                            });

                            // Track this for restoration after call
                            preserved_args.push((*arg, temp_reg));
                            // Skip moving since it's already in place
                            continue;
                        }
                    }

                    self.load_value_into_reg(code, *arg, arg_reg, allocation, frame_layout)?;
                }
            }
        }

        // Step 2: Store stack arguments (index >= 8) to outgoing args area
        for (idx, arg) in args.iter().enumerate() {
            if idx >= 8 {
                if let Some(offset) = frame_layout.outgoing_arg_offset(idx) {
                    // Load argument value into temporary register
                    let temp_reg = Gpr::T0;
                    self.load_value_into_reg(code, *arg, temp_reg, allocation, frame_layout)?;

                    // Store to outgoing args area
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::SP,
                        rs2: temp_reg,
                        imm: offset, // Negative offset
                    });
                }
            }
        }

        // Emit call - always use relocation for cross-function calls
        // The direct call optimization doesn't work correctly because we don't know
        // the absolute address of the current function during lowering.
        // Relocations will be fixed up in the final pass with correct absolute addresses.
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

        // Step 3: Move results from return registers (first 8)
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
        }

        // Step 4: Load stack return values (index >= 8) from stack
        // These are stored in the caller's frame at positive offsets from SP
        // After the call returns, the callee's epilogue has restored SP to the caller's frame,
        // so the return values are at positive offsets from SP (just stack_offset)
        for (idx, result) in results.iter().enumerate() {
            if idx >= 8 {
                if let Some(stack_offset) = abi_info.return_stack_offsets.get(&idx) {
                    // After call returns, SP is restored to caller's frame, so offset is just stack_offset
                    // (positive offset, relative to SP after epilogue)
                    let actual_offset = *stack_offset;

                    // Load from stack into temp register
                    let temp_reg = Gpr::T0;
                    code.emit(RiscvInst::Lw {
                        rd: temp_reg,
                        rs1: Gpr::SP,
                        imm: actual_offset,
                    });

                    // Store to result location (register or spill slot)
                    if let Some(result_reg) = self.get_register(*result, allocation) {
                        code.emit(RiscvInst::Addi {
                            rd: result_reg,
                            rs1: temp_reg,
                            imm: 0, // Move
                        });
                    } else if let Some(slot) = self.get_spill_slot(*result, allocation) {
                        let offset = frame_layout.spill_slot_offset(slot);
                        code.emit(RiscvInst::Sw {
                            rs1: Gpr::SP,
                            rs2: temp_reg,
                            imm: offset,
                        });
                    }
                }
            }
        }

        // Step 5: Restore preserved argument values that were used after the call
        for (arg_value, temp_reg) in preserved_args {
            // Restore to the value's allocated location (register or spill slot)
            if let Some(result_reg) = self.get_register(arg_value, allocation) {
                code.emit(RiscvInst::Addi {
                    rd: result_reg,
                    rs1: temp_reg,
                    imm: 0, // Move: addi rd, rs, 0
                });
            } else if let Some(slot) = self.get_spill_slot(arg_value, allocation) {
                let offset = frame_layout.spill_slot_offset(slot);
                code.emit(RiscvInst::Sw {
                    rs1: Gpr::SP,
                    rs2: temp_reg,
                    imm: offset,
                });
            }
        }

        Ok(())
    }

    /// Lower jump instruction.
    fn lower_jump(
        &mut self,
        code: &mut CodeBuffer,
        target: u32,
        block_addresses: &BTreeMap<usize, u32>,
    ) -> Result<(), LoweringError> {
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
        Ok(())
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
    ) -> Result<(), LoweringError> {
        // Load condition into a register
        let cond_reg = if let Some(reg) = self.get_register(condition, allocation) {
            reg
        } else {
            let temp = Gpr::T0;
            self.load_value_into_reg(code, condition, temp, allocation, frame_layout)?;
            temp
        };

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
        Ok(())
    }

    /// Lower syscall instruction.
    fn lower_syscall(
        &mut self,
        code: &mut CodeBuffer,
        number: i32,
        args: &[Value],
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    ) -> Result<(), LoweringError> {
        // Move arguments to a0-a7 registers
        for (idx, arg) in args.iter().enumerate() {
            if idx < 8 {
                let arg_reg = Gpr::new(10 + idx as u8); // a0-a7
                self.load_value_into_reg(code, *arg, arg_reg, allocation, frame_layout)?;
            }
            // Known limitation: Syscalls with > 8 arguments are not yet supported
            // (stack arguments for syscalls would need additional implementation)
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
        Ok(())
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
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

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
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

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
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        assert!(code.instruction_count() > 0);

        // Should end with jalr (return)
        let instructions = code.instructions();
        assert!(matches!(instructions.last(), Some(RiscvInst::Jalr { .. })));
    }

    #[test]
    fn test_lower_error_missing_value() {
        // Test that missing values in allocation return an error
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
        let mut allocation = allocate_registers(&func, &liveness);

        // Remove a value from allocation to simulate error
        let v0 = r5_ir::Value::new(0);
        allocation.value_to_reg.remove(&v0);
        allocation.value_to_slot.remove(&v0);

        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            allocation.spill_slot_count,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let result =
            lowerer.lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info);

        // Should return an error
        assert!(result.is_err());
        match result {
            Err(LoweringError::ValueNotAllocated { value }) => {
                assert_eq!(value, v0);
            }
            Err(e) => panic!("Expected ValueNotAllocated error, got {:?}", e),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[test]
    fn test_lower_error_result_not_in_register() {
        // Test that result values must be in registers
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let mut allocation = allocate_registers(&func, &liveness);

        // Remove result value from register allocation (but keep it spilled)
        let v0 = r5_ir::Value::new(0);
        allocation.value_to_reg.remove(&v0);
        // Add it to value_to_slot to simulate a spilled result value
        if !allocation.value_to_slot.contains_key(&v0) {
            allocation.value_to_slot.insert(v0, 0);
        }

        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        // Include temporary spill slots needed for caller-saved register preservation
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let result =
            lowerer.lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info);

        // Should return an error for result not in register
        assert!(result.is_err());
        match result {
            Err(LoweringError::ResultNotInRegister { value }) => {
                assert_eq!(value, v0);
            }
            Err(e) => panic!("Expected ResultNotInRegister error, got {:?}", e),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[test]
    fn test_lower_error_unimplemented_instruction() {
        // Test that unimplemented instructions return an error
        let ir = r#"
function %test() -> i32 {
block0:
    v0 = iconst 1
    v1 = idiv v0, v0
    return v1
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = false;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let result =
            lowerer.lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info);

        // Should return an error for unimplemented instruction
        assert!(result.is_err());
        match result {
            Err(LoweringError::UnimplementedInstruction { .. }) => {
                // Expected
            }
            Err(e) => panic!("Expected UnimplementedInstruction error, got {:?}", e),
            Ok(_) => panic!("Expected error but got Ok"),
        }
    }

    #[test]
    fn test_prologue_emits_valid_instructions() {
        // Test that prologue emits only valid RISC-V instructions
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
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        // Check that all instructions are valid
        let instructions = code.instructions();
        for (idx, inst) in instructions.iter().enumerate() {
            let encoded = inst.encode();

            // Check that encoded instruction is not zero (except for very specific cases)
            // Zero is not a valid RISC-V instruction
            if encoded == 0 {
                panic!(
                    "Instruction {} at index {} encodes to zero (invalid): {:?}",
                    idx, idx, inst
                );
            }

            // Check that opcode is valid (not 0x00)
            let opcode = encoded & 0x7f;
            if opcode == 0 {
                panic!(
                    "Instruction {} at index {} has invalid opcode 0x00: encoded=0x{:08x}, \
                     inst={:?}",
                    idx, idx, encoded, inst
                );
            }
        }
    }

    #[test]
    fn test_prologue_instruction_sequence() {
        // Test that prologue emits correct sequence of instructions
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
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            0,
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 0);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        let instructions = code.instructions();
        let encoded: Vec<u32> = instructions.iter().map(|i| i.encode()).collect();

        // Check that no instruction encodes to 0x00030000 or similar invalid values
        for (idx, enc) in encoded.iter().enumerate() {
            if *enc == 0x00030000 {
                panic!(
                    "Found invalid instruction 0x00030000 at index {}: {:?}",
                    idx,
                    instructions.get(idx)
                );
            }

            // Check for other suspicious patterns
            let opcode = enc & 0x7f;
            if opcode == 0 && *enc != 0 {
                panic!(
                    "Found instruction with invalid opcode 0x00 at index {}: encoded=0x{:08x}, \
                     inst={:?}",
                    idx,
                    enc,
                    instructions.get(idx)
                );
            }
        }
    }

    #[test]
    fn test_function_with_call_prologue() {
        // Test prologue for function that makes calls
        let ir = r#"
function %test() -> i32 {
block0:
    call %helper() -> v0
    return v0
}"#;

        let func = parse_function(ir).expect("Failed to parse IR function");
        let liveness = compute_liveness(&func);
        let allocation = allocate_registers(&func, &liveness);
        let spill_reload = create_spill_reload_plan(&func, &allocation, &liveness);

        let has_calls = true;
        let total_spill_slots = allocation.spill_slot_count + spill_reload.max_temp_spill_slots;
        let frame_layout = FrameLayout::compute(
            &allocation.used_callee_saved,
            total_spill_slots,
            has_calls,
            func.signature.params.len(),
            8, // Max outgoing args
        );

        let abi_info = Abi::compute_abi_info(&func, &allocation, 8);

        let mut lowerer = Lowerer::new();
        let code = lowerer
            .lower_function(&func, &allocation, &spill_reload, &frame_layout, &abi_info)
            .expect("Failed to lower function");

        // Check that prologue instructions are valid
        let instructions = code.instructions();
        for (idx, inst) in instructions.iter().enumerate() {
            let encoded = inst.encode();
            let opcode = encoded & 0x7f;

            if opcode == 0 && encoded != 0 {
                panic!(
                    "Invalid instruction at index {}: encoded=0x{:08x}, inst={:?}",
                    idx, encoded, inst
                );
            }
        }
    }
}
