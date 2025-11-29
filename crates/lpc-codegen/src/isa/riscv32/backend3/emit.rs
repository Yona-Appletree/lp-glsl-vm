//! Code emission for RISC-V 32-bit backend3
//!
//! This module handles emission of VCode to machine code, including
//! application of register allocations and edits (spills/reloads).

use alloc::vec::Vec;
use regalloc2::Edit;

use crate::backend3::vcode::VCode;
use crate::isa::riscv32::backend3::abi::{preg_to_gpr, FrameLayout, Riscv32ABI};
use crate::isa::riscv32::backend3::inst::Riscv32MachInst;
use crate::isa::riscv32::inst_buffer::InstBuffer;
use crate::isa::riscv32::regs::Gpr;

impl VCode<Riscv32MachInst> {
    /// Emit VCode to machine code with register allocations applied
    ///
    /// This method:
    /// 1. Computes frame layout from regalloc output
    /// 2. Emits prologue
    /// 3. Emits instructions with allocations applied
    /// 4. Emits edits (spills/reloads/moves) at their program points
    /// 5. Emits epilogue
    pub fn emit(&self, regalloc: &regalloc2::Output) -> InstBuffer {
        // Compute frame layout
        let frame_layout = self.compute_frame_layout(regalloc);

        let mut buffer = InstBuffer::new();

        // Emit prologue
        self.emit_prologue(&mut buffer, &frame_layout);

        // Emit blocks in order
        for block_idx in 0..self.block_ranges.len() {
            let block = regalloc2::Block::new(block_idx);
            self.emit_block(&mut buffer, block, regalloc, &frame_layout);
        }

        buffer
    }

    /// Compute frame layout from regalloc output
    fn compute_frame_layout(&self, regalloc: &regalloc2::Output) -> FrameLayout {
        use regalloc2::PRegSet;
        use crate::isa::riscv32::backend3::abi::Riscv32ABI;

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
        use regalloc2::Function as RegallocFunction;
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
            if let Some(&inst_clobbered) = self.clobbers.get(&crate::backend3::types::InsnIndex::new(inst_idx)) {
                clobbered_pregs.union_from(inst_clobbered);
            }
        }

        // Filter to only callee-saved registers
        // Callee-saved registers are the non-preferred registers in MachineEnv
        let machine_env = Riscv32ABI::machine_env();
        let callee_saved_pregs: Vec<regalloc2::PReg> = machine_env.non_preferred_regs_by_class[regalloc2::RegClass::Int as usize]
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

    /// Emit prologue
    fn emit_prologue(&self, buffer: &mut InstBuffer, frame: &FrameLayout) {
        // Save FP and RA
        // sw sp, -4(sp)  # Save FP (but we need to adjust SP first)
        // sw sp, -8(sp)  # Save RA
        // Actually, we need to:
        // 1. Save FP and RA to stack (at positive offsets before SP adjustment)
        // 2. Adjust SP

        // For now, simplified prologue:
        // Adjust SP to make room for frame
        let frame_size = frame.total_size();
        if frame_size > 0 {
            buffer.push_addi(Gpr::Sp, Gpr::Sp, -(frame_size as i32));
        }

        // Save FP and RA
        buffer.push_sw(Gpr::Sp, Gpr::Ra, (frame_size - 4) as i32);
        buffer.push_sw(Gpr::Sp, Gpr::S0, (frame_size - 8) as i32);

        // Save callee-saved registers
        let mut offset = (frame_size - 8) as i32;
        for reg in &frame.clobbered_regs {
            offset -= 4;
            buffer.push_sw(Gpr::Sp, *reg, offset);
        }
    }

    /// Emit a block with instructions and edits
    fn emit_block(
        &self,
        buffer: &mut InstBuffer,
        block: regalloc2::Block,
        regalloc: &regalloc2::Output,
        frame: &FrameLayout,
    ) {
        // Get the actual range from block_ranges to know the instruction indices
        let block_range = self.block_ranges.get(block.index()).expect("block should exist");
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

        // Sort edits by program point (they should already be sorted, but ensure)
        block_edits.sort_by_key(|(prog_point, _)| {
            (prog_point.inst().index(), prog_point.pos())
        });

        // Emit instructions and edits
        let mut edit_idx = 0;
        for inst_idx in range_start_idx..range_end_idx {
            let inst = regalloc2::Inst::new(inst_idx);

            // Emit edits that come before this instruction
            // ProgramPointPos::Before is 0, After is 1 (based on u8 cast in tests)
            while edit_idx < block_edits.len() {
                let (prog_point, edit) = &block_edits[edit_idx];
                if prog_point.inst().index() < inst_idx
                    || (prog_point.inst().index() == inst_idx
                        && (prog_point.pos() as u8) == 0) // Before = 0
                {
                    self.emit_edit(buffer, edit, frame);
                    edit_idx += 1;
                } else {
                    break;
                }
            }

            // Emit the instruction
            let mut mach_inst = self.insts[inst_idx].clone();
            let allocs = regalloc.inst_allocs(inst);
            self.apply_allocations(&mut mach_inst, allocs);
            self.emit_instruction(buffer, &mach_inst);

            // Emit edits that come after this instruction
            // ProgramPointPos::After is 1 (based on u8 cast in tests)
            while edit_idx < block_edits.len() {
                let (prog_point, edit) = &block_edits[edit_idx];
                if prog_point.inst().index() == inst_idx
                    && (prog_point.pos() as u8) == 1 // After = 1
                {
                    self.emit_edit(buffer, edit, frame);
                    edit_idx += 1;
                } else {
                    break;
                }
            }
        }

        // Emit any remaining edits (shouldn't happen, but be safe)
        while edit_idx < block_edits.len() {
            let (_, edit) = &block_edits[edit_idx];
            self.emit_edit(buffer, edit, frame);
            edit_idx += 1;
        }
    }

    /// Apply register allocations to a machine instruction
    fn apply_allocations(
        &self,
        inst: &mut Riscv32MachInst,
        allocs: &[regalloc2::Allocation],
    ) {
        // This is a placeholder - actual implementation would need to
        // replace VRegs in the instruction with PRegs based on allocations
        // For now, we'll handle this during emission
        let _ = (inst, allocs);
    }

    /// Emit a machine instruction
    ///
    /// Note: This is a placeholder. Actual instruction emission would need to
    /// convert Reg operands to Gpr based on allocations. For now, we focus
    /// on edit emission (spills/reloads).
    fn emit_instruction(&self, _buffer: &mut InstBuffer, _inst: &Riscv32MachInst) {
        // Placeholder - actual implementation would emit the instruction
        // with register allocations applied
    }

    /// Emit an edit (spill/reload/move)
    fn emit_edit(&self, buffer: &mut InstBuffer, edit: &Edit, frame: &FrameLayout) {
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

