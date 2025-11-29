//! Code emission for RISC-V 32-bit backend3
//!
//! This module handles emission of VCode to machine code, including
//! application of register allocations and edits (spills/reloads).

use alloc::vec::Vec;

use lpc_lpir::RelSourceLoc;
use regalloc2::{Edit, Function as RegallocFunction};

use crate::{
    backend3::{
        types::{BlockIndex, InsnIndex},
        vcode::{MachInst, MachTerminator, VCode},
    },
    isa::riscv32::{
        backend3::{
            abi::{preg_to_gpr, FrameLayout, Riscv32ABI},
            inst::Riscv32MachInst,
        },
        inst::Inst,
        inst_buffer::{BranchType, InstBuffer},
        regs::Gpr,
    },
};

/// Emission state tracking
///
/// Tracks label offsets, pending fixups, and other state during code emission.
struct EmitState {
    /// Current stack pointer offset (for SP-relative addressing)
    /// This tracks the offset from the original SP (before prologue) to the current SP.
    /// Negative values mean SP has been decremented (stack grows down).
    sp_offset: i32,

    /// Label offsets: maps block index to code offset
    /// UNKNOWN_LABEL_OFFSET if label not yet bound
    label_offsets: Vec<u32>,

    /// Pending fixups: branches waiting for labels to be bound
    pending_fixups: Vec<PendingFixup>,

    /// Prologue/epilogue state
    frame_size: u32,
    clobbered_callee_saved: Vec<Gpr>,

    /// Current source location (for debugging)
    cur_srcloc: Option<RelSourceLoc>,
}

/// Special offset value meaning "label not yet bound"
const UNKNOWN_LABEL_OFFSET: u32 = u32::MAX;

/// Pending fixup for a branch instruction
struct PendingFixup {
    /// Offset in buffer where branch instruction is
    branch_offset: usize,
    /// Block index this branch targets
    target_block: BlockIndex,
    /// Branch type (for patching)
    branch_type: BranchType,
}

impl EmitState {
    /// Create a new emission state
    fn new(num_blocks: usize) -> Self {
        EmitState {
            sp_offset: 0,
            label_offsets: alloc::vec![UNKNOWN_LABEL_OFFSET; num_blocks],
            pending_fixups: Vec::new(),
            frame_size: 0,
            clobbered_callee_saved: Vec::new(),
            cur_srcloc: None,
        }
    }

    /// Bind a label to the current code offset
    fn bind_label(&mut self, block: BlockIndex, offset: u32) {
        self.label_offsets[block.index()] = offset;
    }

    /// Get the offset for a label, or UNKNOWN_LABEL_OFFSET if not yet bound
    fn get_label_offset(&self, block: BlockIndex) -> u32 {
        self.label_offsets[block.index()]
    }

    /// Resolve or record a fixup for a branch
    /// If label is already bound, patch immediately. Otherwise, record for later.
    fn resolve_or_record_fixup(
        &mut self,
        buffer: &mut InstBuffer,
        branch_offset: usize,
        target_block: BlockIndex,
        branch_type: BranchType,
    ) {
        let target_offset = self.get_label_offset(target_block);
        if target_offset != UNKNOWN_LABEL_OFFSET {
            // Label already bound - patch immediately
            buffer.patch_branch(branch_offset, target_offset, branch_type);
        } else {
            // Label not yet bound - record fixup
            self.pending_fixups.push(PendingFixup {
                branch_offset,
                target_block,
                branch_type,
            });
        }
    }

    /// Resolve all pending fixups for a newly-bound label
    fn resolve_pending_fixups(
        &mut self,
        buffer: &mut InstBuffer,
        block: BlockIndex,
        label_offset: u32,
    ) {
        // Find all fixups targeting this block and resolve them
        let mut i = 0;
        while i < self.pending_fixups.len() {
            if self.pending_fixups[i].target_block == block {
                let fixup = self.pending_fixups.remove(i);
                buffer.patch_branch(fixup.branch_offset, label_offset, fixup.branch_type);
            } else {
                i += 1;
            }
        }
    }

    /// Resolve all remaining pending fixups (should be none if emission order is correct)
    fn resolve_all_pending_fixups(&mut self, buffer: &mut InstBuffer) {
        for fixup in &self.pending_fixups {
            let target_offset = self.get_label_offset(fixup.target_block);
            if target_offset != UNKNOWN_LABEL_OFFSET {
                buffer.patch_branch(fixup.branch_offset, target_offset, fixup.branch_type);
            } else {
                panic!(
                    "Unresolved label fixup: block {:?} not bound",
                    fixup.target_block
                );
            }
        }
        self.pending_fixups.clear();
    }
}

impl VCode<Riscv32MachInst> {
    /// Emit VCode to machine code with register allocations applied
    ///
    /// This method:
    /// 1. Computes frame layout from regalloc output
    /// 2. Emits prologue
    /// 3. Emits instructions with allocations applied
    /// 4. Emits edits (spills/reloads/moves) at their program points
    /// 5. Emits epilogue at returns
    pub fn emit(&self, regalloc: &regalloc2::Output) -> InstBuffer {
        let mut buffer = InstBuffer::new();

        // Compute frame layout from regalloc results
        let frame_layout = self.compute_frame_layout(regalloc);

        // Initialize emission state
        let mut state = EmitState::new(self.block_ranges.len());
        state.frame_size = frame_layout.total_size();
        state.clobbered_callee_saved = frame_layout.clobbered_regs.clone();

        // Compute emission order (cold blocks at end)
        let block_order = self.compute_emission_order();

        // Emit blocks in final order
        for block_idx in block_order {
            // Is this the entry block? Emit prologue
            if block_idx.index() == self.entry.index() {
                self.gen_prologue(&mut buffer, &mut state, &frame_layout);
            }

            // Bind label for this block
            let block_start_offset = buffer.cur_offset();
            state.bind_label(block_idx, block_start_offset);
            buffer.bind_label(block_idx.index() as u32);

            // Emit block instructions and edits
            self.emit_block(&mut buffer, &mut state, block_idx, regalloc, &frame_layout);

            // Resolve any pending fixups that targeted this label
            state.resolve_pending_fixups(&mut buffer, block_idx, block_start_offset);
        }

        // Resolve any remaining forward references (should be none if order is correct)
        state.resolve_all_pending_fixups(&mut buffer);

        buffer
    }

    /// Compute emission order (cold blocks at end)
    fn compute_emission_order(&self) -> Vec<BlockIndex> {
        // Start with original order
        let mut order: Vec<BlockIndex> = (0..self.block_ranges.len())
            .map(|i| regalloc2::Block::new(i))
            .collect();

        // Move cold blocks to end
        let mut cold = Vec::new();
        let mut hot = Vec::new();
        for block_idx in &order {
            if self.block_metadata[block_idx.index()].cold {
                cold.push(*block_idx);
            } else {
                hot.push(*block_idx);
            }
        }

        // Optimize hot path for fallthrough (simple: keep original order for now)
        hot.extend(cold);
        hot
    }

    /// Compute frame layout from regalloc output
    fn compute_frame_layout(&self, regalloc: &regalloc2::Output) -> FrameLayout {
        use regalloc2::PRegSet;

        // Count spill slots
        let num_spill_slots = regalloc.num_spillslots;
        let spill_slots_size = num_spill_slots * 4; // 4 bytes per slot for RISC-V 32

        // Compute clobbered callee-saved registers
        let mut clobbered_pregs = PRegSet::default();

        // Add registers that are targets of moves (from edits)
        for (_prog_point, edit) in &regalloc.edits {
            if let Edit::Move { to, .. } = edit {
                if let Some(preg) = to.as_reg() {
                    clobbered_pregs.add(preg);
                }
            }
        }

        // Add registers that are defs (written to) in instructions
        for inst_idx in 0..self.insts.len() {
            let inst = regalloc2::Inst::new(inst_idx);
            let allocs = regalloc.inst_allocs(inst);
            let operands = RegallocFunction::inst_operands(self, inst);

            for (operand, alloc) in operands.iter().zip(allocs.iter()) {
                // Only consider defs (writes)
                if operand.kind() == regalloc2::OperandKind::Def {
                    if let Some(preg) = alloc.as_reg() {
                        clobbered_pregs.add(preg);
                    }
                }
            }

            // Add explicitly clobbered registers from instruction clobber lists
            if let Some(&inst_clobbered) = self.clobbers.get(&InsnIndex::new(inst_idx)) {
                clobbered_pregs.union_from(inst_clobbered);
            }
        }

        // Filter to only callee-saved registers
        let machine_env = Riscv32ABI::machine_env();
        let callee_saved_pregs: Vec<regalloc2::PReg> = machine_env.non_preferred_regs_by_class
            [regalloc2::RegClass::Int as usize]
            .iter()
            .copied()
            .collect();
        let mut clobbered_callee_saved = Vec::new();

        // Iterate over all possible PRegs and check if they're in the clobbered set
        for preg in &callee_saved_pregs {
            if clobbered_pregs.contains(*preg) {
                clobbered_callee_saved.push(preg_to_gpr(*preg));
            }
        }

        // Sort for consistent ordering (affects frame layout)
        clobbered_callee_saved.sort_by_key(|gpr: &Gpr| gpr.num());

        FrameLayout {
            setup_area_size: 8, // FP + RA (8 bytes)
            clobber_area_size: (clobbered_callee_saved.len() * 4) as u32,
            spill_slots_size: spill_slots_size as u32,
            abi_size: 0, // No ABI requirements for now (outgoing args, etc.)
            clobbered_regs: clobbered_callee_saved,
        }
    }

    /// Generate prologue
    fn gen_prologue(&self, buffer: &mut InstBuffer, state: &mut EmitState, frame: &FrameLayout) {
        // 1. Setup area: save FP and RA
        // addi sp, sp, -8
        // sw ra, 4(sp)
        // sw fp, 0(sp)
        state.sp_offset = -8;
        buffer.push_addi(Gpr::Sp, Gpr::Sp, -8);
        buffer.push_sw(Gpr::Sp, Gpr::Ra, 4);
        buffer.push_sw(Gpr::Sp, Gpr::S0, 0);

        // 2. Adjust SP for entire frame
        let total_size = frame.total_size();
        if total_size > 8 {
            buffer.push_addi(Gpr::Sp, Gpr::Sp, -((total_size - 8) as i32));
            state.sp_offset = -(total_size as i32);
        }

        // 3. Save clobbered callee-saved registers
        let mut offset = 8; // After setup area
        for reg in &frame.clobbered_regs {
            buffer.push_sw(Gpr::Sp, *reg, offset);
            offset += 4;
        }
    }

    /// Generate epilogue (emitted at each return instruction)
    fn gen_epilogue(&self, buffer: &mut InstBuffer, state: &mut EmitState, frame: &FrameLayout) {
        // 1. Restore clobbered callee-saved registers (reverse order)
        let mut offset = 8 + (frame.clobbered_regs.len() * 4) as i32;
        for reg in frame.clobbered_regs.iter().rev() {
            offset -= 4;
            buffer.push_lw(*reg, Gpr::Sp, offset);
        }

        // 2. Restore SP
        let total_size = frame.total_size();
        if total_size > 8 {
            buffer.push_addi(Gpr::Sp, Gpr::Sp, (total_size - 8) as i32);
        }

        // 3. Restore FP and RA
        buffer.push_lw(Gpr::S0, Gpr::Sp, 0);
        buffer.push_lw(Gpr::Ra, Gpr::Sp, 4);
        buffer.push_addi(Gpr::Sp, Gpr::Sp, 8);

        // 4. Return
        buffer.push_jalr(Gpr::Zero, Gpr::Ra, 0);

        // Reset SP offset
        state.sp_offset = 0;
    }

    /// Emit a block with instructions and edits
    fn emit_block(
        &self,
        buffer: &mut InstBuffer,
        state: &mut EmitState,
        block_idx: BlockIndex,
        regalloc: &regalloc2::Output,
        frame: &FrameLayout,
    ) {
        // Get the actual range from block_ranges to know the instruction indices
        let block_range = self
            .block_ranges
            .get(block_idx.index())
            .expect("block should exist");
        let range_start_idx = block_range.start;
        let range_end_idx = block_range.end;

        // Collect edits for this block, sorted by program point
        let mut block_edits: Vec<(regalloc2::ProgPoint, Edit)> = regalloc
            .edits
            .iter()
            .filter(|(prog_point, _)| {
                let inst = prog_point.inst();
                inst.index() >= range_start_idx && inst.index() < range_end_idx
            })
            .cloned()
            .collect();

        // Sort edits by program point
        block_edits.sort_by_key(|(prog_point, _)| (prog_point.inst().index(), prog_point.pos()));

        // Emit instructions and edits
        let mut edit_idx = 0;
        for inst_idx in range_start_idx..range_end_idx {
            let inst = regalloc2::Inst::new(inst_idx);

            // Update source location if it changed
            let inst_srcloc = self.srclocs[inst_idx];
            if state.cur_srcloc != Some(inst_srcloc) {
                if state.cur_srcloc.is_some() {
                    // End previous source location range
                    state.cur_srcloc = None;
                }
                if !inst_srcloc.is_default() {
                    // Start new source location range
                    state.cur_srcloc = Some(inst_srcloc);
                }
            }

            // Emit edits that come before this instruction
            while edit_idx < block_edits.len() {
                let (prog_point, edit) = &block_edits[edit_idx];
                if prog_point.inst().index() < inst_idx
                    || (prog_point.inst().index() == inst_idx && (prog_point.pos() as u8) == 0)
                // Before = 0
                {
                    self.emit_edit(buffer, edit, frame, state);
                    edit_idx += 1;
                } else {
                    break;
                }
            }

            // Emit the instruction
            let mut mach_inst = self.insts[inst_idx].clone();
            let allocs = regalloc.inst_allocs(inst);

            // If this is a return, emit epilogue instead of return instruction
            if mach_inst.is_term() == MachTerminator::Ret {
                self.gen_epilogue(buffer, state, frame);
                // Skip remaining edits for this instruction
                while edit_idx < block_edits.len() {
                    let (prog_point, _) = &block_edits[edit_idx];
                    if prog_point.inst().index() == inst_idx {
                        edit_idx += 1;
                    } else {
                        break;
                    }
                }
                continue;
            }

            // Apply register allocations to operands
            self.apply_allocations(&mut mach_inst, allocs);

            // Handle branches (resolve labels)
            if let Some(branch_info) = self.get_branch_info(&mach_inst, block_idx) {
                self.emit_branch(buffer, state, mach_inst, branch_info);
            } else {
                // Regular instruction - emit directly
                self.emit_instruction(buffer, &mach_inst);
            }

            // Emit edits that come after this instruction
            while edit_idx < block_edits.len() {
                let (prog_point, edit) = &block_edits[edit_idx];
                if prog_point.inst().index() == inst_idx && (prog_point.pos() as u8) == 1
                // After = 1
                {
                    self.emit_edit(buffer, edit, frame, state);
                    edit_idx += 1;
                } else {
                    break;
                }
            }
        }

        // Emit any remaining edits (shouldn't happen, but be safe)
        while edit_idx < block_edits.len() {
            let (_, edit) = &block_edits[edit_idx];
            self.emit_edit(buffer, edit, frame, state);
            edit_idx += 1;
        }
    }

    /// Get branch information for an instruction
    fn get_branch_info(&self, inst: &Riscv32MachInst, block: BlockIndex) -> Option<BranchInfo> {
        match inst {
            Riscv32MachInst::Br { .. } | Riscv32MachInst::Jump => {
                let succ_range = self.block_succ_range.get(block.index())?;
                let succs = &self.block_succs[succ_range.start..succ_range.end];
                match succs.len() {
                    1 => Some(BranchInfo::OneDest { target: succs[0] }),
                    2 => Some(BranchInfo::TwoDest {
                        target_true: succs[0],
                        target_false: succs[1],
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Emit a branch instruction with label resolution
    fn emit_branch(
        &self,
        buffer: &mut InstBuffer,
        state: &mut EmitState,
        mut branch: Riscv32MachInst,
        branch_info: BranchInfo,
    ) {
        match branch_info {
            BranchInfo::TwoDest {
                target_true,
                target_false,
            } => {
                // Convert two-dest branch to single-dest
                // For now, assume false is fallthrough (next block)
                // In practice, need to check block order
                let current_offset = buffer.cur_offset();
                let true_offset = state.get_label_offset(target_true);
                let false_offset = state.get_label_offset(target_false);

                // Determine if one target is fallthrough
                // Simplified: assume false is fallthrough if it's next block
                let (target_block, invert) = if false_offset == current_offset + 4 {
                    (target_true, false)
                } else {
                    (target_false, true)
                };

                // Invert condition if needed (simplified - actual implementation would
                // need to modify the instruction condition)
                if invert {
                    // For now, just emit the branch as-is
                    // TODO: Implement condition inversion
                }

                // Emit branch with label target
                let branch_offset = buffer.cur_offset();
                let inst = self.convert_branch_to_inst(&branch, target_block);
                buffer.emit_branch_with_label(inst, target_block.index() as u32);

                // Try to resolve immediately, or record fixup
                state.resolve_or_record_fixup(
                    buffer,
                    branch_offset as usize,
                    target_block,
                    BranchType::Conditional,
                );
            }
            BranchInfo::OneDest { target } => {
                let branch_offset = buffer.cur_offset();
                let inst = self.convert_branch_to_inst(&branch, target);
                buffer.emit_branch_with_label(inst, target.index() as u32);

                state.resolve_or_record_fixup(
                    buffer,
                    branch_offset as usize,
                    target,
                    BranchType::Unconditional,
                );
            }
        }
    }

    /// Convert Riscv32MachInst branch to Inst for emission
    fn convert_branch_to_inst(&self, branch: &Riscv32MachInst, _target: BlockIndex) -> Inst {
        match branch {
            Riscv32MachInst::Br { condition } => {
                // For now, emit as BEQ with zero (simplified)
                // TODO: Extract actual condition and convert properly
                Inst::Beq {
                    rs1: Gpr::Zero,
                    rs2: Gpr::Zero,
                    imm: 0, // Will be patched
                }
            }
            Riscv32MachInst::Jump => Inst::Jal {
                rd: Gpr::Zero,
                imm: 0, // Will be patched
            },
            _ => panic!("Not a branch instruction: {:?}", branch),
        }
    }

    /// Apply register allocations to a machine instruction
    fn apply_allocations(&self, inst: &mut Riscv32MachInst, allocs: &[regalloc2::Allocation]) {
        // Get operands for this instruction
        let mut operand_idx = 0;
        let mut collector = AllocationCollector {
            allocs,
            operand_idx: &mut operand_idx,
        };

        // Collect operands to get their order
        let mut temp_inst = inst.clone();
        temp_inst.get_operands(&mut collector);

        // Now apply allocations based on operand order
        operand_idx = 0;
        inst.apply_allocations_internal(allocs, &mut operand_idx);
    }

    /// Emit a machine instruction (converted to Inst)
    fn emit_instruction(&self, buffer: &mut InstBuffer, inst: &Riscv32MachInst) {
        let riscv_inst = self.convert_machinst_to_inst(inst);
        buffer.emit(riscv_inst);
    }

    /// Convert Riscv32MachInst to Inst for emission
    fn convert_machinst_to_inst(&self, inst: &Riscv32MachInst) -> Inst {
        match inst {
            Riscv32MachInst::Add { rd, rs1, rs2 } => Inst::Add {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Addi { rd, rs1, imm } => Inst::Addi {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Sub { rd, rs1, rs2 } => Inst::Sub {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Lui { rd, imm } => Inst::Lui {
                rd: self.reg_to_gpr(rd.to_reg()),
                imm: *imm,
            },
            Riscv32MachInst::Lw { rd, rs1, imm } => Inst::Lw {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Sw { rs1, rs2, imm } => Inst::Sw {
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
                imm: *imm,
            },
            Riscv32MachInst::Move { rd, rs } => Inst::Addi {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs),
                imm: 0,
            },
            Riscv32MachInst::Mul { rd, rs1, rs2 } => Inst::Mul {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Div { rd, rs1, rs2 } => Inst::Div {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Rem { rd, rs1, rs2 } => Inst::Rem {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Slt { rd, rs1, rs2 } => Inst::Slt {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Sltiu { rd, rs1, imm } => Inst::Sltiu {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Sltu { rd, rs1, rs2 } => Inst::Sltu {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Xori { rd, rs1, imm } => Inst::Xori {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::And { rd, rs1, rs2 } => Inst::Add {
                // TODO: Add AND instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Andi { rd, rs1, imm } => Inst::Addi {
                // TODO: Add ANDI instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Or { rd, rs1, rs2 } => Inst::Add {
                // TODO: Add OR instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Ori { rd, rs1, imm } => Inst::Addi {
                // TODO: Add ORI instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Xor { rd, rs1, rs2 } => Inst::Add {
                // TODO: Add XOR instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Sll { rd, rs1, rs2 } => Inst::Add {
                // TODO: Add SLL instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Slli { rd, rs1, imm } => Inst::Addi {
                // TODO: Add SLLI instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Srl { rd, rs1, rs2 } => Inst::Add {
                // TODO: Add SRL instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Srli { rd, rs1, imm } => Inst::Addi {
                // TODO: Add SRLI instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Sra { rd, rs1, rs2 } => Inst::Add {
                // TODO: Add SRA instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Srai { rd, rs1, imm } => Inst::Addi {
                // TODO: Add SRAI instruction to Inst enum
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Return { .. } => {
                // Returns are handled separately (emit epilogue)
                panic!("Return should be handled before convert_machinst_to_inst");
            }
            Riscv32MachInst::Br { .. } | Riscv32MachInst::Jump => {
                // Branches are handled separately
                panic!("Branch should be handled before convert_machinst_to_inst");
            }
            Riscv32MachInst::Jal { .. } => {
                // Function calls - TODO: implement
                panic!("Function calls not yet implemented");
            }
            Riscv32MachInst::Ecall { .. } => {
                // System calls - TODO: implement
                panic!("System calls not yet implemented");
            }
            Riscv32MachInst::Ebreak => Inst::Ebreak,
            Riscv32MachInst::Trap { .. }
            | Riscv32MachInst::Trapz { .. }
            | Riscv32MachInst::Trapnz { .. } => {
                // Traps - TODO: implement
                panic!("Traps not yet implemented");
            }
            Riscv32MachInst::Args { .. } => {
                // Args is a pseudo-instruction, emits no code
                panic!("Args should not be emitted");
            }
        }
    }

    /// Convert Reg to Gpr
    fn reg_to_gpr(&self, reg: crate::backend3::types::Reg) -> Gpr {
        if let Some(preg) = reg.to_real_reg() {
            preg_to_gpr(preg)
        } else {
            panic!("Virtual register not allocated: {:?}", reg);
        }
    }

    /// Emit an edit (spill/reload/move)
    fn emit_edit(
        &self,
        buffer: &mut InstBuffer,
        edit: &Edit,
        frame: &FrameLayout,
        _state: &EmitState,
    ) {
        match edit {
            Edit::Move { from, to } => {
                match (from.as_reg(), to.as_reg()) {
                    (Some(from_reg), Some(to_reg)) => {
                        // Reg-to-reg move: emit ADD with imm=0
                        let from_gpr = preg_to_gpr(from_reg);
                        let to_gpr = preg_to_gpr(to_reg);
                        buffer.push_addi(to_gpr, from_gpr, 0);
                    }
                    (Some(from_reg), None) => {
                        // Spill: store to stack slot
                        let from_gpr = preg_to_gpr(from_reg);
                        let slot = to.as_stack().expect("to should be stack slot");
                        let slot_index = slot.index();
                        let offset = frame.spill_slot_offset(slot_index);
                        buffer.push_sw(Gpr::Sp, from_gpr, offset);
                    }
                    (None, Some(to_reg)) => {
                        // Reload: load from stack slot
                        let to_gpr = preg_to_gpr(to_reg);
                        let slot = from.as_stack().expect("from should be stack slot");
                        let slot_index = slot.index();
                        let offset = frame.spill_slot_offset(slot_index);
                        buffer.push_lw(to_gpr, Gpr::Sp, offset);
                    }
                    _ => {
                        // Invalid combination
                        panic!("Invalid edit: from={:?}, to={:?}", from, to);
                    }
                }
            }
        }
    }
}

/// Branch information for emission
enum BranchInfo {
    TwoDest {
        target_true: BlockIndex,
        target_false: BlockIndex,
    },
    OneDest {
        target: BlockIndex,
    },
}

/// Helper to collect operands for allocation application
struct AllocationCollector<'a> {
    allocs: &'a [regalloc2::Allocation],
    operand_idx: &'a mut usize,
}

impl<'a> crate::backend3::vcode::OperandVisitor for AllocationCollector<'a> {
    fn visit_use(
        &mut self,
        _vreg: crate::backend3::types::VReg,
        _constraint: regalloc2::OperandConstraint,
    ) {
        *self.operand_idx += 1;
    }

    fn visit_def(
        &mut self,
        _vreg: crate::backend3::types::VReg,
        _constraint: regalloc2::OperandConstraint,
    ) {
        *self.operand_idx += 1;
    }

    fn visit_mod(
        &mut self,
        _vreg: crate::backend3::types::VReg,
        _constraint: regalloc2::OperandConstraint,
    ) {
        *self.operand_idx += 1;
    }
}

/// Extension trait for Riscv32MachInst to apply allocations
trait ApplyAllocations {
    fn apply_allocations_internal(
        &mut self,
        allocs: &[regalloc2::Allocation],
        operand_idx: &mut usize,
    );
}

impl ApplyAllocations for Riscv32MachInst {
    fn apply_allocations_internal(
        &mut self,
        allocs: &[regalloc2::Allocation],
        operand_idx: &mut usize,
    ) {
        match self {
            Riscv32MachInst::Add { rd, rs1, rs2 } => {
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *rd = crate::backend3::types::Writable::new(
                            crate::backend3::types::Reg::from_real_reg(preg),
                        );
                    }
                }
                *operand_idx += 1;
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *rs1 = crate::backend3::types::Reg::from_real_reg(preg);
                    }
                }
                *operand_idx += 1;
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *rs2 = crate::backend3::types::Reg::from_real_reg(preg);
                    }
                }
                *operand_idx += 1;
            }
            Riscv32MachInst::Addi { rd, rs1, imm: _ } => {
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *rd = crate::backend3::types::Writable::new(
                            crate::backend3::types::Reg::from_real_reg(preg),
                        );
                    }
                }
                *operand_idx += 1;
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *rs1 = crate::backend3::types::Reg::from_real_reg(preg);
                    }
                }
                *operand_idx += 1;
            }
            // Add more instruction types as needed...
            _ => {
                // For other instructions, apply allocations similarly
                // This is a simplified version - full implementation would handle all cases
            }
        }
    }
}
