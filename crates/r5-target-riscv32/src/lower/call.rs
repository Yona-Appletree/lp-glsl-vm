//! Function call instruction lowering.

use r5_ir::Value;
use riscv32_encoder::{Gpr, Inst as RiscvInst};

use super::types::{LoweringError, Relocation, RelocationInstType, RelocationTarget};
use crate::{
    abi::{Abi, AbiInfo},
    emit::CodeBuffer,
    frame::FrameLayout,
    regalloc::RegisterAllocation,
};

impl super::Lowerer {
    /// Lower call instruction.
    pub(super) fn lower_call(
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
        let mut preserved_args: alloc::vec::Vec<(Value, Gpr)> = alloc::vec::Vec::new();

        for (idx, arg) in args.iter().enumerate() {
            if idx < 8 {
                if let Some(arg_reg) = Abi::arg_reg(idx) {
                    // Check if this value is used after the call
                    // Only preserve if the argument appears in the results (meaning it's
                    // passed through and used as a return value). For other cases where
                    // the argument is used later, the register allocator should have
                    // allocated it to a callee-saved register or spilled it.
                    let used_after_call = results.contains(arg);

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
        // Outgoing stack args are stored in the outgoing args area at the top of the frame.
        // After prologue, SP points to the bottom of the frame. The callee reads incoming
        // args at SP + offset before adjusting SP, where SP = caller's SP_after_prologue.
        // The outgoing args area starts at: SP + (total_size - outgoing_args_size)
        // Then we add the per-argument offset (0, 4, 8, ...) to get the specific argument.
        let outgoing_args_base = (frame_layout.total_size() - frame_layout.outgoing_args_size) as i32;
        for (idx, arg) in args.iter().enumerate() {
            if idx >= 8 {
                if let Some(offset) = frame_layout.outgoing_arg_offset(idx) {
                    // Load argument value into temporary register
                    let temp_reg = Gpr::T0;
                    self.load_value_into_reg(code, *arg, temp_reg, allocation, frame_layout)?;

                    // Store to outgoing args area
                    // Stack arguments are stored at the top of the frame in the outgoing args area.
                    // We compute the offset from SP (bottom) to the start of outgoing args area,
                    // then add the per-argument offset.
                    let actual_offset = outgoing_args_base + offset.as_i32();
                    code.emit(RiscvInst::Sw {
                        rs1: Gpr::SP,
                        rs2: temp_reg,
                        imm: actual_offset,
                    });
                }
            }
        }

        // Emit call - always use relocation for cross-function calls
        // The direct call optimization doesn't work correctly because we don't know
        // the absolute address of the current function during lowering.
        // Relocations will be fixed up in the final pass with correct absolute addresses.
        // Emit placeholder jal (will be fixed up later)
        let jal_inst_idx = code.instruction_count();
        code.emit(RiscvInst::Jal {
            rd: Gpr::RA,
            imm: 0, // Placeholder
        });

        // Record relocation for jal (function call target)
        self.relocations.push(Relocation {
            offset: jal_inst_idx,
            target: RelocationTarget::Function(alloc::string::String::from(callee)),
            inst_type: RelocationInstType::Jal { rd: Gpr::RA },
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
                            imm: offset.as_i32(),
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
                            imm: offset.as_i32(),
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
                    imm: offset.as_i32(),
                });
            }
        }

        Ok(())
    }
}
