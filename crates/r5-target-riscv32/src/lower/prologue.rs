//! Function prologue generation.

use r5_ir::Function;
use riscv32_encoder::{Gpr, Inst as RiscvInst};

use crate::{abi::AbiInfo, emit::CodeBuffer, frame::FrameLayout, regalloc::RegisterAllocation};

impl super::Lowerer {
    /// Generate function prologue.
    pub(super) fn gen_prologue(
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
            let mut stack_args_to_spill = alloc::vec::Vec::new();
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
                    // Note: For entry functions, RA is garbage at the start, but we save it anyway
                    // because calls will set RA, and we need to preserve it across nested calls.
                    // The epilogue will handle entry functions specially.
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
}

#[cfg(test)]
mod tests {
    extern crate std;

    use r5_ir::parse_function;

    use super::super::Lowerer;
    use crate::{
        abi::Abi, frame::FrameLayout, liveness::compute_liveness, regalloc::allocate_registers,
        spill_reload::create_spill_reload_plan,
    };

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
        let encoded: alloc::vec::Vec<u32> = instructions.iter().map(|i| i.encode()).collect();

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
