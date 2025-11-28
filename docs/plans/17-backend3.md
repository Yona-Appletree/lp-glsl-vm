# Backend3: Cranelift-Inspired Backend Architecture

## Overview

This document outlines the design and implementation plan for `backend3`, a new RISC-V 32-bit backend that follows a Cranelift-inspired architecture. The new backend is a fully independent implementation that separates concerns cleanly: **Lowering → VCode (virtual registers) → Register Allocation → Emission**.

**Architecture Separation**: The backend3 implementation is split into two parts:

- **ISA-agnostic code** (`crates/lpc-codegen/src/backend3/`): Generic backend infrastructure that works with any ISA through traits
- **RISC-V 32-specific code** (`crates/lpc-codegen/src/isa/riscv32/backend3/`): RISC-V 32-specific implementations (MachInst enum, ABI spec)

## Architecture Comparison

### Current Backend (`isa/riscv32/backend_old/`)

**Pipeline**:

```
IR Function
  ↓ [Liveness Analysis]
  ↓ [Register Allocation on IR values]
  ↓ [Spill/Reload Planning]
  ↓ [Frame Layout]
  ↓ [Lowering with pre-allocated registers]
Machine Code
```

**Problems**:

- Register allocation happens before lowering (on IR values)
- No virtual register phase
- Allocation decisions mixed with lowering logic
- Hard to reason about register allocation
- Multi-return incomplete (panics on >2 returns)

### New Backend (`backend3/` + `isa/riscv32/backend3/`)

**Pipeline** (Cranelift-inspired):

```
IR Function
  ↓ [Block Lowering Order Computation]
BlockLoweringOrder (critical edge splitting, block ordering)
  ↓ [Lowering]
VCode<MachInst> (virtual registers)
  ↓ [regalloc2]
regalloc2::Output (allocations + edits)
  ↓ [Emission]
  ├─ [Prologue Generation]
  ├─ [Instruction Emission + Edits]
  ├─ [Block Layout Optimization]
  ├─ [Branch Resolution]
  ├─ [Epilogue Generation]
  └─ [Relocation Fixup]
Machine Code
```

**Benefits**:

- Clean separation: lowering → regalloc → emission
- Virtual registers enable better register allocation
- Edits represent allocation decisions explicitly
- Easier to test and debug each phase
- Proper multi-return support from the start

## Compilation Pipeline

### Phase 0: Block Lowering Order Computation

**Purpose**: Compute block ordering and handle critical edge splitting before lowering.

**Input**: `Function` (LPIR) + `DominatorTree`
**Output**: `BlockLoweringOrder`

**Key Steps**:

1. **Critical Edge Detection**: Identify edges where phi moves need to be inserted

   - An edge is critical if the source has multiple successors AND the target has multiple predecessors
   - These edges need intermediate blocks for phi value moves

2. **Edge Block Creation**: Create intermediate blocks for critical edges

   - Each critical edge gets an intermediate block
   - These blocks will contain moves for phi values during lowering

3. **Block Ordering**: Compute reverse postorder (RPO) traversal

   - Ensures defs come before uses (SSA property)
   - Optimizes for fallthrough branches
   - Handles both original blocks and edge blocks

4. **Cold Block Identification**: Mark blocks that are unlikely to execute

   - Used later for block layout optimization
   - Cold blocks can be moved to end of function

5. **Indirect Branch Target Tracking**: Track blocks that are indirect branch targets
   - Needed for proper block alignment
   - May require special handling in emission

**Algorithm** (simplified):

```rust
pub struct BlockLoweringOrder {
    /// Lowered blocks in RPO order
    lowered_order: Vec<LoweredBlock>,
    /// Successor lists for each lowered block
    lowered_succs: Vec<Vec<BlockIndex>>,
    /// Mapping from IR blocks to lowered block indices
    block_to_index: BTreeMap<Block, BlockIndex>,
    /// Cold blocks (for layout optimization)
    cold_blocks: BTreeSet<BlockIndex>,
    /// Indirect branch targets
    indirect_targets: BTreeSet<BlockIndex>,
}

enum LoweredBlock {
    /// Original IR block
    Orig { block: Block },
    /// Edge block (for critical edges)
    Edge { from: Block, to: Block },
}
```

**Key Features**:

- Handles critical edge splitting automatically
- Preserves SSA ordering (defs before uses)
- Enables later block layout optimizations
- Tracks metadata for emission (cold, indirect targets)

### Phase 1: Lowering (IR → VCode)

**Purpose**: Convert LPIR `Function` to `VCode` with virtual registers.

**Input**: `Function` (LPIR)
**Output**: `VCode<MachInst>`

**Key Steps**:

1. **Use Block Lowering Order**: Iterate blocks in computed order (not IR layout order)
2. **Handle Edge Blocks**: For edge blocks, emit phi moves (copy values between VRegs)
3. **Create virtual registers**: For each IR value (function params, block params, instruction results)
4. **Lower instructions**: Convert IR instructions to machine instructions with VReg operands
5. **Build VCode structure**: Construct blocks, instructions, operands, branch args
6. **Track operand constraints**: Record register class constraints for regalloc
7. **Handle constants**: Materialize constants (inline or pool)
8. **Record relocations**: Track function calls and other relocations

### Phase 2: Register Allocation

**Purpose**: Assign physical registers or spill slots to virtual registers.

**Input**: `VCode<MachInst>`
**Output**: `regalloc2::Output`

**Key Steps**:

1. Implement `regalloc2::Function` trait for VCode
2. Configure ABI machine spec
3. Run regalloc2 algorithm
4. Get allocations and edits

### Phase 3: Emission

**Purpose**: Apply register allocations and generate final machine code.

**Input**: `VCode<MachInst>` + `regalloc2::Output`
**Output**: `InstBuffer` (machine code)

**Architecture**: Streaming emission with label-based branch resolution (inspired by Cranelift's MachBuffer).

**Key Steps**:

1. **Compute Emission Order**: Determine final block order (cold blocks at end) BEFORE emission
2. **Initialize Emission State**: Track SP offsets, labels, relocations
3. **Compute Frame Layout**: Calculate frame size from regalloc spills + ABI requirements
4. **Generate Prologue**: Frame setup, callee-saved saves (at entry block)
5. **Emit Blocks in Order**: For each block in emission order:
   - Bind block label to current offset
   - Emit block alignment (if needed)
   - Emit instructions and edits (apply allocations, insert moves/spills/reloads)
   - Emit branches (using labels, resolve incrementally)
   - Emit epilogue at return instructions (not at end)
6. **Final Branch Resolution**: Resolve any remaining forward references
7. **Fix External Relocations**: Resolve function call addresses and other external relocations

**Label-Based Branch Resolution**:

- Each block gets a `MachLabel` (essentially a block index)
- Labels are bound to code offsets as blocks are emitted
- Branches reference labels (not offsets)
- Branch targets resolved incrementally:
  - If label already bound: compute offset and patch branch immediately
  - If label not yet bound: record fixup, resolve when label is bound
- Two-dest branches converted to single-dest during emission:
  - If one target is fallthrough: emit conditional branch to other target
  - Otherwise: emit inverted conditional + unconditional (or optimize later)

**Emission State Tracking**:

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

3. **During Instruction Emission**:

   - `sp_offset` remains constant (SP doesn't change during function body)
   - Stack-relative loads/stores use `sp_offset` to compute addresses

4. **Epilogue** (at each return):
   - Restore callee-saved: `sp_offset` unchanged
   - Restore SP: `sp_offset = 0`
   - Restore FP + RA: `sp_offset = 0`

**Stack Slot Offset Computation**:

```rust
impl EmitState {
    /// Compute the SP-relative offset for a stack slot
    /// Stack slots are allocated above the frame (negative offsets from SP)
    fn compute_stack_slot_offset(&self, slot: StackSlot, frame_layout: &FrameLayout) -> i32 {
        // Stack slots are allocated in the frame, above the setup area
        // Offset is negative (stack grows down)
        let slot_offset_in_frame = frame_layout.compute_slot_offset(slot);
        self.sp_offset + slot_offset_in_frame
    }

    /// Compute offset for a spill slot (from regalloc)
    fn compute_spill_offset(&self, spill_slot: SpillSlot, frame_layout: &FrameLayout) -> i32 {
        // Spill slots are allocated after callee-saved area
        let spill_offset_in_frame = frame_layout.spill_area_start +
            (spill_slot.index() * 4) as i32;
        self.sp_offset + spill_offset_in_frame
    }
}
```

**Note**: `sp_offset` is maintained throughout emission and doesn't change during the function body (only during prologue/epilogue). This simplifies offset computation for stack-relative addressing.

## Key Components

### 1. VCode Structure

**File**: `crates/lpc-codegen/src/backend3/vcode.rs` (ISA-agnostic)

```rust
/// Virtual-register code: machine instructions with virtual registers
pub struct VCode<I: MachInst> {
    /// Machine instructions (with VReg operands)
    insts: Vec<I>,

    /// Operands: flat array for regalloc2
    /// Each operand has: (vreg, constraint, kind)
    operands: Vec<Operand>,

    /// Operand ranges: per-instruction ranges in operands array
    operand_ranges: Ranges,

    /// Clobbers: explicit clobber sets per instruction (for function calls, etc.)
    clobbers: FxHashMap<InsnIndex, PRegSet>,

    /// Block structure
    block_ranges: Ranges,           // Per-block instruction ranges
    block_succ_range: Ranges,        // Per-block successor ranges
    block_succs: Vec<BlockIndex>,   // Successors (flat array)
    block_pred_range: Ranges,        // Per-block predecessor ranges
    block_preds: Vec<BlockIndex>,    // Predecessors (flat array)
    block_params_range: Ranges,      // Per-block parameter ranges
    block_params: Vec<VReg>,         // Block parameter VRegs (flat array)

    /// Branch arguments (values passed to blocks)
    branch_block_args: Vec<VReg>,
    branch_block_arg_range: Ranges,
    branch_block_arg_succ_range: Ranges,

    /// Entry block
    entry: BlockIndex,

    /// Block lowering order
    block_order: BlockLoweringOrder,

    /// ABI information
    abi: Callee<I::ABIMachineSpec>,

    /// Constants (inline or pool references)
    constants: VCodeConstants,

    /// Block metadata
    block_metadata: Vec<BlockMetadata>,

    /// Relocations (function calls, etc.)
    relocations: Vec<VCodeReloc>,

    // Optional fields (deferred):
    // vreg_types: Vec<Type>,  // VReg types (add if needed for validation)
    // srclocs: Vec<RelSourceLoc>,  // Source locations (deferred)
    // debug_value_labels: Vec<(VReg, InsnIndex, InsnIndex, u32)>,  // Debug info (deferred)
}

/// Block metadata
struct BlockMetadata {
    /// Is this a cold block?
    is_cold: bool,
    /// Is this an indirect branch target?
    is_indirect_target: bool,
    /// Alignment requirement (if any)
    alignment: Option<u32>,
}

/// Relocation in VCode
struct VCodeReloc {
    /// Instruction index where relocation occurs
    inst_idx: InsnIndex,
    /// Relocation type (function call, etc.)
    kind: RelocKind,
    /// Target name or identifier
    target: String,
}
```

**Key Features**:

- Machine instructions with `VReg` operands (not physical registers)
- Flat operand array for efficient regalloc2 access
- Block structure preserved from IR (with edge blocks)
- Branch arguments tracked separately
- Constants stored (inline or pool references)
- Block metadata for layout optimization
- Relocations tracked for later fixup

**VCode Structure Completeness**: See [Phase 1](17-backend3-1-foundation.md) for details on which fields are included vs. deferred.

### 2. Machine Instruction Type

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/inst.rs` (RISC-V 32-specific)

```rust
// File: crates/lpc-codegen/src/isa/riscv32/backend3/inst.rs

/// RISC-V 32-bit machine instruction with virtual registers
#[derive(Debug, Clone)]
pub enum MachInst {
    /// ADD: rd = rs1 + rs2
    Add { rd: Writable<VReg>, rs1: VReg, rs2: VReg },

    /// ADDI: rd = rs1 + imm
    Addi { rd: Writable<VReg>, rs1: VReg, imm: i32 },

    /// LW: rd = mem[rs1 + imm]
    Lw { rd: Writable<VReg>, rs1: VReg, imm: i32 },

    /// SW: mem[rs1 + imm] = rs2
    Sw { rs1: VReg, rs2: VReg, imm: i32 },

    /// JAL: rd = PC + 4; PC = PC + imm
    Jal { rd: Writable<VReg>, imm: i32 },

    /// JALR: rd = PC + 4; PC = rs1 + imm
    Jalr { rd: Writable<VReg>, rs1: VReg, imm: i32 },

    /// Branch instructions (BEQ, BNE, etc.)
    Branch { kind: BranchKind, rs1: VReg, rs2: VReg, target: MachLabel },

    // ... more instructions ...
}

impl backend3::MachInst for MachInst {
    type ABIMachineSpec = Riscv32ABI;

    fn get_operands(&mut self, collector: &mut impl OperandVisitor) {
        match self {
            MachInst::Add { rd, rs1, rs2 } => {
                collector.visit_def(*rd);
                collector.visit_use(*rs1);
                collector.visit_use(*rs2);
            }
            // ... handle all instruction types ...
        }
    }

    // ... implement other MachInst trait methods ...
}
```

**Key Features**:

- Uses `VReg` (virtual register) instead of `Gpr` (physical register)
- Implements `MachInst` trait for regalloc2
- Operand visitor for regalloc2 integration

### 3. Lowering

**File**: `crates/lpc-codegen/src/backend3/lower.rs` (ISA-agnostic, uses ISA-specific MachInst trait)

```rust
// File: crates/lpc-codegen/src/backend3/lower.rs

use crate::isa::riscv32::backend3::MachInst;

/// Lowering context: converts IR to VCode
/// Generic over MachInst trait (ISA-agnostic)
pub struct Lower<I: MachInst> {
    /// Function being lowered
    func: Function,

    /// VCode being built
    vcode: VCodeBuilder<I>,

    /// Value to virtual register mapping (immutable after creation)
    /// Each IR Value maps to exactly one VReg. Created once in create_virtual_registers(),
    /// then only read from during lowering. In SSA form, all Values (including instruction
    /// results and block parameters) exist before lowering, so we can create VRegs upfront.
    value_to_vreg: BTreeMap<Value, VReg>,

    /// Block to block index mapping
    block_to_index: BTreeMap<Block, BlockIndex>,

    /// ABI information (ISA-specific, provided via MachInst trait)
    abi: Callee<I::ABIMachineSpec>,
}

impl Lower {
    /// Lower a function to VCode
    pub fn lower(mut self, block_order: &BlockLoweringOrder) -> VCode<MachInst> {
        // 1. Create virtual registers for all values
        self.create_virtual_registers();

        // 2. Lower blocks in computed order (not IR layout order)
        for lowered_block in block_order.lowered_order() {
            match lowered_block {
                LoweredBlock::Orig { block } => {
                    self.lower_block(block);
                }
                LoweredBlock::Edge { from, to } => {
                    // Emit phi moves for edge block
                    self.lower_edge_block(*from, *to);
                }
            }
        }

        // 3. Build VCode
        self.vcode.build()
    }

    /// Lower an edge block (phi moves)
    fn lower_edge_block(&mut self, from: Block, to: Block) {
        // Get phi values for target block
        let target_params = self.func.block_params(to);
        // Get corresponding source values from predecessor
        // (This requires tracking which values come from which predecessor)
        // Emit moves: vreg_target = vreg_source
        // ...
    }

    /// Create virtual registers for all values
    /// This is called once before lowering. In SSA form, all Values already exist
    /// in the IR (function params, block params, instruction results), so we can
    /// create VRegs for all of them upfront. The mapping is then immutable during lowering.
    fn create_virtual_registers(&mut self) {
        // 1. Function parameters (entry block params)
        for param_value in self.func.block_params(self.func.entry_block()) {
            let vreg = self.vcode.alloc_vreg();
            self.value_to_vreg.insert(*param_value, vreg);
        }

        // 2. Block parameters (phi nodes) - each block's params get VRegs
        for block in self.func.blocks() {
            for param_value in self.func.block_params(block) {
                let vreg = self.vcode.alloc_vreg();
                self.value_to_vreg.insert(*param_value, vreg);
            }
        }

        // 3. Instruction results - each instruction's result Values get VRegs
        for block in self.func.blocks() {
            for inst in self.func.block_insts(block) {
                let inst_data = self.func.dfg.inst_data(inst).unwrap();
                for result_value in &inst_data.results {
                    let vreg = self.vcode.alloc_vreg();
                    self.value_to_vreg.insert(*result_value, vreg);
                }
            }
        }
    }

    /// Lower a block
    fn lower_block(&mut self, block: Block) {
        // Lower each instruction
        for inst in self.func.block_insts(block) {
            self.lower_inst(inst);
        }
    }

    /// Lower an instruction
    fn lower_inst(&mut self, inst: InstEntity) {
        let inst_data = self.func.dfg.inst_data(inst).unwrap();

        match inst_data.opcode {
            Opcode::Iadd => {
                let rs1 = self.value_to_vreg[&inst_data.args[0]];
                let rs2 = self.value_to_vreg[&inst_data.args[1]];
                let rd = self.value_to_vreg[&inst_data.results[0]];
                self.vcode.push(MachInst::Add { rd, rs1, rs2 });
            }
            // ... handle all opcodes ...
        }
    }
}
```

**Key Features**:

- Creates virtual registers for all IR values upfront (immutable mapping)
- Each Value maps to exactly one VReg (1:1 relationship in SSA form)
- Maps IR instructions to machine instructions using the Value→VReg mapping
- Handles block parameters (phi-like values) - they are Values and get VRegs too
- Tracks operand constraints for regalloc2

**Note**: The `value_to_vreg` mapping is immutable after `create_virtual_registers()` completes. This works because in SSA form, all Values (function params, block params, instruction results including constants) exist before lowering begins. During lowering, we only read from this map to get VRegs for operands. This matches Cranelift's approach: each IR Value maps to exactly one VReg, and the mapping is established upfront before any lowering occurs.

### 4. Regalloc2 Integration

**File**: `crates/lpc-codegen/src/backend3/regalloc.rs` (ISA-agnostic)

**Purpose**: Implement `regalloc2::Function` trait for VCode to enable register allocation.

**See**: [Phase 2](17-backend3-2-regalloc2-integration.md) for complete implementation details.

### 5. ABI Machine Spec

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/abi.rs` (RISC-V 32-specific)

```rust
// File: crates/lpc-codegen/src/isa/riscv32/backend3/abi.rs

use regalloc2::{ABIMachineSpec, ...};
use super::inst::MachInst;

/// RISC-V 32-bit ABI machine specification for regalloc2
pub struct Riscv32ABI;

impl ABIMachineSpec for Riscv32ABI {
    type I = MachInst;

    fn callee_saved_gprs() -> &'static [RealReg] {
        // s0-s11 (x8-x9, x18-x27)
    }

    fn caller_saved_gprs() -> &'static [RealReg] {
        // a0-a7, t0-t6 (x5-x7, x10-x17, x28-x31)
    }

    fn fixed_stack_slots() -> &'static [StackSlot] {
        // None for now
    }

    // ... implement ABI methods ...
}
```

**Key Features**:

- Defines register classes
- Specifies callee-saved vs caller-saved
- Configures stack slots
- Handles multi-return (return area)
- Provides frame layout computation helpers
- Generates prologue/epilogue sequences

### 6. Emission

**File**: `crates/lpc-codegen/src/backend3/emit.rs` (ISA-agnostic, uses ISA-specific MachInst trait)

**Architecture**: Streaming emission with label-based branch resolution. Blocks are emitted in final optimized order, and branches use labels that are resolved incrementally as blocks are emitted.

```rust
/// Emit VCode to machine code
impl VCode<MachInst> {
    pub fn emit(
        self,
        regalloc: &regalloc2::Output,
    ) -> InstBuffer {
        let mut state = EmitState::new();
        let mut buffer = InstBuffer::new();

        // 1. Compute emission order (cold blocks at end) BEFORE emission
        let block_order = self.compute_emission_order();

        // 2. Initialize label offsets (all unknown initially)
        state.label_offsets.resize(self.num_blocks(), UNKNOWN_LABEL_OFFSET);

        // 3. Compute frame layout from regalloc results
        let frame_layout = self.compute_frame_layout(regalloc);
        state.frame_size = frame_layout.total_size;
        state.clobbered_callee_saved = frame_layout.clobbered_regs.clone();

        // 4. Emit blocks in final order
        for block_idx in block_order {
            // Bind label for this block
            let label = MachLabel::from_block(block_idx);
            let block_start = buffer.cur_offset();
            state.bind_label(label, block_start);

            // Resolve any pending fixups that targeted this label
            state.resolve_pending_fixups(&mut buffer, label, block_start);

            // Is this the entry block? Emit prologue
            if block_idx == self.entry {
                self.gen_prologue(&mut buffer, &mut state, &frame_layout);
            }

            // Emit block alignment (if needed)
            // For RISC-V 32, blocks are naturally 4-byte aligned (instruction size)
            // Some blocks (e.g., indirect branch targets) may need additional alignment
            // For now, we use natural alignment (no padding needed)
            // Future: add alignment support if needed for performance
            let aligned_offset = I::align_basic_block(buffer.cur_offset());
            while buffer.cur_offset() < aligned_offset {
                // Emit NOPs to align (if alignment > 4 bytes)
                let nop = I::gen_nop(1);
                buffer.emit(nop);
            }

            // Emit block start instruction (if needed)
            // Some ISAs emit special instructions at block start (e.g., for CFI)
            // RISC-V 32 doesn't need this, but trait allows it
            if let Some(block_start) = I::gen_block_start(
                self.block_metadata[block_idx].is_indirect_target,
                false, // forward edge CFI not needed for RISC-V 32
            ) {
                buffer.emit(block_start);
            }

            // Emit instructions and edits
            for inst_or_edit in regalloc.block_insts_and_edits(&self, block_idx) {
                match inst_or_edit {
                    InstOrEdit::Inst(inst_idx) => {
                        let inst = &self.insts[inst_idx];

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
                            buffer.emit(inst.to_physical(&mut state));
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

        // 5. Resolve any remaining forward references (should be none if order is correct)
        state.resolve_all_pending_fixups(&mut buffer);

        // 6. Fix external relocations (function calls, etc.)
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
        // These represent register-to-register moves inserted by regalloc
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
                // Only consider defs (writes)
                if operand.kind() == OperandKind::Def {
                    if let Some(preg) = alloc.as_reg() {
                        clobbered.add(preg);
                    }
                }
            }

            // 3. Add explicitly clobbered registers from instruction clobber lists
            // (if instruction has explicit clobber list and is included in clobbers)
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
                // Check if this is a callee-saved register
                if callee_saved.contains(&real_reg) {
                    clobbered_callee_saved.push(real_reg);
                }
            }
        }

        // Sort for consistent ordering (affects frame layout)
        clobbered_callee_saved.sort();
        clobbered_callee_saved
    }

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
}

/// Emission state for streaming label-based emission
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
        // Find all fixups targeting this label
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
                // This shouldn't happen if emission order is correct
                panic!("Unresolved label fixup: label {:?} not bound", fixup.target_label);
            }
        }
        self.pending_fixups.clear();
    }
}
```

**Key Features**:

- **Streaming Emission**: Blocks emitted in final optimized order
- **Label-Based Branches**: Branches use labels, resolved incrementally as blocks are emitted
- **Emission State Tracking**: Tracks SP offsets, label offsets, pending fixups
- **Frame Layout Computation**: Calculated from regalloc spills + ABI requirements
- **Prologue Generation**: Emitted at entry block (setup area → SP adjustment → callee-saved saves)
- **Epilogue Generation**: Emitted at each return instruction (callee-saved restores → SP restore → return)
- **Block Layout Optimization**: Cold block sinking (computed before emission)
- **Edit Emission**: Moves, spills, reloads inserted between instructions
- **Branch Resolution**: Two-dest branches converted to single-dest during emission
- **External Relocation Fixup**: Function calls and other external relocations resolved after emission

**InstBuffer Enhancements**: See [Phase 3](17-backend3-3-emission.md) for complete API design and implementation details.

**Note**: The following detailed sections have been moved to phase-specific files:

The current `InstBuffer` needs enhancements for label-based emission. Since we're using structured instructions (not raw bytes), we can use a simpler approach than Cranelift's byte-patching:

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

**Simplified Approach**:

Since we're using structured instructions (not raw bytes), we can:

1. Emit branches with placeholder offsets (0)
2. Store instruction indices where branches are (in EmitState)
3. Patch instructions directly when labels are bound (by modifying the `Inst` enum in-place)

This is simpler than Cranelift's byte-patching approach and sufficient for our needs.

**Key Points**:

- RISC-V branch offsets are in 2-byte units (even though instructions are 4 bytes)
- Conditional branches: ±4KB range (12-bit signed × 2 bytes)
- Unconditional jumps: ±1MB range (20-bit signed × 2 bytes)
- Patching happens by modifying the `imm` field of the `Inst` enum
- We assume functions < 4KB for now (panic if out of range)

**Branch Range Limits** (RISC-V 32):

- **Conditional branches** (BEQ, BNE, etc.): ±4KB range (12-bit signed offset × 2 bytes)
- **Unconditional jumps** (JAL): ±1MB range (20-bit signed offset × 2 bytes)

**Assumption**: Functions are < 4KB for now. If larger functions are needed later, veneer/island insertion can be added (similar to Cranelift's MachBuffer).

**Note**: Since we emit blocks in final order, most branches will be backward (to already-emitted blocks), making immediate resolution possible. Forward branches (to not-yet-emitted blocks) are handled via pending fixups that resolve when the target block is emitted.

### Block Alignment

**Purpose**: Some blocks may need alignment for performance or correctness (e.g., indirect branch targets).

**RISC-V 32 Alignment**:

- Instructions are naturally 4-byte aligned (instruction size)
- No special alignment requirements for most blocks
- Indirect branch targets may benefit from alignment (optional, deferred)

**Implementation**:

The `MachInst` trait provides:

```rust
trait MachInst {
    /// Align a basic block offset. Default: no alignment (returns offset unchanged)
    fn align_basic_block(offset: CodeOffset) -> CodeOffset {
        offset
    }

    /// Generate a block start instruction (if needed). Default: None
    fn gen_block_start(
        is_indirect_branch_target: bool,
        is_forward_edge_cfi_enabled: bool,
    ) -> Option<Self> {
        None
    }

    /// Generate a NOP instruction of given size (in instructions, not bytes)
    fn gen_nop(size: usize) -> Self;
}
```

**For RISC-V 32**:

- `align_basic_block()`: Returns offset unchanged (natural 4-byte alignment)
- `gen_block_start()`: Returns `None` (no special block start instruction)
- `gen_nop()`: Generates `ADDI x0, x0, 0` (NOP instruction)

**Future**: If alignment is needed (e.g., 8-byte or 16-byte alignment for indirect branches), implement `align_basic_block()` to return the next aligned offset, and emit NOPs to pad.

## Implementation Phases

The implementation is broken down into 5 phases. Each phase has its own detailed plan document:

- **[Phase 1: Foundation](17-backend3-1-foundation.md)** (Week 1) - Basic structure and lowering
- **[Phase 2: Regalloc2 Integration](17-backend3-2-regalloc2-integration.md)** (Week 2) - Register allocation working
- **[Phase 3: Emission](17-backend3-3-emission.md)** (Week 3) - Generate machine code
- **[Phase 4: Control Flow](17-backend3-4-control-flow.md)** (Week 4) - Branches and calls
- **[Phase 5: Advanced Features](17-backend3-5-advanced-features.md)** (Week 5+) - Complete feature set

See the individual phase documents for detailed task breakdowns, implementation details, testing strategies, and success criteria.

## Additional Components

### 7. Block Lowering Order

**File**: `crates/lpc-codegen/src/backend3/blockorder.rs` (ISA-agnostic)

**Purpose**: Compute block ordering and handle critical edge splitting before lowering.

**Key Features**:

- **Critical Edge Detection**: Identifies edges needing phi moves
- **Edge Block Creation**: Creates intermediate blocks for critical edges
- **Reverse Postorder**: Computes RPO traversal for proper ordering
- **Cold Block Tracking**: Marks blocks for later layout optimization
- **Indirect Target Tracking**: Tracks blocks that are indirect branch targets

**Algorithm**:

1. Build CFG with edge blocks (conceptual, not materialized)
2. Perform DFS traversal
3. Compute reverse postorder
4. Mark cold blocks (if profile data available)
5. Track indirect branch targets

### 8. Constant Handling

**File**: `crates/lpc-codegen/src/backend3/constants.rs` (ISA-agnostic)

**Purpose**: Handle constant materialization and storage.

**See**: [Phase 1](17-backend3-1-foundation.md) for complete implementation details including decision criteria, special cases, and implementation notes.

### 9. Relocation Handling

**File**: `crates/lpc-codegen/src/backend3/reloc.rs` (ISA-agnostic)

**Purpose**: Track and resolve relocations (function calls, etc.).

**Relocation Types**:

- **Function Call**: Direct call to function (needs address fixup)
- **Indirect Call**: Call via register (no relocation needed)
- **Branch**: Conditional/unconditional branch (resolved during emission)
- **Constant Pool**: Reference to constant pool (future)

**Lifecycle**:

1. **During Lowering**: Record relocations in VCode
2. **During Emission**: Record relocation positions in emission state
3. **After Emission**: Fix up relocations with actual addresses

**Implementation**:

```rust
pub enum RelocKind {
    /// Function call (needs function address)
    FunctionCall,
    /// Branch target (resolved during emission)
    Branch,
}

pub struct Reloc {
    /// Offset in instruction buffer where relocation occurs
    pub offset: CodeOffset,
    /// Relocation type
    pub kind: RelocKind,
    /// Target identifier (function name, etc.)
    pub target: String,
}

impl VCode {
    fn record_reloc(&mut self, inst_idx: InsnIndex, kind: RelocKind, target: String) {
        self.relocations.push(VCodeReloc {
            inst_idx,
            kind,
            target,
        });
    }
}
```

### 10. Branch Resolution

**File**: `crates/lpc-codegen/src/backend3/branch.rs` (ISA-agnostic)

**Purpose**: Resolve two-dest branches to single-dest branches during emission, and perform basic branch optimizations.

**See**: [Phase 4](17-backend3-4-control-flow.md) for complete implementation details including fallthrough detection algorithm and branch conversion logic.

## File Structure

### ISA-Agnostic Code (Generic Backend Infrastructure)

```
crates/lpc-codegen/src/backend3/
├── mod.rs                 # Main module, compile_function entry point
├── vcode.rs               # VCode structure (generic over MachInst)
├── vcode_builder.rs       # VCode builder
├── blockorder.rs          # Block lowering order computation
├── lower.rs               # Lowering (IR → VCode, generic)
├── constants.rs           # Constant materialization
├── regalloc.rs            # Regalloc2 integration (generic)
├── emit.rs                # Emission (VCode → Machine code, generic)
├── branch.rs              # Branch resolution and optimization
├── reloc.rs               # Relocation handling
└── tests/
    ├── mod.rs
    ├── blockorder_tests.rs # Block ordering tests
    ├── lower_tests.rs      # Lowering tests
    ├── regalloc_tests.rs   # Regalloc tests
    ├── emit_tests.rs       # Emission tests
    └── branch_tests.rs     # Branch resolution tests
```

### RISC-V 32-Specific Code

```
crates/lpc-codegen/src/isa/riscv32/backend3/
├── mod.rs                 # RISC-V 32 backend3 module
├── inst.rs                # MachInst enum (RISC-V instructions with VReg)
├── abi.rs                 # Riscv32ABI (ABI machine spec for regalloc2)
├── lower.rs               # RISC-V specific lowering helpers (if needed)
├── emit.rs                # RISC-V specific emission helpers (if needed)
└── tests/
    ├── mod.rs
    └── integration_tests.rs  # RISC-V specific integration tests
```

**Note**: The ISA-agnostic code uses traits (e.g., `MachInst`) that are implemented by ISA-specific types. The RISC-V 32 implementation provides the concrete `MachInst` enum and `Riscv32ABI` that plug into the generic infrastructure.

## Key Design Decisions

### 1. Virtual Registers

**Decision**: Use `regalloc2::VReg` for virtual registers.

**Rationale**:

- Compatible with regalloc2
- Clear separation from physical registers
- Enables proper register allocation

### 2. Operand Representation

**Decision**: Flat operand array with ranges (Cranelift-inspired design).

**Rationale**:

- Efficient access for regalloc2
- Simple to implement
- Proven design pattern used by modern code generators

### 3. Edits

**Decision**: Use regalloc2's Edit mechanism.

**Rationale**:

- Explicit representation of allocation decisions
- Easy to test and debug
- Clean separation of concerns

### 4. Multi-Return

**Decision**: Implement return area mechanism from the start.

**Rationale**:

- Required for proper ABI compliance
- Better than panicking
- Matches RISC-V ABI specification

## Testing Strategy

### Unit Tests

1. **Lowering tests**

   - Test each IR opcode → machine instruction
   - Test virtual register creation
   - Test block parameter handling

2. **Regalloc tests**

   - Test regalloc2 integration
   - Test allocation decisions
   - Test edit generation

3. **Emission tests**
   - Test instruction emission
   - Test edit emission (moves, spills, reloads)
   - Test prologue/epilogue

### Integration Tests

1. **End-to-end tests**

   - Compile simple functions
   - Execute and verify results
   - Compare with current backend

2. **Multi-return tests**

   - Test functions with 3+ returns
   - Test call/return with multi-return
   - Verify return area mechanism

3. **Complex function tests**
   - Test functions with branches
   - Test functions with calls
   - Test register pressure

## Migration Path

### Phase 1: Parallel Implementation

- Implement backend3 alongside current backend
- Keep current backend working
- Test backend3 incrementally

### Phase 2: Feature Parity

- Match all current backend features
- Pass all existing tests
- Performance comparison

### Phase 3: Switchover

- Update module to use backend3
- Remove old backend (`isa/riscv32/backend_old/`) or keep as reference
- Update documentation

## Dependencies

### External Crates

- `regalloc2`: Register allocation

### Internal Dependencies

- `lpc-lpir`: IR types (includes `PrimaryMap` for entity maps)
- `lpc-codegen`: Instruction types, InstBuffer
- `isa/riscv32/backend/`: Reference implementation (frame layout, ABI)

## Performance Considerations

### Regalloc2

- Uses efficient algorithms (Ion/Fastalloc)
- Should be faster than current linear scan
- Better register allocation quality

### VCode Structure

- Flat arrays for efficient access
- Minimal indirection
- Cache-friendly layout

### Emission

- Single pass through instructions
- Edits inserted efficiently
- Minimal allocations

## Open Questions

1. **Block ordering**: Use layout order or optimize?

   - **Answer**: Start with reverse postorder (RPO), add cold block optimization later
   - Block lowering order computed before lowering
   - Block layout optimization during emission

2. **Constant handling**: Inline or pool?

   - **Answer**: Start with inline (lui + addi for large constants), add pooling later if needed
   - Most constants fit in 12-bit immediates
   - Large constants use lui + addi sequence

3. **Debug information**: How to preserve?

   - **Answer**: Add later, focus on correctness first
   - Can add source location tracking to VCode
   - Emit debug info during emission

4. **Error handling**: Panic or Result?

   - **Answer**: Use Result for user-facing APIs, panic for internal invariants
   - `compile_function` returns `Result<InstBuffer, CodegenError>`
   - Internal invariants use `debug_assert!` or `unreachable!`

5. **Frame pointer**: Always use or only when needed?

   - **Answer**: Start without frame pointer, add if needed for debugging
   - SP-relative addressing sufficient for most cases
   - Can add frame pointer later for easier debugging

6. **Cold block optimization**: When to enable?

   - **Answer**: Start without, add profile-guided optimization later
   - Block metadata tracks cold blocks
   - Layout optimization moves them to end

## Success Criteria

1. ✅ Can compile simple arithmetic functions
2. ✅ Can compile functions with branches
3. ✅ Can compile functions with calls
4. ✅ Supports multi-return (3+ values)
5. ✅ Proper register allocation
6. ✅ Handles register pressure (spilling)
7. ✅ Generates correct machine code
8. ✅ Passes all existing tests
9. ✅ Performance comparable or better than current backend

## Related Documents

### Implementation Plans

- `docs/plans/17-backend3-1-foundation.md` - Phase 1: Foundation
- `docs/plans/17-backend3-2-regalloc2-integration.md` - Phase 2: Regalloc2 Integration
- `docs/plans/17-backend3-3-emission.md` - Phase 3: Emission
- `docs/plans/17-backend3-4-control-flow.md` - Phase 4: Control Flow
- `docs/plans/17-backend3-5-advanced-features.md` - Phase 5: Advanced Features

### Supporting Documents

- `docs/plans/17-backend3-notes.md` - Remaining questions and open issues
- `docs/plans/17-backend3-deferred.md` - Deferred features and optimizations
- `docs/plans/16-lpir-improvements-for-backend.md` - LPIR improvements needed
- `docs/plans/10.5-call-handling.md` - Call/return handling details
- `docs/riscv32-abi.md` - RISC-V 32-bit ABI specification
- `docs/plans/06-register-allocation.md` - Register allocation design

## Implementation Notes

### Getting Started

1. Create `backend3/mod.rs` with basic structure (ISA-agnostic)
2. Create `isa/riscv32/backend3/mod.rs` for RISC-V 32-specific code
3. Implement `BlockLoweringOrder` (ISA-agnostic)
   - Critical edge detection
   - Reverse postorder computation
4. Implement `VCode` and `VCodeBuilder` (ISA-agnostic)
   - Block structure, instructions, operands
   - Block metadata, relocations
5. Implement `MachInst` enum with VReg operands (RISC-V 32-specific)
6. Implement `MachInst` trait for regalloc2 integration
7. Implement constant handling (ISA-agnostic)
   - Inline constants, large constant materialization
8. Implement basic lowering (ISA-agnostic, uses MachInst trait)
   - Use block lowering order
   - Handle edge blocks (phi moves)
9. Implement `Riscv32ABI` (RISC-V 32-specific)
   - Register classes, callee-saved/caller-saved
   - Frame layout helpers
10. Integrate regalloc2 (ISA-agnostic)
    - Implement `regalloc2::Function` trait
11. Implement emission (ISA-agnostic, uses MachInst trait)
    - Frame layout computation
    - Prologue/epilogue generation
    - Edit emission
    - Relocation fixup
12. Implement branch resolution (ISA-agnostic)
    - Two-dest to single-dest conversion
    - Basic branch simplification
13. Test incrementally

### Code Style

- Follow existing code style
- Use `just fmt` before committing
- Add tests for each feature
- Document public APIs

### Debugging

- Add `Debug` implementations
- Use `dbg!()` for debugging
- Print VCode structure
- Print regalloc results
- Print emitted code
