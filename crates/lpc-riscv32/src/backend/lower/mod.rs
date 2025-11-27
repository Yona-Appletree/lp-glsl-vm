//! Instruction lowering from IR to RISC-V 32-bit instructions.

pub mod arithmetic;
pub mod branch;
pub mod call;
pub mod comparisons;
pub mod helpers;
pub mod iconst;
pub mod return_;

use alloc::{collections::BTreeMap, vec::Vec};

use lpc_lpir::{Block, Function, Inst as IrInst, Value};

use crate::{
    backend::{
        abi::{Abi, ArgLoc},
        frame::FrameLayout,
        liveness::{InstPoint, LivenessInfo},
        regalloc::RegisterAllocation,
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

/// Represents a function call relocation that needs to be fixed up.
///
/// A function call relocation records where a JAL instruction is and what function it calls.
/// After all functions are compiled, we fix up these relocations with the correct
/// PC-relative offsets.
#[derive(Clone)]
pub(crate) struct CallRelocation {
    /// Instruction index in the buffer where the call is
    pub(crate) inst_idx: usize,
    /// Name of the function being called
    pub(crate) callee_name: alloc::string::String,
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
    /// Function call relocations that need to be fixed up
    #[cfg_attr(test, allow(dead_code))]
    call_relocations: Vec<CallRelocation>,
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
            call_relocations: Vec::new(),
            phi_sources,
            liveness,
            current_block_idx: 0,
        }
    }

    /// Compute the maximum outgoing args size needed for any call in this function.
    fn compute_max_outgoing_args_size(&self) -> u32 {
        let mut max_size = 0u32;

        for block in &self.function.blocks {
            for inst in &block.insts {
                if let lpc_lpir::Inst::Call { args, .. } = inst {
                    let num_args = args.len();
                    if num_args > 8 {
                        // Compute stack args size
                        let num_stack_args = num_args - 8;
                        let stack_size = (num_stack_args as u32) * 4;
                        // Align to 16 bytes
                        let aligned_size = (stack_size + 15) & !15;
                        max_size = max_size.max(aligned_size);
                    }
                }
            }
        }

        max_size
    }

    /// Lower the entire function to RISC-V instructions.
    /// Returns both the instruction buffer and call relocations.
    pub fn lower_function(mut self) -> (InstBuffer, Vec<CallRelocation>) {
        // Initialize block addresses vector
        self.block_addresses.resize(self.function.blocks.len(), 0);

        // Generate prologue
        crate::backend::abi::gen_prologue_frame_setup(&mut self.inst_buffer, &self.frame_layout);

        // Load function parameters from argument registers/stack
        // This must happen after frame setup (FP is set) but before clobber save
        // so we can access stack args via FP
        self.load_function_parameters();

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

        // Note: Epilogue is generated at each return site (in lower_return),
        // following Cranelift's approach. We don't generate a single epilogue
        // at the end because return instructions can appear in any block.

        let call_relocs = self.call_relocations.clone();
        (self.inst_buffer, call_relocs)
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
                branch::lower_br(
                    self,
                    *condition,
                    *target_true,
                    args_true,
                    *target_false,
                    args_false,
                );
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

    /// Load function parameters from argument registers and stack.
    ///
    /// This loads parameters from their ABI locations (a0-a7 or stack)
    /// into their allocated registers or spill slots.
    fn load_function_parameters(&mut self) {
        // Extract entry block info first to avoid borrow conflicts
        let entry_block_params = self.function.blocks[0].params.clone();
        let num_params = entry_block_params.len();

        if num_params == 0 {
            return;
        }

        // Compute argument locations
        let has_return_area = self.abi.uses_return_area;
        let arg_locs = Abi::compute_arg_locs(num_params, has_return_area);

        // If multi-return, save return area pointer (passed in a0) to s1
        // We'll need it later for storing excess returns
        if has_return_area {
            // Save a0 (return area pointer) to s1 (callee-saved)
            // Note: s1 will be saved in clobber_save if it's used
            self.inst_buffer_mut().push_add(Gpr::S1, Gpr::A0, Gpr::Zero);
        }

        // Load each parameter
        // Collect parameter info first to avoid borrow conflicts
        let param_info: Vec<(usize, Value, ArgLoc)> = entry_block_params
            .iter()
            .enumerate()
            .map(|(idx, val)| (idx, *val, arg_locs[idx]))
            .collect();

        for (param_idx, param_value, arg_loc) in param_info {
            // Skip parameters that aren't allocated (they're unused)
            // Get where this parameter should be stored (from register allocation)
            if let Some(&target_reg) = self.allocation.value_to_reg.get(&param_value) {
                // Parameter goes to a register
                match arg_loc {
                    ArgLoc::Register(src_reg) => {
                        // Copy from argument register to allocated register
                        if src_reg != target_reg {
                            self.inst_buffer_mut()
                                .push_add(target_reg, src_reg, Gpr::Zero);
                        }
                    }
                    ArgLoc::Stack { offset } => {
                        // Load from stack (caller's outgoing args area)
                        // Stack args are stored by caller at SP+offset (relative to caller's SP after clobber save)
                        // We load parameters AFTER prologue_frame_setup but BEFORE clobber_save
                        // At this point: SP_callee = SP_caller - setup_area_size
                        // So stack args are at: SP_callee + setup_area_size + offset
                        let actual_offset = self.frame_layout.setup_area_size as i32 + offset;
                        self.inst_buffer_mut()
                            .push_lw(target_reg, Gpr::Sp, actual_offset);
                    }
                }
            } else if let Some(&slot) = self.allocation.value_to_slot.get(&param_value) {
                // Parameter is spilled - load from stack arg to spill slot
                // Use a temp register to load from stack, then store to spill slot
                // Get frame layout values before mutable borrow
                let setup_area_size = self.frame_layout.setup_area_size;
                let outgoing_args_size = self.frame_layout.outgoing_args_size;
                let clobber_size = self.frame_layout.clobber_size;
                let fixed_frame_storage_size = self.frame_layout.fixed_frame_storage_size;

                let temp_reg = Gpr::T0;
                match arg_loc {
                    ArgLoc::Register(src_reg) => {
                        // Copy from argument register to temp
                        self.inst_buffer_mut()
                            .push_add(temp_reg, src_reg, Gpr::Zero);
                    }
                    ArgLoc::Stack { offset } => {
                        // Load from stack to temp
                        // Same calculation as above - only setup_area_size has been applied
                        let actual_offset = setup_area_size as i32 + offset;
                        self.inst_buffer_mut()
                            .push_lw(temp_reg, Gpr::Sp, actual_offset);
                    }
                }
                // Store temp to spill slot
                // Spill slots are in fixed_frame_storage area, which is after outgoing_args_size
                // After clobber save, SP points to bottom of outgoing args
                // So spill slots are at SP + outgoing_args_size + slot_offset
                // But we're before clobber save, so SP is higher by (clobber_size + fixed_frame_storage_size + outgoing_args_size)
                // So we need to add that adjustment
                let slot_offset = (slot * 4) as i32;
                let base_offset = outgoing_args_size as i32;
                let sp_adjustment = clobber_size + fixed_frame_storage_size;
                let adjusted_offset = base_offset + slot_offset + sp_adjustment as i32;
                self.inst_buffer_mut()
                    .push_sw(Gpr::Sp, temp_reg, adjusted_offset);
            }
            // If parameter is not allocated, it's unused - skip loading it
        }
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

    /// Get the register for a value, loading from stack if it's an unallocated parameter.
    pub(crate) fn get_reg_for_value_required(&mut self, value: Value) -> Gpr {
        // First check if it's allocated to a register
        if let Some(reg) = self.allocation.value_to_reg.get(&value) {
            return *reg;
        }

        // Check if it's a parameter that wasn't allocated (shouldn't happen, but handle it)
        let entry_block_params = &self.function.blocks[0].params;
        if let Some(param_idx) = entry_block_params.iter().position(|&p| p == value) {
            // This is a parameter - load it from stack/register on-demand
            let num_params = entry_block_params.len();
            let has_return_area = self.abi.uses_return_area;
            let arg_locs = Abi::compute_arg_locs(num_params, has_return_area);
            let arg_loc = &arg_locs[param_idx];

            // Use a temporary register to load the parameter
            let temp_reg = Gpr::T0;
            match arg_loc {
                ArgLoc::Register(src_reg) => {
                    // Copy from argument register to temp
                    self.inst_buffer_mut()
                        .push_add(temp_reg, *src_reg, Gpr::Zero);
                }
                ArgLoc::Stack { offset } => {
                    // Load from stack to temp
                    // Stack args are stored by caller at SP+offset (relative to caller's SP after clobber save)
                    // At function entry, FP points to our setup area (SP after prologue_frame_setup)
                    // Caller's SP = FP + setup_area_size
                    // But wait, stack args are at caller's SP + offset, where caller's SP is before our prologue
                    // Actually, when we enter, caller's SP is still valid (it hasn't changed)
                    // Our prologue adjusts SP by -setup_area_size, so caller's SP = our SP + setup_area_size
                    // After clobber_save, our SP is further adjusted, so caller's SP = our SP + setup_area_size + (clobber_size + fixed_frame_storage_size + outgoing_args_size)
                    // But actually, we should use FP to access stack args
                    // FP points to setup area, which is at SP_caller - setup_area_size
                    // So SP_caller = FP + setup_area_size
                    // Stack args are at SP_caller + offset = FP + setup_area_size + offset
                    // But wait, FP is set to SP after prologue, so FP = SP_caller - setup_area_size
                    // So SP_caller = FP + setup_area_size
                    // Stack args are at SP_caller + offset = FP + setup_area_size + offset
                    // But after clobber_save, FP still points to the same place (setup area)
                    // So we can use FP + setup_area_size + offset to access stack args
                    let actual_offset = self.frame_layout.setup_area_size as i32 + offset;
                    self.inst_buffer_mut()
                        .push_lw(temp_reg, Gpr::S0, actual_offset); // Use FP (s0) instead of SP
                }
            }
            return temp_reg;
        }

        // Not a parameter and not allocated - this is an error
        panic!(
            "Value {} has no register allocated (may be spilled)",
            value.index()
        )
    }

    /// Record a relocation that needs to be fixed up.
    pub(crate) fn record_relocation(&mut self, reloc: Relocation) {
        self.relocations.push(reloc);
    }

    /// Record a function call relocation that needs to be fixed up.
    pub(crate) fn record_call_relocation(
        &mut self,
        inst_idx: usize,
        callee_name: alloc::string::String,
    ) {
        self.call_relocations.push(CallRelocation {
            inst_idx,
            callee_name,
        });
    }

    /// Get function call relocations (for module compilation)
    pub(crate) fn call_relocations(&self) -> &[CallRelocation] {
        &self.call_relocations
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
    pub(crate) fn copy_args_to_params(&mut self, args: &[lpc_lpir::Value], target_block: usize) {
        // Bounds check - target block must exist
        if target_block >= self.function.blocks.len() {
            return;
        }

        // Extract target block params first to avoid borrow conflicts
        let target_params: Vec<lpc_lpir::Value> = self.function.blocks[target_block]
            .params
            .iter()
            .copied()
            .collect();

        // If target has no parameters, nothing to copy
        if target_params.is_empty() {
            return;
        }

        // Collect all copies needed: (source_reg, target_reg)
        // Also track direct stores to spill slots: (source_reg, target_slot)
        let mut copies = Vec::new();
        let mut direct_stores = Vec::new(); // (source_reg, target_slot)
        let mut reloads_needed = Vec::new(); // (source_value, temp_reg) - reloads to emit

        for (param_idx, param_value) in target_params.iter().enumerate() {
            // Get the corresponding arg value
            if let Some(&source_value) = args.get(param_idx) {
                // Handle source (may be spilled)
                let source_reg = if let Some(reg) = self.allocation.value_to_reg.get(&source_value)
                {
                    // Source is in a register
                    *reg
                } else {
                    // Source is spilled - reload to a temporary register
                    let slot = self
                        .allocation
                        .value_to_slot
                        .get(&source_value)
                        .expect("Spilled value must have a slot");

                    // Reload to a temporary register
                    // Use a caller-saved register as temp (t0-t6)
                    let temp_reg = self.find_temp_register(&copies, &[]);

                    // Record reload to emit later (after we're done borrowing)
                    reloads_needed.push((*slot, temp_reg));

                    temp_reg
                };

                // Handle target (may be spilled)
                if let Some(target_reg) = self.allocation.value_to_reg.get(param_value) {
                    // Target is in a register - normal copy
                    if source_reg != *target_reg {
                        copies.push((source_reg, *target_reg));
                    }
                } else {
                    // Target is spilled - store directly to slot
                    let slot = self
                        .allocation
                        .value_to_slot
                        .get(param_value)
                        .expect("Spilled parameter must have a slot");
                    direct_stores.push((source_reg, *slot));
                }
            }
        }

        // Now emit reloads (we're done with immutable borrows)
        let base_offset = self.frame_layout.outgoing_args_size as i32;
        for (slot, temp_reg) in reloads_needed {
            let slot_offset = (slot * 4) as i32;
            let total_offset = base_offset + slot_offset;
            self.inst_buffer_mut()
                .push_lw(temp_reg, Gpr::Sp, total_offset);
        }

        // Emit parallel copy for register-to-register copies
        if !copies.is_empty() {
            self.emit_parallel_copy(copies);
        }

        // Emit direct stores to spill slots
        for (source_reg, target_slot) in direct_stores {
            let slot_offset = (target_slot * 4) as i32;
            let total_offset = base_offset + slot_offset;
            self.inst_buffer_mut()
                .push_sw(Gpr::Sp, source_reg, total_offset);
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
    pub(crate) fn emit_parallel_copy(&mut self, mut copies: Vec<(Gpr, Gpr)>) {
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

                // Skip no-ops immediately
                if src == dst {
                    remaining.remove(i);
                    found_non_cycle = true;
                    break;
                }

                // Check if dst is used as src in any remaining copy (would create cycle)
                let creates_cycle = remaining.iter().any(|(s, _)| *s == dst);

                if !creates_cycle {
                    // Safe to emit - no cycle
                    self.inst_buffer_mut().push_add(dst, src, Gpr::Zero);
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
            Gpr::T0,
            Gpr::T1,
            Gpr::T2,
            Gpr::T3,
            Gpr::T4,
            Gpr::T5,
            Gpr::T6,
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
                    if *target_true as usize == target_block
                        || *target_false as usize == target_block
                    {
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
                            phi_sources
                                .insert((pred_idx, target_idx, param_idx), passed_args[param_idx]);
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
