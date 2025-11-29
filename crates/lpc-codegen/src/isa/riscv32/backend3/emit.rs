//! Code emission for RISC-V 32-bit backend3
//!
//! This module handles emission of VCode to machine code, including
//! application of register allocations and edits (spills/reloads).

use alloc::{string::String, vec::Vec};

use lpc_lpir::RelSourceLoc;
use regalloc2::{Edit, Function as RegallocFunction};

use crate::{
    backend3::{
        symbols::{Symbol, SymbolTable},
        types::{BlockIndex, InsnIndex},
        vcode::{MachInst, MachTerminator, RelocKind, VCode},
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

    /// External relocations (function calls, etc.)
    external_relocations: Vec<Reloc>,

    /// Prologue/epilogue state
    frame_size: u32,
    clobbered_callee_saved: Vec<Gpr>,

    /// Current source location (for debugging)
    cur_srcloc: Option<RelSourceLoc>,
}

/// External relocation record
struct Reloc {
    /// Offset in buffer where relocation occurs
    offset: u32,
    /// Relocation kind
    kind: RelocKind,
    /// Target symbol
    target: Symbol,
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
            external_relocations: Vec::new(),
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
    ///
    /// # Arguments
    ///
    /// * `regalloc` - Register allocation results
    /// * `symbol_table` - Symbol table for resolving function calls (optional)
    /// * `function_name` - Name of this function (for registering in symbol table, optional)
    pub fn emit(
        &self,
        regalloc: &regalloc2::Output,
        symbol_table: Option<&mut SymbolTable>,
        function_name: Option<&str>,
    ) -> InstBuffer {
        let mut buffer = InstBuffer::new();

        // Compute frame layout from regalloc results
        let frame_layout = self.compute_frame_layout(regalloc);

        // Initialize emission state
        let mut state = EmitState::new(self.block_ranges.len());
        state.frame_size = frame_layout.total_size();
        state.clobbered_callee_saved = frame_layout.clobbered_regs.clone();

        // Register function start offset in symbol table if provided
        let function_start_offset = buffer.cur_offset();
        if let Some(symtab) = symbol_table {
            if let Some(name) = function_name {
                symtab.add_local(Symbol::local(name), function_start_offset);
            }

            // Compute emission order (cold blocks at end)
            let block_order = self.compute_emission_order();

            // Emit blocks in final order
            for block_idx in block_order {
                // Is this the entry block? Emit prologue
                if block_idx.index() == self.entry.index() {
                    self.gen_prologue(&mut buffer, &mut state, &frame_layout);
                }

                // Check alignment requirement for this block
                if let Some(align) = self.block_metadata[block_idx.index()].alignment {
                    let current_offset = buffer.cur_offset();
                    let padding_needed = (align - (current_offset % align)) % align;
                    // Emit NOP instructions for padding (ADDI x0, x0, 0)
                    // Each instruction is 4 bytes
                    let nop_count = (padding_needed / 4) as usize;
                    for _ in 0..nop_count {
                        buffer.emit(Inst::Addi {
                            rd: Gpr::Zero,
                            rs1: Gpr::Zero,
                            imm: 0,
                        });
                    }
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

            // Fix external relocations (function calls, etc.)
            self.fix_external_relocations(&mut buffer, &state, symtab);
        } else {
            // No symbol table - emit without relocation resolution
            // Compute emission order (cold blocks at end)
            let block_order = self.compute_emission_order();

            // Emit blocks in final order
            for block_idx in block_order {
                // Is this the entry block? Emit prologue
                if block_idx.index() == self.entry.index() {
                    self.gen_prologue(&mut buffer, &mut state, &frame_layout);
                }

                // Check alignment requirement for this block
                if let Some(align) = self.block_metadata[block_idx.index()].alignment {
                    let current_offset = buffer.cur_offset();
                    let padding_needed = (align - (current_offset % align)) % align;
                    // Emit NOP instructions for padding (ADDI x0, x0, 0)
                    // Each instruction is 4 bytes
                    let nop_count = (padding_needed / 4) as usize;
                    for _ in 0..nop_count {
                        buffer.emit(Inst::Addi {
                            rd: Gpr::Zero,
                            rs1: Gpr::Zero,
                            imm: 0,
                        });
                    }
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
            // No relocation resolution - this is OK for single-function compilation without calls
        }

        // End any remaining source location range
        if state.cur_srcloc.is_some() {
            buffer.end_srcloc();
        }

        buffer
    }

    /// Fix external relocations (function calls, etc.)
    ///
    /// This resolves relocations that were recorded during emission.
    /// For function calls, this patches the call instruction with the function address.
    fn fix_external_relocations(
        &self,
        buffer: &mut InstBuffer,
        state: &EmitState,
        symbol_table: &SymbolTable,
    ) {
        for reloc in &state.external_relocations {
            if reloc.kind != RelocKind::FunctionCall {
                // Only handle function calls for now
                continue;
            }

            // Look up symbol address/offset
            let target_addr = match symbol_table.lookup(&reloc.target) {
                Some(addr) => addr,
                None => {
                    // Symbol not found - this is an error for now
                    // In the future, we might want to defer resolution for external symbols
                    continue;
                }
            };

            // Get the current PC (offset of the AUIPC instruction)
            let auipc_offset = reloc.offset;
            let pc = auipc_offset as u64;

            // Check if this is a local symbol (PC-relative) or external (absolute)
            let is_local = !symbol_table.is_external(&reloc.target);

            if is_local {
                // Local symbol: use PC-relative addressing
                // AUIPC loads high 20 bits of (target_addr - pc)
                // ADDI adds low 12 bits
                let diff = target_addr.wrapping_sub(pc);
                let hi20 = ((diff >> 12) & 0xFFFFF) as u32;
                let lo12 = (diff & 0xFFF) as u32;

                // Sign-extend lo12 if needed (for negative offsets)
                let lo12_signed = if lo12 & 0x800 != 0 {
                    (lo12 | 0xFFFFF000) as i32
                } else {
                    lo12 as i32
                };

                // Patch AUIPC instruction (high 20 bits)
                // AUIPC format: rd = (imm << 12) + pc
                // We need to patch the immediate field (bits [31:12])
                let auipc_inst_idx = (auipc_offset / 4) as usize;
                if auipc_inst_idx < buffer.instruction_count() {
                    let current_inst = &buffer.instructions()[auipc_inst_idx];
                    if let Inst::Lui { rd, imm: _ } = current_inst {
                        // Replace LUI with AUIPC
                        buffer.set_instruction(auipc_inst_idx, Inst::Auipc { rd: *rd, imm: hi20 });
                    } else if let Inst::Auipc { rd, imm: _ } = current_inst {
                        // Already AUIPC, just patch immediate
                        buffer.set_instruction(auipc_inst_idx, Inst::Auipc { rd: *rd, imm: hi20 });
                    }
                }

                // Patch ADDI instruction (low 12 bits)
                // ADDI is the next instruction after AUIPC
                let addi_inst_idx = auipc_inst_idx + 1;
                if addi_inst_idx < buffer.instruction_count() {
                    let current_inst = &buffer.instructions()[addi_inst_idx];
                    if let Inst::Addi { rd, rs1, imm: _ } = current_inst {
                        buffer.set_instruction(
                            addi_inst_idx,
                            Inst::Addi {
                                rd: *rd,
                                rs1: *rs1,
                                imm: lo12_signed,
                            },
                        );
                    }
                }
            } else {
                // External symbol: use absolute addressing
                // LUI loads high 20 bits of target_addr
                // ADDI adds low 12 bits
                let hi20 = ((target_addr >> 12) & 0xFFFFF) as u32;
                let lo12 = (target_addr & 0xFFF) as u32;

                // Sign-extend lo12 if needed
                let lo12_signed = if lo12 & 0x800 != 0 {
                    (lo12 | 0xFFFFF000) as i32
                } else {
                    lo12 as i32
                };

                // Patch LUI instruction (high 20 bits)
                let lui_inst_idx = (auipc_offset / 4) as usize;
                if lui_inst_idx < buffer.instruction_count() {
                    let current_inst = &buffer.instructions()[lui_inst_idx];
                    if let Inst::Lui { rd, imm: _ } = current_inst {
                        buffer.set_instruction(lui_inst_idx, Inst::Lui { rd: *rd, imm: hi20 });
                    } else if let Inst::Auipc { rd, imm: _ } = current_inst {
                        // Convert AUIPC to LUI for absolute addressing
                        buffer.set_instruction(lui_inst_idx, Inst::Lui { rd: *rd, imm: hi20 });
                    }
                }

                // Patch ADDI instruction (low 12 bits)
                let addi_inst_idx = lui_inst_idx + 1;
                if addi_inst_idx < buffer.instruction_count() {
                    let current_inst = &buffer.instructions()[addi_inst_idx];
                    if let Inst::Addi { rd, rs1, imm: _ } = current_inst {
                        buffer.set_instruction(
                            addi_inst_idx,
                            Inst::Addi {
                                rd: *rd,
                                rs1: *rs1,
                                imm: lo12_signed,
                            },
                        );
                    }
                }
            }
        }
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

        // Compute maximum outgoing args area size
        // Scan all Jal instructions to find the maximum number of stack arguments needed
        let mut max_stack_args = 0u32;
        for inst in &self.insts {
            if let Riscv32MachInst::Jal { args, .. } = inst {
                let stack_args_needed = if args.len() > 8 {
                    ((args.len() - 8) * 4) as u32 // 4 bytes per stack argument
                } else {
                    0
                };
                max_stack_args = max_stack_args.max(stack_args_needed);
            }
        }

        FrameLayout {
            setup_area_size: 8, // FP + RA (8 bytes)
            clobber_area_size: (clobbered_callee_saved.len() * 4) as u32,
            spill_slots_size: spill_slots_size as u32,
            abi_size: max_stack_args,
            clobbered_regs: clobbered_callee_saved,
        }
    }

    /// Generate prologue
    fn gen_prologue(&self, buffer: &mut InstBuffer, state: &mut EmitState, frame: &FrameLayout) {
        // 1. Setup area: save FP and RA
        // addi sp, sp, -8
        // sw ra, 4(sp)
        // sw fp, 0(sp)  (fp is s0/x8)
        state.sp_offset = -8;
        buffer.push_addi(Gpr::Sp, Gpr::Sp, -8);
        buffer.push_sw(Gpr::Sp, Gpr::Ra, 4);
        buffer.push_sw(Gpr::Sp, Gpr::S0, 0); // Save old frame pointer (s0/x8)

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
        buffer.push_lw(Gpr::S0, Gpr::Sp, 0); // Restore old frame pointer (s0/x8)
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
                    buffer.end_srcloc();
                    state.cur_srcloc = None;
                }
                if !inst_srcloc.is_default() {
                    // Start new source location range
                    buffer.start_srcloc(inst_srcloc);
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

            // Skip Args pseudo-instruction (it emits no code, just tells regalloc about ABI args)
            if matches!(mach_inst, Riscv32MachInst::Args { .. }) {
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
                self.emit_instruction(buffer, &mach_inst, state, InsnIndex::new(inst_idx), frame);
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
                let _true_offset = state.get_label_offset(target_true);
                let false_offset = state.get_label_offset(target_false);

                // Determine if one target is fallthrough
                // Simplified: assume false is fallthrough if it's next block
                let (target_block, invert) = if false_offset == current_offset + 4 {
                    (target_true, false)
                } else {
                    (target_false, true)
                };

                // Emit branch with label target (condition inversion handled in convert_branch_to_inst)
                let inst = self.convert_branch_to_inst(&branch, target_block, invert);
                let inst_idx = buffer.emit_branch_with_label(inst, target_block.index() as u32);

                // Try to resolve immediately, or record fixup
                state.resolve_or_record_fixup(
                    buffer,
                    inst_idx,
                    target_block,
                    BranchType::Conditional,
                );
            }
            BranchInfo::OneDest { target } => {
                let inst = self.convert_branch_to_inst(&branch, target, false);
                let inst_idx = buffer.emit_branch_with_label(inst, target.index() as u32);

                state.resolve_or_record_fixup(
                    buffer,
                    inst_idx,
                    target,
                    BranchType::Unconditional,
                );
            }
        }
    }

    /// Convert Riscv32MachInst branch to Inst for emission
    fn convert_branch_to_inst(
        &self,
        branch: &Riscv32MachInst,
        _target: BlockIndex,
        invert: bool,
    ) -> Inst {
        match branch {
            Riscv32MachInst::Br { condition } => {
                // Condition is a Reg containing comparison result (0 or non-zero)
                // For true branch: BNE condition, zero, target (branch if condition != 0)
                // For false branch: BEQ condition, zero, target (branch if condition == 0)
                // If invert is true, we invert the condition
                let condition_gpr = self.reg_to_gpr(*condition);
                if invert {
                    // Inverted: branch if condition == 0 (false)
                    Inst::Beq {
                        rs1: condition_gpr,
                        rs2: Gpr::Zero,
                        imm: 0, // Will be patched
                    }
                } else {
                    // Normal: branch if condition != 0 (true)
                    Inst::Bne {
                        rs1: condition_gpr,
                        rs2: Gpr::Zero,
                        imm: 0, // Will be patched
                    }
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
    fn emit_instruction(
        &self,
        buffer: &mut InstBuffer,
        inst: &Riscv32MachInst,
        state: &mut EmitState,
        _inst_idx: InsnIndex,
        frame: &FrameLayout,
    ) {
        // Handle instructions that need multiple instructions specially
        match inst {
            Riscv32MachInst::Trapz { condition, .. } => {
                // Conditional trap if zero: skip EBREAK if condition != 0
                // Emit: BEQ condition, zero, skip_label (skip EBREAK if condition != 0)
                let condition_gpr = self.reg_to_gpr(*condition);
                
                // Emit branch with placeholder offset
                let branch_inst_idx = buffer.instructions().len();
                buffer.emit(Inst::Beq {
                    rs1: condition_gpr,
                    rs2: Gpr::Zero,
                    imm: 0, // Placeholder, will be patched
                });
                
                // Emit EBREAK
                buffer.emit(Inst::Ebreak);
                
                // Patch branch: skip EBREAK (4 bytes = 1 instruction)
                // Offset is in 2-byte units, so 4 bytes = 2 units
                let skip_offset = buffer.cur_offset();
                buffer.patch_branch(branch_inst_idx, skip_offset, BranchType::Conditional);
                return;
            }
            Riscv32MachInst::Trapnz { condition, .. } => {
                // Conditional trap if non-zero: skip EBREAK if condition == 0
                // Emit: BNE condition, zero, skip_label (skip EBREAK if condition == 0)
                let condition_gpr = self.reg_to_gpr(*condition);
                
                // Emit branch with placeholder offset
                let branch_inst_idx = buffer.instructions().len();
                buffer.emit(Inst::Bne {
                    rs1: condition_gpr,
                    rs2: Gpr::Zero,
                    imm: 0, // Placeholder, will be patched
                });
                
                // Emit EBREAK
                buffer.emit(Inst::Ebreak);
                
                // Patch branch: skip EBREAK (4 bytes = 1 instruction)
                // Offset is in 2-byte units, so 4 bytes = 2 units
                let skip_offset = buffer.cur_offset();
                buffer.patch_branch(branch_inst_idx, skip_offset, BranchType::Conditional);
                return;
            }
            Riscv32MachInst::Ecall { number, args, result } => {
                // System call ABI:
                // - Syscall number goes in a7 (x17)
                // - Arguments go in a0-a6 (x10-x16)
                // - Return value comes from a0 (x10)
                
                // Move syscall number to a7
                // For now, assume number is a constant immediate
                // TODO: Handle case where number is in a register
                buffer.push_addi(Gpr::A7, Gpr::Zero, *number);
                
                // Move arguments to a0-a6
                let arg_regs = [Gpr::A0, Gpr::A1, Gpr::A2, Gpr::A3, Gpr::A4, Gpr::A5, Gpr::A6];
                for (i, arg) in args.iter().take(7).enumerate() {
                    let arg_gpr = self.reg_to_gpr(*arg);
                    if arg_gpr != arg_regs[i] {
                        // Move argument to ABI register
                        buffer.push_addi(arg_regs[i], arg_gpr, 0);
                    }
                }
                
                // Emit ECALL
                buffer.emit(Inst::Ecall);
                
                // Move return value from a0 to result register if needed
                if let Some(result_reg) = result {
                    let result_gpr = self.reg_to_gpr(result_reg.to_reg());
                    if result_gpr != Gpr::A0 {
                        buffer.push_addi(result_gpr, Gpr::A0, 0);
                    }
                }
                return;
            }
            Riscv32MachInst::Jal { rd, callee, args } => {
                // Function call ABI:
                // - First 8 integer args in a0-a7 (x10-x17)
                // - Additional args on stack (outgoing args area)
                // - Return value in a0 (x10), second return in a1 (x11) if applicable
                
                // Move arguments to ABI registers (a0-a7)
                let arg_regs = [
                    Gpr::A0, Gpr::A1, Gpr::A2, Gpr::A3, Gpr::A4, Gpr::A5, Gpr::A6, Gpr::A7,
                ];
                for (i, arg) in args.iter().take(8).enumerate() {
                    let arg_gpr = self.reg_to_gpr(*arg);
                    if arg_gpr != arg_regs[i] {
                        // Move argument to ABI register
                        buffer.push_addi(arg_regs[i], arg_gpr, 0);
                    }
                }
                
                // Handle additional arguments on stack (outgoing args area)
                // Outgoing args are stored at the top of the frame (highest addresses)
                // After prologue, SP points to the bottom of the frame
                // Outgoing args area starts at: SP + (setup_area + clobber_area + spill_slots)
                // Which is: SP + (frame_size - abi_size)
                for (idx, arg) in args.iter().enumerate().skip(8) {
                    let arg_gpr = self.reg_to_gpr(*arg);
                    // Stack offset: base offset to outgoing args area + per-arg offset
                    let base_offset = (frame.setup_area_size + frame.clobber_area_size + frame.spill_slots_size) as i32;
                    let arg_offset = ((idx - 8) * 4) as i32;
                    let stack_offset = base_offset + arg_offset;
                    buffer.push_sw(Gpr::Sp, arg_gpr, stack_offset);
                }
                
                // Emit function call sequence: AUIPC + ADDI + JALR
                // This allows us to call functions at arbitrary addresses
                // We'll use a temporary register (t0) to hold the function address
                let temp_reg = Gpr::T0; // t0 is a caller-saved temporary
                
                // Record relocation for the AUIPC instruction
                // The relocation will patch both AUIPC and ADDI
                let auipc_offset = buffer.cur_offset();
                state.external_relocations.push(Reloc {
                    offset: auipc_offset,
                    kind: RelocKind::FunctionCall,
                    target: Symbol::local(callee.clone()), // Convert String to Symbol::local
                });
                
                // Emit AUIPC with placeholder immediate (will be patched by relocation)
                // AUIPC: temp_reg = (imm << 12) + pc
                buffer.emit(Inst::Auipc {
                    rd: temp_reg,
                    imm: 0, // Placeholder, will be patched
                });
                
                // Emit ADDI with placeholder immediate (will be patched by relocation)
                // ADDI: temp_reg = temp_reg + imm
                buffer.emit(Inst::Addi {
                    rd: temp_reg,
                    rs1: temp_reg,
                    imm: 0, // Placeholder, will be patched
                });
                
                // Emit JALR: ra = pc + 4; pc = temp_reg + 0
                // JALR: rd = pc + 4; pc = rs1 + imm
                buffer.emit(Inst::Jalr {
                    rd: Gpr::Ra, // Return address goes in RA
                    rs1: temp_reg,
                    imm: 0, // No offset needed
                });
                
                // Move return value from a0 to destination register if needed
                if let Some(rd_reg) = rd.to_reg().to_real_reg() {
                    let rd_gpr = preg_to_gpr(rd_reg);
                    if rd_gpr != Gpr::A0 {
                        buffer.push_addi(rd_gpr, Gpr::A0, 0);
                    }
                }
                return;
            }
            _ => {}
        }
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
            Riscv32MachInst::And { rd, rs1, rs2 } => Inst::And {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Andi { rd, rs1, imm } => Inst::Andi {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Or { rd, rs1, rs2 } => Inst::Or {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Ori { rd, rs1, imm } => Inst::Ori {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Xor { rd, rs1, rs2 } => Inst::Xor {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Sll { rd, rs1, rs2 } => Inst::Sll {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Slli { rd, rs1, imm } => Inst::Slli {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Srl { rd, rs1, rs2 } => Inst::Srl {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Srli { rd, rs1, imm } => Inst::Srli {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                imm: *imm,
            },
            Riscv32MachInst::Sra { rd, rs1, rs2 } => Inst::Sra {
                rd: self.reg_to_gpr(rd.to_reg()),
                rs1: self.reg_to_gpr(*rs1),
                rs2: self.reg_to_gpr(*rs2),
            },
            Riscv32MachInst::Srai { rd, rs1, imm } => Inst::Srai {
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
                // Function calls are handled in emit_instruction
                panic!("Jal should be handled in emit_instruction");
            }
            Riscv32MachInst::Ecall { .. } => {
                // System calls are handled in emit_instruction
                panic!("Ecall should be handled in emit_instruction");
            }
            Riscv32MachInst::Ebreak => Inst::Ebreak,
            Riscv32MachInst::Trap { .. } => {
                // Unconditional trap: emit EBREAK
                // The trap code is encoded in the instruction metadata and can be
                // used by trap handlers or debuggers
                Inst::Ebreak
            }
            Riscv32MachInst::Trapz { .. } | Riscv32MachInst::Trapnz { .. } => {
                // Conditional traps are handled in emit_instruction
                panic!("Trapz/Trapnz should be handled in emit_instruction");
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
            Riscv32MachInst::Sub { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Lui { rd, imm: _ } => {
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *rd = crate::backend3::types::Writable::new(
                            crate::backend3::types::Reg::from_real_reg(preg),
                        );
                    }
                }
                *operand_idx += 1;
            }
            Riscv32MachInst::Lw { rd, rs1, imm: _ } => {
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
            Riscv32MachInst::Sw { rs1, rs2, imm: _ } => {
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
            Riscv32MachInst::Move { rd, rs } => {
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
                        *rs = crate::backend3::types::Reg::from_real_reg(preg);
                    }
                }
                *operand_idx += 1;
            }
            Riscv32MachInst::Mul { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Div { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Rem { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Slt { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Sltiu { rd, rs1, imm: _ } => {
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
            Riscv32MachInst::Sltu { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Xori { rd, rs1, imm: _ } => {
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
            Riscv32MachInst::And { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Andi { rd, rs1, imm: _ } => {
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
            Riscv32MachInst::Or { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Ori { rd, rs1, imm: _ } => {
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
            Riscv32MachInst::Xor { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Sll { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Slli { rd, rs1, imm: _ } => {
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
            Riscv32MachInst::Srl { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Srli { rd, rs1, imm: _ } => {
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
            Riscv32MachInst::Sra { rd, rs1, rs2 } => {
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
            Riscv32MachInst::Srai { rd, rs1, imm: _ } => {
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
            Riscv32MachInst::Br { condition } => {
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *condition = crate::backend3::types::Reg::from_real_reg(preg);
                    }
                }
                *operand_idx += 1;
            }
            Riscv32MachInst::Trapz { condition, code: _ } => {
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *condition = crate::backend3::types::Reg::from_real_reg(preg);
                    }
                }
                *operand_idx += 1;
            }
            Riscv32MachInst::Trapnz { condition, code: _ } => {
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *condition = crate::backend3::types::Reg::from_real_reg(preg);
                    }
                }
                *operand_idx += 1;
            }
            Riscv32MachInst::Return { ret_vals } => {
                for ret_val in ret_vals.iter_mut() {
                    if let Some(alloc) = allocs.get(*operand_idx) {
                        if let Some(preg) = alloc.as_reg() {
                            *ret_val = crate::backend3::types::Reg::from_real_reg(preg);
                        }
                    }
                    *operand_idx += 1;
                }
            }
            Riscv32MachInst::Jal { rd, callee: _, args } => {
                if let Some(alloc) = allocs.get(*operand_idx) {
                    if let Some(preg) = alloc.as_reg() {
                        *rd = crate::backend3::types::Writable::new(
                            crate::backend3::types::Reg::from_real_reg(preg),
                        );
                    }
                }
                *operand_idx += 1;
                for arg in args.iter_mut() {
                    if let Some(alloc) = allocs.get(*operand_idx) {
                        if let Some(preg) = alloc.as_reg() {
                            *arg = crate::backend3::types::Reg::from_real_reg(preg);
                        }
                    }
                    *operand_idx += 1;
                }
            }
            Riscv32MachInst::Ecall { number: _, args, result } => {
                for arg in args.iter_mut() {
                    if let Some(alloc) = allocs.get(*operand_idx) {
                        if let Some(preg) = alloc.as_reg() {
                            *arg = crate::backend3::types::Reg::from_real_reg(preg);
                        }
                    }
                    *operand_idx += 1;
                }
                if let Some(ref mut result_reg) = result {
                    if let Some(alloc) = allocs.get(*operand_idx) {
                        if let Some(preg) = alloc.as_reg() {
                            *result_reg = crate::backend3::types::Writable::new(
                                crate::backend3::types::Reg::from_real_reg(preg),
                            );
                        }
                    }
                    *operand_idx += 1;
                }
            }
            Riscv32MachInst::Jump
            | Riscv32MachInst::Ebreak
            | Riscv32MachInst::Trap { .. }
            | Riscv32MachInst::Args { .. } => {
                // These instructions have no operands that need allocation
            }
        }
    }
}
