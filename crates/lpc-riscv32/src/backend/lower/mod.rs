//! Instruction lowering from IR to RISC-V 32-bit instructions.

pub mod arithmetic;
pub mod branch;
pub mod call;
pub mod comparisons;
pub mod helpers;
pub mod iconst;
pub mod return_;

use alloc::vec::Vec;

use lpc_lpir::{Block, Function, Inst as IrInst, Value};

use alloc::collections::BTreeMap;

use crate::{
    backend::{
        abi::Abi, frame::FrameLayout, liveness::{InstPoint, LivenessInfo}, regalloc::RegisterAllocation,
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

/// Maps (predecessor_block_idx, target_block_idx, param_idx) -> source_value
/// This represents which value from a predecessor block feeds into which parameter
/// of a target block (phi node source).
pub(crate) type PhiSourceMap = BTreeMap<(usize, usize, usize), Value>;

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
    /// Phi source mapping: (pred_block, target_block, param_idx) -> source_value
    phi_sources: PhiSourceMap,
    /// Liveness info (needed for phi source inference and copy decisions)
    liveness: LivenessInfo,
    /// Current block being lowered (for phi copy context)
    current_block_idx: usize,
}

impl Lowerer {
    /// Create a new lowerer with pre-computed allocation and frame layout.
    pub fn new(
        function: Function,
        allocation: RegisterAllocation,
        spill_reload: SpillReloadPlan,
        frame_layout: FrameLayout,
        abi: Abi,
        liveness: LivenessInfo,
        phi_sources: PhiSourceMap,
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
            phi_sources,
            liveness,
            current_block_idx: 0,
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
            self.current_block_idx = block_idx;
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

        // Note: Phi nodes are handled at branch sites (before jumping to successor blocks),
        // not at block entry. This ensures correct handling with multiple predecessors.

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
            IrInst::Jump { target, args } => {
                branch::lower_jump(self, *target, args);
            }
            IrInst::Br {
                condition,
                target_true,
                args_true,
                target_false,
                args_false,
            } => {
                branch::lower_br(self, *condition, *target_true, args_true, *target_false, args_false);
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

    #[cfg(test)]
    /// Get the instruction buffer (for testing)
    pub(crate) fn inst_buffer(&self) -> &InstBuffer {
        &self.inst_buffer
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

    /// Get the current block index (for phi copy context)
    pub(crate) fn current_block_idx(&self) -> usize {
        self.current_block_idx
    }

    #[cfg(test)]
    /// Set the current block index (for testing)
    pub(crate) fn set_current_block_idx(&mut self, idx: usize) {
        self.current_block_idx = idx;
    }

    /// Copy explicit args to target block's parameters.
    /// This is called before branching to the target block.
    pub(crate) fn copy_args_to_params(
        &mut self,
        args: &[lpc_lpir::Value],
        target_block: usize,
    ) {
        // Bounds check - target block must exist
        if target_block >= self.function.blocks.len() {
            return;
        }

        let target = &self.function.blocks[target_block];

        // If target has no parameters, nothing to copy
        if target.params.is_empty() {
            return;
        }

        // Collect all copies needed: (source_reg, target_reg)
        let mut copies = Vec::new();

        for (param_idx, param_value) in target.params.iter().enumerate() {
            // Get the corresponding arg value
            if let Some(&source_value) = args.get(param_idx) {
                // Get source register (may need to reload if spilled)
                let source_reg = if let Some(reg) = self.allocation.value_to_reg.get(&source_value) {
                    *reg
                } else {
                    // Source is spilled - need to reload before copy
                    // For now, panic - we'll handle spills later
                    panic!(
                        "Phi source value {} is spilled - spill handling not yet implemented",
                        source_value.index()
                    );
                };

                // Get target register (parameter register)
                let target_reg = if let Some(reg) = self.allocation.value_to_reg.get(param_value) {
                    *reg
                } else {
                    // Target is spilled - copy directly to slot
                    // For now, panic - we'll handle spills later
                    panic!(
                        "Phi target parameter {} is spilled - spill handling not yet implemented",
                        param_value.index()
                    );
                };

                // Skip if source and target are same register
                if source_reg != target_reg {
                    copies.push((source_reg, target_reg));
                }
            }
        }

        // Emit parallel copy if needed
        if !copies.is_empty() {
            self.emit_parallel_copy(copies);
        }
    }

    /// Copy phi values from the current block to a target block's parameters.
    /// This is called before branching to the target block.
    /// DEPRECATED: Use copy_args_to_params with explicit args instead.
    pub(crate) fn copy_phi_values(&mut self, from_block: usize, to_block: usize) {
        // Bounds check - target block must exist
        if to_block >= self.function.blocks.len() {
            // Target block doesn't exist yet (shouldn't happen, but be defensive)
            return;
        }
        
        let target_block = &self.function.blocks[to_block];
        
        // If target has no parameters, nothing to copy
        if target_block.params.is_empty() {
            return;
        }

        // Collect all copies needed: (source_reg, target_reg)
        let mut copies = Vec::new();

        for (param_idx, param_value) in target_block.params.iter().enumerate() {
            // Get phi source for this edge
            if let Some(source_value) = self.phi_sources.get(&(from_block, to_block, param_idx)) {
                // Get source register (may need to reload if spilled)
                let source_reg = if let Some(reg) = self.allocation.value_to_reg.get(source_value) {
                    *reg
                } else {
                    // Source is spilled - need to reload before copy
                    // For now, panic - we'll handle spills later
                    panic!(
                        "Phi source value {} is spilled - spill handling not yet implemented",
                        source_value.index()
                    );
                };

                // Get target register (parameter register)
                let target_reg = if let Some(reg) = self.allocation.value_to_reg.get(param_value) {
                    *reg
                } else {
                    // Target is spilled - copy directly to slot
                    // For now, panic - we'll handle spills later
                    panic!(
                        "Phi target parameter {} is spilled - spill handling not yet implemented",
                        param_value.index()
                    );
                };

                // Skip if source and target are same register
                if source_reg != target_reg {
                    copies.push((source_reg, target_reg));
                }
            } else {
                // Missing phi source - inference failed
                // Skip copy for this parameter (assume value is already correct)
                // This is a fallback for cases where inference couldn't determine the source
                // TODO: Improve phi source inference or handle this case more gracefully
                continue;
            }
        }

        // Emit parallel copy if needed
        if !copies.is_empty() {
            self.emit_parallel_copy(copies);
        }
    }

    /// Emit a parallel copy, handling cycles correctly.
    /// Copies are performed atomically - all source registers are read before any target is written.
    fn emit_parallel_copy(&mut self, mut copies: Vec<(Gpr, Gpr)>) {
        if copies.is_empty() {
            return;
        }

        // Simple cycle-breaking algorithm:
        // 1. Find copies that don't create cycles (can be emitted immediately)
        // 2. For cycles, break them with a temporary register
        let mut remaining = copies;
        let mut emitted = Vec::new();

        while !remaining.is_empty() {
            // Find a copy that doesn't create a cycle
            let mut found_non_cycle = false;
            for i in 0..remaining.len() {
                let (src, dst) = remaining[i];

                // Check if dst is used as src in any remaining copy (would create cycle)
                let creates_cycle = remaining.iter().any(|(s, _)| *s == dst);

                if !creates_cycle {
                    // Safe to emit - no cycle
                    if src != dst {
                        self.inst_buffer_mut().push_add(dst, src, Gpr::Zero);
                    }
                    emitted.push(remaining.remove(i));
                    found_non_cycle = true;
                    break;
                }
            }

            if !found_non_cycle {
                // All remaining copies form cycles - break first one with temp register
                let (src, dst) = remaining.remove(0);
                
                // Use a caller-saved temporary register (t0-t6)
                // Pick one that's not involved in any copy
                let temp = self.find_temp_register(&remaining, &emitted);
                
                // Break cycle: src -> temp -> dst
                self.inst_buffer_mut().push_add(temp, src, Gpr::Zero);
                self.inst_buffer_mut().push_add(dst, temp, Gpr::Zero);
                emitted.push((src, dst));
            }
        }
    }

    /// Find a temporary register that's not used in any copy.
    fn find_temp_register(&self, remaining: &[(Gpr, Gpr)], emitted: &[(Gpr, Gpr)]) -> Gpr {
        // List of caller-saved temporary registers (t0-t6)
        let temp_regs = [
            Gpr::T0, Gpr::T1, Gpr::T2, Gpr::T3, Gpr::T4, Gpr::T5, Gpr::T6,
        ];

        // Collect all registers used in copies (use Vec since Gpr doesn't implement Ord)
        let mut used_regs = Vec::new();
        for (src, dst) in remaining.iter().chain(emitted.iter()) {
            used_regs.push(*src);
            used_regs.push(*dst);
        }

        // Find first temp register not in use
        for &temp in &temp_regs {
            if !used_regs.contains(&temp) {
                return temp;
            }
        }

        // Fallback: use t0 even if it's in use (shouldn't happen in practice)
        // This means we have too many simultaneous copies
        Gpr::T0
    }
}

/// Find all predecessor blocks for a given block.
pub(crate) fn find_predecessors(func: &Function, target_block: usize) -> Vec<usize> {
    let mut predecessors = Vec::new();

    for (pred_idx, block) in func.blocks.iter().enumerate() {
        for inst in &block.insts {
            match inst {
                IrInst::Jump { target, .. } => {
                    if *target as usize == target_block {
                        predecessors.push(pred_idx);
                    }
                }
                IrInst::Br {
                    target_true,
                    target_false,
                    ..
                } => {
                    if *target_true as usize == target_block || *target_false as usize == target_block {
                        predecessors.push(pred_idx);
                    }
                }
                _ => {}
            }
        }
    }

    predecessors
}

/// Compute phi sources by reading explicit arguments from Jump/Br instructions.
/// 
/// With explicit SSA, values passed to block parameters are explicitly specified
/// in Jump/Br instruction arguments, so we can directly read them instead of inferring.
pub(crate) fn compute_phi_sources(func: &Function, _liveness: &LivenessInfo) -> PhiSourceMap {
    let mut phi_sources = BTreeMap::new();

    // For each block with parameters, find all predecessor blocks
    for (target_idx, target_block) in func.blocks.iter().enumerate() {
        if target_block.params.is_empty() {
            continue;
        }

        let predecessors = find_predecessors(func, target_idx);

        for &pred_idx in &predecessors {
            let pred_block = &func.blocks[pred_idx];
            
            // Find the instruction that branches to target_block
            // It should be the last instruction in the predecessor block
            if let Some(last_inst) = pred_block.insts.last() {
                let args = match last_inst {
                    IrInst::Jump { target, args } if *target as usize == target_idx => {
                        Some(args.clone())
                    }
                    IrInst::Br {
                        target_true,
                        args_true,
                        target_false,
                        args_false,
                        ..
                    } => {
                        if *target_true as usize == target_idx {
                            Some(args_true.clone())
                        } else if *target_false as usize == target_idx {
                            Some(args_false.clone())
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(passed_args) = args {
                    // Map the passed arguments to block parameters by position
                    // The i-th argument passed corresponds to the i-th parameter
                    for (param_idx, param_value) in target_block.params.iter().enumerate() {
                        if param_idx < passed_args.len() {
                            // Direct mapping: arg[param_idx] -> param[param_idx]
                            phi_sources.insert((pred_idx, target_idx, param_idx), passed_args[param_idx]);
                        } else {
                            // Not enough arguments passed - this shouldn't happen in valid IR
                            // but handle gracefully by using the parameter itself
                            phi_sources.insert((pred_idx, target_idx, param_idx), *param_value);
                        }
                    }
                } else {
                    // No matching branch found - this shouldn't happen if find_predecessors is correct
                    // but handle gracefully by using parameters themselves
                    for (param_idx, param_value) in target_block.params.iter().enumerate() {
                        phi_sources.insert((pred_idx, target_idx, param_idx), *param_value);
                    }
                }
            }
        }
    }

    phi_sources
}

// Re-export submodules for use in other modules
pub(crate) use arithmetic::*;
pub(crate) use branch::*;
pub(crate) use call::*;
pub(crate) use comparisons::*;
pub(crate) use helpers::*;
pub(crate) use iconst::*;
pub(crate) use return_::*;
