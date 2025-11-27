//! Instruction lowering from IR to RISC-V 32-bit instructions.

mod arithmetic;
mod branch;
mod call;
mod comparisons;
mod helpers;
mod iconst;
mod return_;

use alloc::vec::Vec;

use lpc_lpir::{Block, Function, Inst as IrInst, Value};

use crate::{
    backend::{
        abi::Abi, frame::FrameLayout, liveness::InstPoint, regalloc::RegisterAllocation,
        spill_reload::SpillReloadPlan,
    },
    inst_buffer::InstBuffer,
    Gpr, Inst,
};

/// Represents a relocation that needs to be fixed up.
///
/// A relocation records where a branch instruction is and what block it targets.
/// After all blocks are lowered, we fix up these relocations with the correct
/// PC-relative offsets.
pub(crate) struct Relocation {
    /// Instruction index in the buffer where the branch is
    pub(crate) inst_idx: usize,
    /// Target block index
    pub(crate) target_block: u32,
    /// Type of branch (determines how to patch)
    pub(crate) branch_type: BranchType,
}

/// Type of branch instruction for patching.
#[derive(Debug, Clone, Copy)]
pub(crate) enum BranchType {
    /// Unconditional jump (JAL) - uses Jal20 format
    Jump,
    /// Conditional branch true target (BEQ/BNE/etc) - uses B12 format
    BranchTrue,
    /// Conditional branch false target (JAL if needed) - uses Jal20 format
    BranchFalse,
}

/// Context for lowering a function.
///
/// This holds all the state needed to lower IR instructions to RISC-V instructions.
pub struct Lowerer {
    /// The function being lowered.
    function: Function,
    /// Pre-computed register allocation.
    allocation: RegisterAllocation,
    /// Spill/reload plan.
    spill_reload: SpillReloadPlan,
    /// Frame layout for this function.
    frame_layout: FrameLayout,
    /// ABI information for this function.
    abi: Abi,
    /// Instruction buffer for accumulating RISC-V instructions.
    inst_buffer: InstBuffer,
    /// Map from block index to instruction index where block starts
    #[cfg_attr(test, allow(dead_code))]
    block_addresses: Vec<usize>,
    /// Relocations that need to be fixed up
    #[cfg_attr(test, allow(dead_code))]
    relocations: Vec<Relocation>,
}

impl Lowerer {
    /// Create a new lowerer with pre-computed allocation and frame layout.
    pub fn new(
        function: Function,
        allocation: RegisterAllocation,
        spill_reload: SpillReloadPlan,
        frame_layout: FrameLayout,
        abi: Abi,
    ) -> Self {
        Self {
            function,
            allocation,
            spill_reload,
            frame_layout,
            abi,
            inst_buffer: InstBuffer::new(),
            block_addresses: Vec::new(),
            relocations: Vec::new(),
        }
    }

    /// Lower the entire function to RISC-V instructions.
    pub fn lower_function(mut self) -> InstBuffer {
        // Initialize block addresses vector
        self.block_addresses.resize(self.function.blocks.len(), 0);

        // Generate prologue
        crate::backend::abi::gen_prologue_frame_setup(&mut self.inst_buffer, &self.frame_layout);
        crate::backend::abi::gen_clobber_save(&mut self.inst_buffer, &self.frame_layout);

        // Phase 1: Lower all blocks (records relocations)
        // Clone blocks to avoid borrow checker issues
        let blocks: Vec<(usize, Block)> =
            self.function.blocks.iter().cloned().enumerate().collect();
        for (block_idx, block) in blocks {
            self.lower_block(block_idx, &block);
        }

        // Phase 2: Fix up relocations
        self.fixup_relocations();

        // Generate epilogue
        crate::backend::abi::gen_clobber_restore(&mut self.inst_buffer, &self.frame_layout);
        crate::backend::abi::gen_epilogue_frame_restore(&mut self.inst_buffer, &self.frame_layout);

        self.inst_buffer
    }

    /// Lower a single basic block.
    fn lower_block(&mut self, block_idx: usize, block: &Block) {
        // Record where this block starts (instruction index)
        let block_start = self.inst_buffer.instruction_count();

        // Ensure block_addresses is large enough
        if block_idx >= self.block_addresses.len() {
            self.block_addresses.resize(block_idx + 1, 0);
        }
        self.block_addresses[block_idx] = block_start;

        // Handle block parameters (phi nodes)
        // For now, simplified handling - assume values are already in correct registers
        // TODO: Handle phi nodes properly with predecessor tracking

        // Reload spilled values at block entry (if any)
        if let Some(reloads) = self.spill_reload.block_boundary.get(&block_idx) {
            let reloads = reloads.clone();
            for reload_op in reloads {
                if let crate::backend::spill_reload::SpillReloadOp::Reload {
                    value: _,
                    reg,
                    slot,
                } = reload_op
                {
                    // Reload value from stack slot to register
                    // Spill slots are in fixed_frame_storage area, starting at outgoing_args_size from SP
                    let slot_offset = (slot * 4) as i32; // Each slot is 4 bytes
                    let base_offset = self.frame_layout.outgoing_args_size as i32;
                    let total_offset = base_offset + slot_offset;
                    self.inst_buffer_mut().push_lw(reg, Gpr::Sp, total_offset);
                }
            }
        }

        // Lower all instructions in the block (branches will record relocations)
        for (inst_idx, inst) in block.insts.iter().enumerate() {
            let point = InstPoint {
                block: block_idx,
                inst: inst_idx + 1, // +1 because 0 is block entry
            };

            // Insert reloads before this instruction (if any)
            if let Some(reloads) = self.spill_reload.before.get(&point) {
                let reloads = reloads.clone();
                for reload_op in reloads {
                    if let crate::backend::spill_reload::SpillReloadOp::Reload {
                        value: _,
                        reg,
                        slot,
                    } = reload_op
                    {
                        let slot_offset = (slot * 4) as i32;
                        let base_offset = self.frame_layout.outgoing_args_size as i32;
                        let total_offset = base_offset + slot_offset;
                        self.inst_buffer_mut().push_lw(reg, Gpr::Sp, total_offset);
                    }
                }
            }

            // Lower the instruction
            self.lower_inst(inst);

            // Insert spills after this instruction (if any)
            if let Some(spills) = self.spill_reload.after.get(&point) {
                let spills = spills.clone();
                for spill_op in spills {
                    if let crate::backend::spill_reload::SpillReloadOp::Spill {
                        value: _,
                        reg,
                        slot,
                    } = spill_op
                    {
                        let slot_offset = (slot * 4) as i32;
                        let base_offset = self.frame_layout.outgoing_args_size as i32;
                        let total_offset = base_offset + slot_offset;
                        self.inst_buffer_mut().push_sw(Gpr::Sp, reg, total_offset);
                    }
                }
            }
        }
    }

    /// Lower a single IR instruction.
    fn lower_inst(&mut self, inst: &IrInst) {
        match inst {
            IrInst::Iadd { result, arg1, arg2 } => {
                arithmetic::lower_iadd(self, *result, *arg1, *arg2);
            }
            IrInst::Isub { result, arg1, arg2 } => {
                arithmetic::lower_isub(self, *result, *arg1, *arg2);
            }
            IrInst::Imul { result, arg1, arg2 } => {
                arithmetic::lower_imul(self, *result, *arg1, *arg2);
            }
            IrInst::Idiv { result, arg1, arg2 } => {
                arithmetic::lower_idiv(self, *result, *arg1, *arg2);
            }
            IrInst::Irem { result, arg1, arg2 } => {
                arithmetic::lower_irem(self, *result, *arg1, *arg2);
            }
            IrInst::IcmpEq { result, arg1, arg2 } => {
                comparisons::lower_icmp_eq(self, *result, *arg1, *arg2);
            }
            IrInst::IcmpNe { result, arg1, arg2 } => {
                comparisons::lower_icmp_ne(self, *result, *arg1, *arg2);
            }
            IrInst::IcmpLt { result, arg1, arg2 } => {
                comparisons::lower_icmp_lt(self, *result, *arg1, *arg2);
            }
            IrInst::IcmpLe { result, arg1, arg2 } => {
                comparisons::lower_icmp_le(self, *result, *arg1, *arg2);
            }
            IrInst::IcmpGt { result, arg1, arg2 } => {
                comparisons::lower_icmp_gt(self, *result, *arg1, *arg2);
            }
            IrInst::IcmpGe { result, arg1, arg2 } => {
                comparisons::lower_icmp_ge(self, *result, *arg1, *arg2);
            }
            IrInst::Iconst { result, value } => {
                iconst::lower_iconst(self, *result, *value);
            }
            IrInst::Jump { target } => {
                branch::lower_jump(self, *target);
            }
            IrInst::Br {
                condition,
                target_true,
                target_false,
            } => {
                branch::lower_br(self, *condition, *target_true, *target_false);
            }
            IrInst::Call {
                callee,
                args,
                results,
            } => {
                call::lower_call(self, callee, args, results);
            }
            IrInst::Return { values } => {
                return_::lower_return(self, values);
            }
            IrInst::Load {
                result,
                address,
                ty: _,
            } => {
                helpers::lower_load(self, *result, *address);
            }
            IrInst::Store {
                address,
                value,
                ty: _,
            } => {
                helpers::lower_store(self, *address, *value);
            }
            IrInst::Syscall { number, args } => {
                helpers::lower_syscall(self, *number, args);
            }
            IrInst::Halt => {
                helpers::lower_halt(self);
            }
            IrInst::Fconst { .. } => {
                panic!("Floating point not supported");
            }
        }
    }

    /// Get mutable access to the instruction buffer.
    pub(crate) fn inst_buffer_mut(&mut self) -> &mut InstBuffer {
        &mut self.inst_buffer
    }

    /// Get the frame layout.
    pub(crate) fn frame_layout(&self) -> &FrameLayout {
        &self.frame_layout
    }

    /// Get the ABI information.
    pub(crate) fn abi(&self) -> &Abi {
        &self.abi
    }

    /// Get a register for a value from pre-computed allocation.
    pub(crate) fn get_reg_for_value(&mut self, value: Value) -> Gpr {
        // Look up from pre-computed allocation
        if let Some(reg) = self.allocation.value_to_reg.get(&value) {
            return *reg;
        }

        // If spilled, we need to reload it - but this should have been handled
        // by spill/reload planning. For now, panic.
        panic!(
            "Value {} not found in register allocation (may be spilled)",
            value.index()
        );
    }

    /// Get the register for a value, panicking if not allocated.
    pub(crate) fn get_reg_for_value_required(&self, value: Value) -> Gpr {
        self.allocation
            .value_to_reg
            .get(&value)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "Value {} has no register allocated (may be spilled)",
                    value.index()
                )
            })
    }

    /// Record a relocation that needs to be fixed up.
    pub(crate) fn record_relocation(&mut self, reloc: Relocation) {
        self.relocations.push(reloc);
    }

    #[cfg(test)]
    /// Get relocations (for testing)
    pub(crate) fn relocations(&self) -> &[Relocation] {
        &self.relocations
    }

    #[cfg(test)]
    /// Get block addresses (for testing)
    pub(crate) fn block_addresses(&self) -> &[usize] {
        &self.block_addresses
    }

    /// Fix up all relocations with correct PC-relative offsets.
    ///
    /// This must be called after all blocks are lowered so that block
    /// addresses are known.
    pub(crate) fn fixup_relocations(&mut self) {
        for reloc in &self.relocations {
            // Get current instruction address (in instructions, not bytes)
            let current_inst_idx = reloc.inst_idx;

            // Get target block start address
            let target_block_start = self
                .block_addresses
                .get(reloc.target_block as usize)
                .copied()
                .unwrap_or_else(|| {
                    panic!(
                        "Relocation references invalid block index {}",
                        reloc.target_block
                    )
                });

            // Calculate PC-relative offset (in instructions)
            // RISC-V offsets are relative to the current instruction
            let offset_insts = (target_block_start as i32) - (current_inst_idx as i32);

            // Get current instruction and update offset
            let insts = self.inst_buffer.instructions();
            let current_inst = &insts[reloc.inst_idx];

            let fixed_inst = match (current_inst, &reloc.branch_type) {
                (Inst::Jal { rd, .. }, BranchType::Jump | BranchType::BranchFalse) => {
                    // JAL: offset is in instructions, encoded as imm[20:1]
                    // Range: ±1MB (±524288 instructions)
                    Inst::Jal {
                        rd: *rd,
                        imm: offset_insts,
                    }
                }
                (Inst::Bne { rs1, rs2, .. }, BranchType::BranchTrue) => {
                    // Branch: offset is in instructions, encoded as imm[12:1]
                    // Range: ±4KB (±2048 instructions)
                    Inst::Bne {
                        rs1: *rs1,
                        rs2: *rs2,
                        imm: offset_insts,
                    }
                }
                (Inst::Beq { rs1, rs2, .. }, BranchType::BranchTrue) => Inst::Beq {
                    rs1: *rs1,
                    rs2: *rs2,
                    imm: offset_insts,
                },
                (Inst::Blt { rs1, rs2, .. }, BranchType::BranchTrue) => Inst::Blt {
                    rs1: *rs1,
                    rs2: *rs2,
                    imm: offset_insts,
                },
                (Inst::Bge { rs1, rs2, .. }, BranchType::BranchTrue) => Inst::Bge {
                    rs1: *rs1,
                    rs2: *rs2,
                    imm: offset_insts,
                },
                _ => panic!(
                    "Invalid relocation type {:?} for instruction: {:?}",
                    reloc.branch_type, current_inst
                ),
            };

            self.inst_buffer.set_instruction(reloc.inst_idx, fixed_inst);
        }
    }
}

// Re-export submodules for use in other modules
pub(crate) use arithmetic::*;
pub(crate) use branch::*;
pub(crate) use call::*;
pub(crate) use comparisons::*;
pub(crate) use helpers::*;
pub(crate) use iconst::*;
pub(crate) use return_::*;
