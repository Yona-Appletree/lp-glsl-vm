# Backend3 Phase 3: Emission

**Goal**: Generate machine code

**Timeline**: Week 3

**Deliverable**: Can compile simple functions end-to-end

## Tasks

### 1. Emission state tracking (ISA-agnostic)

**File**: `backend3/emit.rs`

**Emission State Structure**:

```rust
struct EmitState {
    /// Current stack pointer offset (for SP-relative addressing)
    /// This tracks the offset from the original SP (before prologue) to the current SP.
    /// Negative values mean SP has been decremented (stack grows down).
    /// Updated as instructions that modify SP are emitted.
    sp_offset: i32,

    /// Label offsets: maps MachLabel (block index) to code offset
    /// UNKNOWN_LABEL_OFFSET if label not yet bound
    label_offsets: Vec<CodeOffset>,

    /// Pending fixups: branches waiting for labels to be bound
    pending_fixups: Vec<PendingFixup>,

    /// External relocations (function calls, etc.)
    external_relocations: Vec<Reloc>,

    /// Prologue/epilogue state
    frame_size: u32,
    clobbered_callee_saved: Vec<RealReg>,

    /// Current source location (for debugging)
    /// Tracks the current source location being emitted
    cur_srcloc: Option<RelSourceLoc>,
}

struct PendingFixup {
    /// Offset in buffer where branch instruction is
    branch_offset: CodeOffset,
    /// Label this branch targets
    target_label: MachLabel,
    /// Branch type (for patching)
    branch_type: BranchType,
}

/// Label representing a block (essentially a block index)
type MachLabel = BlockIndex;

/// Special offset value meaning "label not yet bound"
const UNKNOWN_LABEL_OFFSET: CodeOffset = CodeOffset::MAX;
```

**SP Offset Tracking Details**:

The `sp_offset` field tracks the current stack pointer offset relative to the function's entry SP (before prologue). This is used to compute SP-relative addresses for:
- **Spill slots**: Stack slots allocated by register allocation
- **Frame slots**: ABI-required frame slots (incoming args, outgoing args, return area)
- **Callee-saved register saves**: Where callee-saved registers are stored

**SP Offset Lifecycle**:

1. **Initial State**: `sp_offset = 0` (at function entry, before prologue)
2. **Prologue**:
   - After setup area (save FP + RA): `sp_offset = -8`
   - After full frame allocation: `sp_offset = -(frame_size)`
   - After callee-saved saves: `sp_offset` unchanged (saves use positive offsets)
3. **During Instruction Emission**: `sp_offset` remains constant (SP doesn't change during function body)
4. **Epilogue** (at each return):
   - Restore callee-saved: `sp_offset` unchanged
   - Restore SP: `sp_offset = 0`
   - Restore FP + RA: `sp_offset = 0`

**EmitState Helper Methods**:

```rust
impl EmitState {
    /// Bind a label to the current code offset
    fn bind_label(&mut self, label: MachLabel, offset: CodeOffset) {
        self.label_offsets[label.index()] = offset;
    }

    /// Get the offset for a label, or UNKNOWN_LABEL_OFFSET if not yet bound
    fn get_label_offset(&self, label: MachLabel) -> CodeOffset {
        self.label_offsets[label.index()]
    }

    /// Resolve or record a fixup for a branch
    /// If label is already bound, patch immediately. Otherwise, record for later.
    fn resolve_or_record_fixup(
        &mut self,
        buffer: &mut InstBuffer,
        branch_offset: CodeOffset,
        target_label: MachLabel,
        branch_type: BranchType,
    ) {
        let target_offset = self.get_label_offset(target_label);
        if target_offset != UNKNOWN_LABEL_OFFSET {
            // Label already bound - patch immediately
            buffer.patch_branch(branch_offset, target_offset, branch_type);
        } else {
            // Label not yet bound - record fixup
            self.pending_fixups.push(PendingFixup {
                branch_offset,
                target_label,
                branch_type,
            });
        }
    }

    /// Resolve all pending fixups for a newly-bound label
    fn resolve_pending_fixups(
        &mut self,
        buffer: &mut InstBuffer,
        label: MachLabel,
        label_offset: CodeOffset,
    ) {
        // Find all fixups targeting this label and resolve them
        let mut i = 0;
        while i < self.pending_fixups.len() {
            if self.pending_fixups[i].target_label == label {
                let fixup = self.pending_fixups.remove(i);
                buffer.patch_branch(
                    fixup.branch_offset,
                    label_offset,
                    fixup.branch_type,
                );
            } else {
                i += 1;
            }
        }
    }

    /// Resolve all remaining pending fixups (should be none if emission order is correct)
    fn resolve_all_pending_fixups(&mut self, buffer: &mut InstBuffer) {
        for fixup in &self.pending_fixups {
            let target_offset = self.get_label_offset(fixup.target_label);
            if target_offset != UNKNOWN_LABEL_OFFSET {
                buffer.patch_branch(
                    fixup.branch_offset,
                    target_offset,
                    fixup.branch_type,
                );
            } else {
                panic!("Unresolved label fixup: label {:?} not bound", fixup.target_label);
            }
        }
        self.pending_fixups.clear();
    }

    /// Start a new source location range
    fn start_srcloc(&mut self, srcloc: RelSourceLoc) {
        // For now, just track it. In the future, could emit debug info.
        self.cur_srcloc = Some(srcloc);
        // TODO: Could call buffer.start_srcloc() if InstBuffer supports it
    }

    /// End the current source location range
    fn end_srcloc(&mut self) {
        // For now, just clear it. In the future, could emit debug info.
        self.cur_srcloc = None;
        // TODO: Could call buffer.end_srcloc() if InstBuffer supports it
    }
}
```

### 2. Frame layout computation (ISA-agnostic)

**File**: `backend3/emit.rs`

**Frame Layout Structure**:

```rust
struct FrameLayout {
    setup_area_size: u32,        // FP + RA (8 bytes)
    clobber_area_size: u32,      // Callee-saved registers
    spill_slots_size: u32,       // Spill slots from regalloc
    abi_size: u32,               // ABI requirements (args, return area)
    clobbered_regs: Vec<RealReg>, // Which callee-saved regs are used
}

impl FrameLayout {
    fn total_size(&self) -> u32 {
        self.setup_area_size + 
        self.clobber_area_size + 
        self.spill_slots_size + 
        self.abi_size
    }
}
```

**Frame Layout Computation**:

```rust
impl VCode<MachInst> {
    fn compute_frame_layout(&self, regalloc: &regalloc2::Output) -> FrameLayout {
        // 1. Count spill slots from regalloc
        let spill_slots = regalloc.spill_slots().len();

        // 2. Compute ABI requirements (incoming args, outgoing args, return area)
        let abi_size = self.abi.compute_frame_size();

        // 3. Determine clobbered callee-saved registers
        let clobbered = self.compute_clobbered_callee_saved(regalloc);

        // 4. Compute total frame size
        FrameLayout {
            setup_area_size: 8, // FP + RA
            clobber_area_size: clobbered.len() * 4,
            spill_slots_size: spill_slots * 4,
            abi_size,
            clobbered_regs: clobbered,
        }
    }

    fn compute_clobbered_callee_saved(&self, regalloc: &regalloc2::Output) -> Vec<RealReg> {
        // Algorithm (inspired by Cranelift):
        // 1. Collect all registers that are written to (defs) in regalloc results
        // 2. Add registers that are targets of moves (from edits)
        // 3. Add explicitly clobbered registers from instruction clobber lists
        // 4. Filter to only callee-saved registers

        use regalloc2::PRegSet;
        let mut clobbered = PRegSet::default();

        // 1. Add all registers that are targets of moves (from edits)
        for (_, edit) in &regalloc.edits {
            if let Edit::Move { to, .. } = edit {
                if let Some(preg) = to.as_reg() {
                    clobbered.add(preg);
                }
            }
        }

        // 2. Add all registers that are defs (written to) in instructions
        for (inst_idx, range) in self.operand_ranges.iter() {
            let operands = &self.operands[range.clone()];
            let allocs = &regalloc.allocs[range];

            for (operand, alloc) in operands.iter().zip(allocs.iter()) {
                if operand.kind() == OperandKind::Def {
                    if let Some(preg) = alloc.as_reg() {
                        clobbered.add(preg);
                    }
                }
            }

            // 3. Add explicitly clobbered registers from instruction clobber lists
            if let Some(&inst_clobbered) = self.clobbers.get(&InsnIndex::new(inst_idx)) {
                if self.insts[inst_idx].is_included_in_clobbers() {
                    clobbered.union_from(inst_clobbered);
                }
            }
        }

        // 4. Filter to only callee-saved registers
        let callee_saved = self.abi.machine_env().callee_saved_gprs();
        let mut clobbered_callee_saved = Vec::new();

        for preg in clobbered.iter() {
            if let Some(real_reg) = preg.as_reg() {
                if callee_saved.contains(&real_reg) {
                    clobbered_callee_saved.push(real_reg);
                }
            }
        }

        // Sort for consistent ordering (affects frame layout)
        clobbered_callee_saved.sort();
        clobbered_callee_saved
    }
}
```

### 3. Prologue/epilogue generation (ISA-agnostic, uses ISA-specific ABI)

**File**: `backend3/emit.rs`

**Prologue Generation**:

```rust
impl VCode<MachInst> {
    fn gen_prologue(
        &self,
        buffer: &mut InstBuffer,
        state: &mut EmitState,
        frame: &FrameLayout,
    ) {
        // 1. Setup area: save FP and RA
        // addi sp, sp, -8
        // sw ra, 4(sp)
        // sw fp, 0(sp)
        // mv fp, sp  (if using frame pointer)

        state.sp_offset = -8;
        buffer.emit(MachInst::Addi { rd: sp, rs1: sp, imm: -8 });
        buffer.emit(MachInst::Sw { rs1: sp, rs2: ra, imm: 4 });
        buffer.emit(MachInst::Sw { rs1: sp, rs2: fp, imm: 0 });
        if self.abi.uses_frame_pointer() {
            buffer.emit(MachInst::Addi { rd: fp, rs1: sp, imm: 0 });
        }

        // 2. Adjust SP for entire frame
        let total_size = frame.total_size();
        if total_size > 8 {
            buffer.emit(MachInst::Addi { rd: sp, rs1: sp, imm: -(total_size as i32 - 8) });
            state.sp_offset = -(total_size as i32);
        }

        // 3. Save clobbered callee-saved registers
        let mut offset = 8; // After setup area
        for reg in &frame.clobbered_regs {
            buffer.emit(MachInst::Sw { rs1: sp, rs2: *reg, imm: offset });
            offset += 4;
        }
    }
}
```

**Epilogue Generation** (emitted at each return instruction):

```rust
impl VCode<MachInst> {
    fn gen_epilogue(
        &self,
        buffer: &mut InstBuffer,
        state: &mut EmitState,
        frame: &FrameLayout,
    ) {
        // 1. Restore clobbered callee-saved registers (reverse order)
        let mut offset = 8 + (frame.clobbered_regs.len() * 4) as i32;
        for reg in frame.clobbered_regs.iter().rev() {
            offset -= 4;
            buffer.emit(MachInst::Lw { rd: *reg, rs1: sp, imm: offset });
        }

        // 2. Restore SP
        let total_size = frame.total_size();
        if total_size > 8 {
            buffer.emit(MachInst::Addi { rd: sp, rs1: sp, imm: total_size as i32 - 8 });
        }

        // 3. Restore FP and RA
        // lw fp, 0(sp)
        // lw ra, 4(sp)
        // addi sp, sp, 8
        buffer.emit(MachInst::Lw { rd: fp, rs1: sp, imm: 0 });
        buffer.emit(MachInst::Lw { rd: ra, rs1: sp, imm: 4 });
        buffer.emit(MachInst::Addi { rd: sp, rs1: sp, imm: 8 });

        // 4. Return
        buffer.emit(MachInst::Jalr { rd: zero, rs1: ra, imm: 0 });
        
        // Reset SP offset
        state.sp_offset = 0;
    }
}
```

**Note**: Epilogues are generated at each `return` instruction, not at the end of the function. This matches Cranelift's approach.

### 4. Emission implementation (ISA-agnostic)

**File**: `backend3/emit.rs`

**Components**:

- Apply allocations and emit code
- Handle edits (moves, spills, reloads)
- Iterate blocks and instructions
- Record relocations
- Streaming label-based emission
- Track source locations for debugging

**Source Location Tracking During Emission**:

Track the current source location and update when it changes:

```rust
impl VCode<MachInst> {
    fn emit(&self, regalloc: &regalloc2::Output) -> InstBuffer {
        let mut state = EmitState::new();
        let mut buffer = InstBuffer::new();
        let mut cur_srcloc: Option<RelSourceLoc> = None;
        
        // ... emission loop ...
        
        for inst_or_edit in regalloc.block_insts_and_edits(&self, block_idx) {
            match inst_or_edit {
                InstOrEdit::Inst(inst_idx) => {
                    // Update source location if it changed
                    let inst_srcloc = self.srclocs[inst_idx.index()];
                    if cur_srcloc != Some(inst_srcloc) {
                        if cur_srcloc.is_some() {
                            // End previous source location range
                            state.end_srcloc();
                        }
                        if !inst_srcloc.is_default() {
                            // Start new source location range
                            state.start_srcloc(inst_srcloc);
                            cur_srcloc = Some(inst_srcloc);
                        } else {
                            cur_srcloc = None;
                        }
                    }
                    
                    // Emit instruction...
                }
                InstOrEdit::Edit(_) => {
                    // Edits don't have source locations (they're synthetic)
                }
            }
        }
        
        // ...
    }
}
```

**Debugging Support**:

Source locations can be used for:
- Error messages: "Error at instruction X (source location Y)"
- Debug output: Print source locations when dumping VCode
- Future: DWARF debug info generation

**Complete Emission Implementation**:

```rust
/// Emit VCode to machine code
impl VCode<MachInst> {
    pub fn emit(
        self,
        regalloc: &regalloc2::Output,
    ) -> InstBuffer {
        let mut buffer = InstBuffer::new();

        // 1. Reserve labels for all blocks
        buffer.reserve_labels_for_blocks(self.num_blocks());

        // 2. Register constants with buffer (if constant pool supported)
        // This allows the buffer to handle constant references during emission
        buffer.register_constants(&self.constants);

        // 3. Compute emission order (cold blocks at end) BEFORE emission
        let block_order = self.compute_emission_order();

        // 4. Compute frame layout from regalloc results
        let frame_layout = self.compute_frame_layout(regalloc);

        // 5. Initialize emission state with ABI information
        let mut state = EmitState::new(&self.abi);
        state.label_offsets.resize(self.num_blocks(), UNKNOWN_LABEL_OFFSET);
        state.frame_size = frame_layout.total_size;
        state.clobbered_callee_saved = frame_layout.clobbered_regs.clone();

        // 6. Emit blocks in final order
        for block_idx in block_order {
            // Call block start hook (if needed for emission state)
            state.on_new_block();

            // Emit block alignment (if needed)
            let aligned_offset = I::align_basic_block(buffer.cur_offset());
            while buffer.cur_offset() < aligned_offset {
                // Emit NOPs to align (if alignment > 4 bytes)
                let nop = I::gen_nop((aligned_offset - buffer.cur_offset()) as usize);
                nop.emit(&mut buffer, &self.emit_info, &mut state);
            }

            // Is this the entry block? Emit prologue
            if block_idx == self.entry {
                buffer.start_srcloc(Default::default());
                self.gen_prologue(&mut buffer, &mut state, &frame_layout);
                buffer.end_srcloc();
            }

            // Bind label for this block
            let label = MachLabel::from_block(block_idx);
            let block_start_offset = buffer.cur_offset();
            state.bind_label(label, block_start_offset);
            buffer.bind_label(label);

            // Resolve any pending fixups that targeted this label
            state.resolve_pending_fixups(&mut buffer, label, block_start_offset);

            // Emit block start instruction (if needed)
            if let Some(block_start) = I::gen_block_start(
                self.block_metadata[block_idx].is_indirect_target,
                false, // forward edge CFI not needed for RISC-V 32
            ) {
                block_start.emit(&mut buffer, &self.emit_info, &mut state);
            }

            // Emit instructions and edits
            for inst_or_edit in regalloc.block_insts_and_edits(&self, block_idx) {
                match inst_or_edit {
                    InstOrEdit::Inst(inst_idx) => {
                        let inst = &self.insts[inst_idx];

                        // Update source location if it changed
                        let inst_srcloc = self.srclocs[inst_idx.index()];
                        if state.cur_srcloc != Some(inst_srcloc) {
                            if state.cur_srcloc.is_some() {
                                buffer.end_srcloc();
                            }
                            if !inst_srcloc.is_default() {
                                buffer.start_srcloc(inst_srcloc);
                                state.cur_srcloc = Some(inst_srcloc);
                            } else {
                                state.cur_srcloc = None;
                            }
                        }

                        // If this is a return, emit epilogue instead of return instruction
                        if inst.is_term() == MachTerminator::Ret {
                            self.gen_epilogue(&mut buffer, &mut state, &frame_layout);
                            continue;
                        }

                        // Apply register allocations to operands
                        let mut inst = inst.clone();
                        inst.apply_allocations(&regalloc.allocs[inst_idx]);

                        // Handle branches (resolve labels)
                        if let Some(branch_info) = inst.get_branch_info() {
                            self.emit_branch(&mut buffer, &mut state, inst, branch_info);
                        } else {
                            // Regular instruction - emit directly
                            inst.emit(&mut buffer, &self.emit_info, &mut state);
                        }

                        // Handle external relocations (function calls, etc.)
                        if let Some(reloc) = self.find_reloc(inst_idx) {
                            state.external_relocations.push(Reloc {
                                offset: buffer.cur_offset(),
                                kind: reloc.kind,
                                target: reloc.target.clone(),
                            });
                        }
                    }
                    InstOrEdit::Edit(edit) => {
                        self.emit_edit(&mut buffer, edit, &mut state);
                    }
                }
            }
        }

        // 7. Resolve any remaining forward references (should be none if order is correct)
        state.resolve_all_pending_fixups(&mut buffer);

        // 8. Fix external relocations (function calls, etc.)
        self.fix_external_relocations(&mut buffer, &state);

        buffer
    }

    fn emit_branch(
        &self,
        buffer: &mut InstBuffer,
        state: &mut EmitState,
        mut branch: MachInst,
        branch_info: BranchInfo,
    ) {
        match branch_info {
            BranchInfo::TwoDest { target_true, target_false } => {
                // Convert two-dest branch to single-dest
                let true_label = MachLabel::from_block(target_true);
                let false_label = MachLabel::from_block(target_false);
                let current_offset = buffer.cur_offset();

                // Check which target is fallthrough (next block)
                let true_offset = state.get_label_offset(true_label);
                let false_offset = state.get_label_offset(false_label);

                // Determine if one target is fallthrough
                // (Simplified: assume false is fallthrough if it's next block)
                // In practice, need to check block order
                let (target_label, invert) = if false_offset == current_offset + 4 {
                    (true_label, false)
                } else {
                    (false_label, true)
                };

                // Invert condition if needed
                if invert {
                    branch.invert_condition();
                }

                // Emit branch with label target
                let branch_offset = buffer.cur_offset();
                buffer.emit_branch_with_label(branch, target_label);

                // Try to resolve immediately, or record fixup
                state.resolve_or_record_fixup(
                    buffer,
                    branch_offset,
                    target_label,
                    BranchType::Conditional,
                );
            }
            BranchInfo::OneDest { target } => {
                let target_label = MachLabel::from_block(target);
                let branch_offset = buffer.cur_offset();
                buffer.emit_branch_with_label(branch, target_label);

                state.resolve_or_record_fixup(
                    buffer,
                    branch_offset,
                    target_label,
                    BranchType::Unconditional,
                );
            }
        }
    }

    fn compute_clobbers_and_function_calls(&self, regalloc: &regalloc2::Output) -> (Vec<RealReg>, FunctionCalls) {
        // Algorithm (inspired by Cranelift):
        // 1. Collect all registers that are written to (defs) in regalloc results
        // 2. Add registers that are targets of moves (from edits)
        // 3. Add explicitly clobbered registers from instruction clobber lists
        // 4. Filter to only callee-saved registers
        // 5. Track function call types (affects frame layout)

        use regalloc2::PRegSet;
        let mut clobbered = PRegSet::default();
        let mut function_calls = FunctionCalls::None;

        // 1. Add all registers that are targets of moves (from edits)
        for (_, edit) in &regalloc.edits {
            if let Edit::Move { to, .. } = edit {
                if let Some(preg) = to.as_reg() {
                    clobbered.add(preg);
                }
            }
        }

        // 2. Add all registers that are defs (written to) in instructions
        // Also track function call types
        for (inst_idx, range) in self.operand_ranges.iter() {
            let operands = &self.operands[range.clone()];
            let allocs = &regalloc.allocs[range];

            // Track function calls (affects frame layout, e.g., outgoing args area)
            function_calls.update(self.insts[inst_idx].call_type());

            for (operand, alloc) in operands.iter().zip(allocs.iter()) {
                // Only consider defs (writes)
                if operand.kind() == OperandKind::Def {
                    if let Some(preg) = alloc.as_reg() {
                        clobbered.add(preg);
                    }
                }
            }

            // 3. Add explicitly clobbered registers from instruction clobber lists
            if let Some(&inst_clobbered) = self.clobbers.get(&InsnIndex::new(inst_idx)) {
                if self.insts[inst_idx].is_included_in_clobbers() {
                    clobbered.union_from(inst_clobbered);
                }
            }
        }

        // 4. Filter to only callee-saved registers
        let callee_saved = self.abi.machine_env().callee_saved_gprs();
        let mut clobbered_callee_saved = Vec::new();

        for preg in clobbered.iter() {
            if let Some(real_reg) = preg.as_reg() {
                if callee_saved.contains(&real_reg) {
                    clobbered_callee_saved.push(real_reg);
                }
            }
        }

        // Sort for consistent ordering (affects frame layout)
        clobbered_callee_saved.sort();
        (clobbered_callee_saved, function_calls)
    }

    fn emit_edit(
        &self,
        buffer: &mut InstBuffer,
        edit: Edit,
        state: &mut EmitState,
    ) {
        match edit {
            Edit::Move { from, to } => {
                match (from.as_reg(), to.as_reg()) {
                    (Some(from_reg), Some(to_reg)) => {
                        // Reg-to-reg move
                        buffer.emit(MachInst::Addi {
                            rd: to_reg,
                            rs1: from_reg,
                            imm: 0,
                        });
                    }
                    (Some(from_reg), None) => {
                        // Spill: store to stack slot
                        let slot = to.as_stack().unwrap();
                        let offset = self.compute_spill_offset(slot, state);
                        buffer.emit(MachInst::Sw {
                            rs1: sp,
                            rs2: from_reg,
                            imm: offset,
                        });
                    }
                    (None, Some(to_reg)) => {
                        // Reload: load from stack slot
                        let slot = from.as_stack().unwrap();
                        let offset = self.compute_spill_offset(slot, state);
                        buffer.emit(MachInst::Lw {
                            rd: to_reg,
                            rs1: sp,
                            imm: offset,
                        });
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    fn fix_external_relocations(&self, buffer: &mut InstBuffer, state: &EmitState) {
        // Resolve external relocations (function calls, etc.)
        for reloc in &state.external_relocations {
            match reloc.kind {
                RelocKind::FunctionCall => {
                    // Look up function address
                    let target_addr = self.resolve_function(&reloc.target);
                    // Fix up instruction at reloc.offset
                    buffer.fixup_call(reloc.offset, target_addr);
                }
                // ... other relocation types ...
            }
        }
    }

    fn compute_emission_order(&self) -> Vec<BlockIndex> {
        // Start with original order
        let mut order: Vec<BlockIndex> = (0..self.num_blocks()).collect();

        // Move cold blocks to end
        let mut cold = Vec::new();
        let mut hot = Vec::new();
        for (idx, block) in order.iter().enumerate() {
            if self.block_metadata[*block].is_cold {
                cold.push(*block);
            } else {
                hot.push(*block);
            }
        }

        // Optimize hot path for fallthrough
        // (Simple: keep original order for now, optimize later)

        hot.extend(cold);
        hot
    }
}
```

**Function Call Tracking**:

```rust
/// Function call type tracking (affects frame layout)
enum FunctionCalls {
    None,        // No function calls
    Regular,     // Has regular function calls (affects outgoing args area)
    TailCall,    // Has tail calls (may affect frame layout differently)
}

impl FunctionCalls {
    fn update(&mut self, call_type: CallType) {
        match (self, call_type) {
            (FunctionCalls::None, CallType::Regular) => *self = FunctionCalls::Regular,
            (FunctionCalls::None, CallType::TailCall) => *self = FunctionCalls::TailCall,
            (FunctionCalls::Regular, CallType::TailCall) => *self = FunctionCalls::TailCall,
            _ => {} // Already set to more general case
        }
    }
}
```

### 5. Instruction conversion (RISC-V 32-specific)

**Components**:

- Convert VReg operands to physical registers in MachInst
- Handle stack slots (compute offsets)
- Implement MachInst methods for emission

### 6. InstBuffer enhancements

**File**: `isa/riscv32/inst_buffer.rs`

**Purpose**: Enhance InstBuffer for label-based emission.

**API Design**:

```rust
use crate::isa::riscv32::Inst;

/// Type representing a block label (essentially a block index)
pub type MachLabel = u32; // BlockIndex

/// Type representing a code offset in bytes
pub type CodeOffset = u32;

/// Branch type for patching
pub enum BranchType {
    /// Conditional branch (BEQ, BNE, etc.) - 12-bit signed offset
    Conditional,
    /// Unconditional jump (JAL) - 20-bit signed offset
    Unconditional,
}

impl InstBuffer {
    /// Get current code offset (in bytes)
    pub fn cur_offset(&self) -> CodeOffset {
        (self.instructions.len() * 4) as CodeOffset
    }
    
    /// Emit a branch instruction with a label target (not yet resolved offset)
    /// Emits the branch with placeholder offset (0), returns instruction index for patching
    ///
    /// The branch instruction should have `imm: 0` as placeholder.
    /// The actual offset will be patched later via `patch_branch()`.
    pub fn emit_branch_with_label(
        &mut self, 
        branch: Inst
    ) -> usize {
        // Verify it's a branch instruction
        match &branch {
            Inst::Beq { .. } | Inst::Bne { .. } | Inst::Blt { .. } | Inst::Bge { .. }
            | Inst::Jal { .. } => {}
            _ => panic!("Not a branch instruction: {:?}", branch),
        }
        
        let inst_idx = self.instructions.len();
        self.emit(branch);
        inst_idx
    }
    
    /// Patch a branch instruction at the given instruction index
    /// Computes the offset from branch location to target and patches the instruction
    ///
    /// # Panics
    ///
    /// Panics if the offset is out of range for the branch type.
    pub fn patch_branch(
        &mut self,
        inst_idx: usize,
        target_offset: CodeOffset,
        branch_type: BranchType,
    ) {
        let branch_offset = (inst_idx * 4) as CodeOffset;
        let delta = target_offset as i32 - branch_offset as i32;
        
        // RISC-V offsets are in 2-byte units (instructions are 4 bytes, but offset is /2)
        let offset_in_units = delta / 2;
        
        // Get the instruction
        let inst = &mut self.instructions[inst_idx];
        
        match (inst, branch_type) {
            // Conditional branches: 12-bit signed offset (in 2-byte units)
            (Inst::Beq { imm, .. } | Inst::Bne { imm, .. }
             | Inst::Blt { imm, .. } | Inst::Bge { imm, .. }, BranchType::Conditional) => {
                assert!(
                    offset_in_units >= -2048 && offset_in_units <= 2047,
                    "Branch offset {} out of range for conditional branch (max ±4KB)",
                    offset_in_units * 2
                );
                *imm = offset_in_units as i32;
            }
            // Unconditional jumps: 20-bit signed offset (in 2-byte units)
            (Inst::Jal { imm, .. }, BranchType::Unconditional) => {
                assert!(
                    offset_in_units >= -524288 && offset_in_units <= 524287,
                    "Jump offset {} out of range for unconditional jump (max ±1MB)",
                    offset_in_units * 2
                );
                *imm = offset_in_units as i32;
            }
            _ => panic!("Mismatch between instruction type and branch type"),
        }
    }
    
    /// Patch a function call instruction (JALR) with target address
    /// This is for external relocations, not internal branches
    pub fn fixup_call(&mut self, inst_idx: usize, target_addr: u32) {
        // For now, function calls use JALR with register (indirect)
        // The address should be loaded into a register first
        // This is a placeholder - actual implementation depends on call ABI
        // TODO: Implement based on call ABI requirements
    }
}
```

**Key Points**:
- RISC-V branch offsets are in 2-byte units (even though instructions are 4 bytes)
- Conditional branches: ±4KB range (12-bit signed × 2 bytes)
- Unconditional jumps: ±1MB range (20-bit signed × 2 bytes)
- Patching happens by modifying the `imm` field of the `Inst` enum
- We assume functions < 4KB for now (panic if out of range)

**Simplified Approach**:

Since we're using structured instructions (not raw bytes), we can:
1. Emit branches with placeholder offsets (0)
2. Store instruction indices where branches are (in EmitState)
3. Patch instructions directly when labels are bound (by modifying the `Inst` enum in-place)

This is simpler than Cranelift's byte-patching approach and sufficient for our needs.

### 7. End-to-end test

**Components**:

- Lower → Regalloc → Emit → Execute
- Verify correct machine code generation

## Testing

- Unit tests for emission state tracking
- Unit tests for frame layout computation
- Unit tests for prologue/epilogue generation
- Unit tests for edit emission (moves, spills, reloads)
- Integration test: End-to-end compilation of simple function

## Success Criteria

- ✅ Can emit prologue/epilogue
- ✅ Can emit instructions with allocated registers
- ✅ Can emit edits (moves, spills, reloads)
- ✅ Can compile simple function end-to-end
- ✅ Generated code executes correctly
