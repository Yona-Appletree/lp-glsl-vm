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

**Key Steps**:

1. **Initialize Emission State**: Track SP offsets, block positions, relocations
2. **Compute Frame Layout**: Calculate frame size from regalloc spills + ABI requirements
3. **Generate Prologue**: Frame setup, callee-saved saves
4. **Emit Instructions**: Iterate blocks, apply allocations, insert edits
5. **Block Layout Optimization**: Reorder blocks (cold sinking, fallthrough optimization)
6. **Branch Resolution**: Resolve two-dest branches, simplify branches
7. **Generate Epilogue**: Callee-saved restores, frame cleanup, return
8. **Fix Relocations**: Resolve function call addresses and other relocations

**Emission State Tracking**:

```rust
struct EmitState {
    /// Current stack pointer offset (for SP-relative addressing)
    sp_offset: i32,
    /// Block start offsets (for branch target computation)
    block_offsets: Vec<CodeOffset>,
    /// Relocations to fix up (position, type, target)
    relocations: Vec<Reloc>,
    /// Prologue/epilogue state
    frame_size: u32,
    clobbered_callee_saved: Vec<RealReg>,
}
```

## Key Components

### 1. VCode Structure

**File**: `crates/lpc-codegen/src/backend3/vcode.rs` (ISA-agnostic)

```rust
/// Virtual-register code: machine instructions with virtual registers
pub struct VCode<I: MachInst> {
    /// Machine instructions (with VReg operands)
    insts: Vec<I>,

    /// Operands: flat array for regalloc2
    /// Each operand has: (value, constraint, kind)
    operands: Vec<Operand>,

    /// Operand ranges: per-instruction ranges in operands array
    operand_ranges: Ranges,

    /// Block structure
    block_ranges: Ranges,           // Per-block instruction ranges
    block_succs: Vec<BlockIndex>,   // Successors
    block_preds: Vec<BlockIndex>,    // Predecessors
    block_params: Vec<VReg>,         // Block parameter VRegs

    /// Branch arguments (values passed to blocks)
    branch_block_args: Vec<VReg>,
    branch_block_arg_range: Ranges,

    /// Entry block
    entry: BlockIndex,

    /// ABI information
    abi: Callee<ABIMachineSpec>,

    /// Constants (inline or pool references)
    constants: VCodeConstants,

    /// Block metadata
    block_metadata: Vec<BlockMetadata>,

    /// Relocations (function calls, etc.)
    relocations: Vec<VCodeReloc>,
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

```rust
use regalloc2::{Function as RegallocFunction, ...};

/// Implement regalloc2::Function trait for VCode
impl RegallocFunction for VCode<MachInst> {
    type Inst = InsnIndex;
    type Block = BlockIndex;
    type VReg = VReg;
    type PReg = RealReg;

    fn blocks(&self) -> &[BlockIndex] {
        // Return all blocks
    }

    fn block_insts(&self, block: BlockIndex) -> &[InsnIndex] {
        // Return instruction indices for block
    }

    fn block_succs(&self, block: BlockIndex) -> &[BlockIndex] {
        // Return successors
    }

    fn block_params(&self, block: BlockIndex) -> &[VReg] {
        // Return block parameter VRegs
    }

    fn inst_operands(&self, inst: InsnIndex) -> &[Operand] {
        // Return operands for instruction
    }

    // ... implement all required methods ...
}
```

**Key Features**:

- Implements `regalloc2::Function` trait
- Provides block structure to regalloc2
- Provides operand information
- Configures ABI machine spec

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

```rust
/// Emit VCode to machine code
impl VCode<MachInst> {
    pub fn emit(
        self,
        regalloc: &regalloc2::Output,
    ) -> InstBuffer {
        let mut state = EmitState::new();
        let mut buffer = InstBuffer::new();

        // 1. Compute frame layout from regalloc results
        let frame_layout = self.compute_frame_layout(regalloc);
        state.frame_size = frame_layout.total_size;
        state.clobbered_callee_saved = frame_layout.clobbered_regs.clone();

        // 2. Generate prologue
        self.gen_prologue(&mut buffer, &mut state, &frame_layout);

        // 3. Emit blocks (with layout optimization)
        let block_order = self.compute_emission_order(); // Reorder for optimization
        for block_idx in block_order {
            state.block_offsets[block_idx] = buffer.instruction_count();

            // Emit block start (alignment, if needed)
            if let Some(align) = self.block_metadata[block_idx].alignment {
                self.emit_block_align(&mut buffer, align, &mut state);
            }

            // Emit instructions and edits
            for inst_or_edit in regalloc.block_insts_and_edits(&self, block_idx) {
                match inst_or_edit {
                    InstOrEdit::Inst(inst_idx) => {
                        // Apply register allocations to operands
                        let mut inst = self.insts[inst_idx].clone();
                        inst.apply_allocations(&regalloc.allocs[inst_idx]);

                        // Handle relocations
                        if let Some(reloc) = self.find_reloc(inst_idx) {
                            state.relocations.push(Reloc {
                                offset: buffer.instruction_count(),
                                kind: reloc.kind,
                                target: reloc.target.clone(),
                            });
                        }

                        // Emit instruction
                        buffer.emit(inst.to_physical(&mut state));
                    }
                    InstOrEdit::Edit(edit) => {
                        self.emit_edit(&mut buffer, edit, &mut state);
                    }
                }
            }
        }

        // 4. Generate epilogue
        self.gen_epilogue(&mut buffer, &mut state, &frame_layout);

        // 5. Fix relocations
        self.fix_relocations(&mut buffer, &state);

        buffer
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

    fn fix_relocations(&self, buffer: &mut InstBuffer, state: &EmitState) {
        // Resolve function call addresses
        for reloc in &state.relocations {
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
```

**Key Features**:

- **Emission State Tracking**: Tracks SP offsets, block positions, relocations
- **Frame Layout Computation**: Calculated from regalloc spills + ABI requirements
- **Prologue Generation**: Setup area → SP adjustment → callee-saved saves
- **Epilogue Generation**: Callee-saved restores → SP restore → return
- **Block Layout Optimization**: Cold block sinking, fallthrough optimization
- **Edit Emission**: Moves, spills, reloads inserted between instructions
- **Branch Resolution**: Two-dest branches resolved, branches simplified
- **Relocation Fixup**: Function calls and other relocations resolved

## Implementation Phases

### Phase 1: Foundation (Week 1)

**Goal**: Basic structure and lowering

1. **Create VCode structure** (ISA-agnostic)

   - `backend3/vcode.rs`: Core VCode type (generic over MachInst)
   - `backend3/vcode_builder.rs`: Builder for constructing VCode
   - Basic block and instruction tracking
   - Block metadata (cold, indirect targets)
   - Relocation tracking

2. **Block lowering order** (ISA-agnostic)

   - `backend3/blockorder.rs`: Block ordering computation
   - Critical edge detection
   - Reverse postorder computation
   - Basic implementation (no cold block optimization yet)

3. **Create machine instruction type** (RISC-V 32-specific)

   - `isa/riscv32/backend3/inst.rs`: MachInst enum with VReg operands
   - Implement basic instructions (add, addi, lw, sw)
   - Implement MachInst trait for regalloc2 (operand visitor)

4. **Basic lowering** (ISA-agnostic)

   - `backend3/lower.rs`: Lower simple instructions (iconst, iadd, isub)
   - Use block lowering order
   - Create virtual registers for values
   - Build VCode structure
   - Handle edge blocks (phi moves)
   - Uses MachInst trait (implemented by RISC-V 32 MachInst)

5. **Constant handling** (ISA-agnostic)
   - `backend3/constants.rs`: Constant materialization
   - Inline constants (12-bit immediates)
   - Large constants (lui + addi)

**Deliverable**: Can lower simple arithmetic functions to VCode

### Phase 2: Regalloc2 Integration (Week 2)

**Goal**: Register allocation working

1. **Implement regalloc2::Function trait** (ISA-agnostic)

   - `backend3/regalloc.rs`: Trait implementation
   - Block structure methods
   - Operand methods

2. **ABI machine spec** (RISC-V 32-specific)

   - `isa/riscv32/backend3/abi.rs`: Riscv32ABI implementation
   - Register classes
   - Callee-saved vs caller-saved

3. **Test regalloc2**
   - Run regalloc2 on simple VCode
   - Verify allocations and edits

**Deliverable**: Can allocate registers for simple functions

### Phase 3: Emission (Week 3)

**Goal**: Generate machine code

1. **Emission state tracking** (ISA-agnostic)

   - Track SP offsets for stack-relative addressing
   - Track block offsets for branch targets
   - Track relocations

2. **Frame layout computation** (ISA-agnostic)

   - Compute frame size from regalloc spills
   - Compute ABI requirements
   - Determine clobbered callee-saved registers

3. **Prologue/epilogue generation** (ISA-agnostic, uses ISA-specific ABI)

   - Generate prologue: setup area → SP adjustment → callee-saved saves
   - Generate epilogue: callee-saved restores → SP restore → return
   - Uses ABI trait for frame layout details

4. **Emission implementation** (ISA-agnostic)

   - `backend3/emit.rs`: Apply allocations and emit code
   - Handle edits (moves, spills, reloads)
   - Iterate blocks and instructions
   - Record relocations

5. **Instruction conversion** (RISC-V 32-specific)

   - Convert VReg operands to physical registers in MachInst
   - Handle stack slots (compute offsets)
   - Implement MachInst methods for emission

6. **End-to-end test**
   - Lower → Regalloc → Emit → Execute

**Deliverable**: Can compile simple functions end-to-end

### Phase 4: Control Flow (Week 4)

**Goal**: Branches and calls

1. **Branch lowering**

   - Lower Jump and Br instructions
   - Handle block parameters (phi moves in edge blocks)
   - Two-dest branch representation
   - Record branch relocations

2. **Branch resolution** (ISA-agnostic)

   - `backend3/branch.rs`: Resolve two-dest branches
   - Convert to single-dest branches during emission
   - Basic branch simplification (empty block elimination)

3. **Call lowering**

   - Lower Call instructions
   - Argument preparation (registers + stack)
   - Return value handling
   - Record function call relocations

4. **Multi-return support**

   - Return area mechanism
   - Handle >2 return values
   - Return area pointer passing

5. **Relocation fixup** (ISA-agnostic)

   - `backend3/reloc.rs`: Relocation handling
   - Fix function call addresses
   - Resolve branch targets

**Deliverable**: Can compile functions with branches and calls

### Phase 5: Advanced Features (Week 5+)

**Goal**: Complete feature set

1. **Memory operations**

   - Load/store lowering
   - Stack frame access (SP-relative addressing)
   - Frame pointer usage (if needed)

2. **Block layout optimization**

   - Cold block sinking
   - Fallthrough optimization
   - Block reordering based on branch probabilities

3. **Branch optimization**

   - Advanced branch simplification
   - Unconditional branch elimination
   - Branch target optimization

4. **Constant pool** (optional)

   - Large constant storage in data section
   - PC-relative constant loading

5. **Module compilation**
   - Multi-function compilation
   - Function address resolution
   - Cross-function relocation fixup

**Deliverable**: Complete backend matching current backend features

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

**Strategies**:

1. **Inline Constants**: Small immediates embedded in instructions

   - RISC-V: 12-bit signed immediates for `addi`, `lw`, `sw`
   - Larger constants: `lui` + `addi` sequence

2. **Constant Pool** (future): Large constants stored in data section
   - Used for 64-bit constants, floating point constants
   - Loaded via PC-relative addressing

**Implementation**:

```rust
pub enum Constant {
    /// Inline immediate (fits in instruction encoding)
    Inline(i32),
    /// Large constant (requires lui + addi)
    Large(i32),
    /// Constant pool reference (future)
    PoolRef(usize),
}

impl Lower {
    fn materialize_constant(&mut self, value: i32) -> VReg {
        if self.fits_in_12_bits(value) {
            // Use inline immediate
            let vreg = self.vcode.alloc_vreg();
            // Will be handled during instruction lowering
            vreg
        } else {
            // Materialize via lui + addi
            self.materialize_large_constant(value)
        }
    }

    fn materialize_large_constant(&mut self, value: i32) -> VReg {
        let vreg = self.vcode.alloc_vreg();
        let upper = (value >> 12) & 0xFFFFF;
        let lower = value & 0xFFF;

        // lui rd, upper
        // addi rd, rd, lower
        // ...
        vreg
    }
}
```

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

**Purpose**: Resolve two-dest branches and optimize branch sequences.

**Two-Dest Branches**:

During lowering, conditional branches are represented with two targets:

- `Branch { kind, rs1, rs2, target_true, target_false }`

During emission, these are resolved to single-dest branches:

- If `target_false` is fallthrough: emit conditional branch to `target_true`
- Otherwise: emit inverted conditional branch to `target_false`, then fallthrough to `target_true`

**Branch Simplification**:

- **Empty Block Elimination**: Remove empty blocks, redirect branches
- **Fallthrough Optimization**: Arrange blocks so branches can fall through
- **Unconditional Branch Elimination**: Remove unnecessary jumps

**Implementation**:

```rust
impl VCode {
    fn resolve_branch(
        &self,
        branch: &MachInst,
        state: &EmitState,
    ) -> Vec<MachInst> {
        match branch {
            MachInst::Branch { kind, rs1, rs2, target_true, target_false } => {
                let true_offset = state.block_offsets[*target_true];
                let false_offset = state.block_offsets[*target_false];
                let current_offset = state.current_offset;

                // Determine which target is fallthrough
                let (target, invert) = if false_offset == current_offset + 4 {
                    (*target_true, false)
                } else {
                    (*target_false, true)
                };

                // Emit conditional branch
                vec![MachInst::Branch {
                    kind: if invert { kind.invert() } else { *kind },
                    rs1: *rs1,
                    rs2: *rs2,
                    target,
                }]
            }
            // ...
        }
    }
}
```

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
