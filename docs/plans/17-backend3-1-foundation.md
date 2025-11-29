# Backend3 Phase 1: Foundation

**Goal**: Basic structure and lowering

**Timeline**: Week 1

**Deliverable**: Can lower simple arithmetic functions to VCode

## Cranelift References

**Primary Reference**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/machinst/`

- **VCode Structure**: `vcode.rs` - Complete VCode implementation with operands, blocks, constants, relocations
- **Block Ordering**: `blockorder.rs` - Block lowering order computation with critical edge splitting
- **Lowering**: `lower.rs` - IR to VCode lowering implementation
- **MachInst Trait**: `mod.rs` - MachInst trait definition and related types
- **Compilation Pipeline**: `compile.rs` - Main compilation entry point (lower → regalloc → emit)

**RISC-V Specific References**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/`

- **Machine Instructions**: `inst/mod.rs` - RISC-V 64-bit MachInst enum with VReg operands
- **ABI**: `abi.rs` - RISC-V 64-bit ABI machine spec implementation
- **Lowering**: `lower.rs` - RISC-V specific lowering helpers
- **Emission**: `inst/emit.rs` - RISC-V instruction emission with EmitState

## Tasks

### 1. Create VCode structure (ISA-agnostic)

**Files**: 
- `backend3/vcode.rs`: Core VCode type (generic over MachInst)
- `backend3/vcode_builder.rs`: Builder for constructing VCode

**VCode Structure**:

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

    /// Source locations for each instruction (for debugging)
    /// One RelSourceLoc per instruction, parallel to `insts` array
    srclocs: Vec<RelSourceLoc>,

    // Optional fields (deferred):
    // vreg_types: Vec<Type>,  // VReg types (add if needed for validation)
    // debug_value_labels: Vec<(VReg, InsnIndex, InsnIndex, u32)>,  // Debug info (deferred)
}
```

**VCode Structure Completeness**:

Comparing with Cranelift's VCode, here's what we include vs. defer:

**Included (Required for Basic Functionality)**:
- ✅ `insts`: Machine instructions
- ✅ `operands`: Flat operand array for regalloc2
- ✅ `operand_ranges`: Per-instruction operand ranges
- ✅ `clobbers`: Explicit clobber sets (for function calls)
- ✅ `block_ranges`: Per-block instruction ranges
- ✅ `block_succs` / `block_preds`: Block structure
- ✅ `block_params`: Block parameter VRegs
- ✅ `branch_block_args`: Branch argument VRegs
- ✅ `entry`: Entry block
- ✅ `abi`: ABI information
- ✅ `constants`: Constant storage
- ✅ `block_metadata`: Cold blocks, indirect targets, alignment
- ✅ `relocations`: Function calls and other relocations
- ✅ `block_order`: Block lowering order

**Deferred (Optional for Initial Implementation)**:
- ❌ `vreg_types`: VReg types (for type checking) - **Add if needed for validation**
- ❌ `debug_tags`: Debug tags - **Deferred** (not needed initially)
- ❌ `user_stack_maps`: Stack maps for safepoints - **Deferred** (GC not needed)
- ❌ `debug_value_labels`: Value labels for debug info - **Deferred** (debug info later)
- ❌ `facts`: Proof-carrying code facts - **Deferred** (advanced feature)
- ❌ `emit_info`: ISA-specific emission info - **May need** (for instruction encoding)

**Note**: `srclocs` is included for debugging support. See source location tracking section below.

### 2. Block lowering order (ISA-agnostic)

**File**: `backend3/blockorder.rs`

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

**Algorithm**:

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

### 3. Create machine instruction type (RISC-V 32-specific)

**File**: `isa/riscv32/backend3/inst.rs`

**Components**:
- MachInst enum with VReg operands
- Implement basic instructions (add, addi, lw, sw)
- Implement MachInst trait for regalloc2 (operand visitor)

**Components**:
- MachInst enum with VReg operands
- Implement basic instructions (add, addi, lw, sw)
- Implement MachInst trait for regalloc2 (operand visitor)

**Example**:

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

### 4. Basic lowering (ISA-agnostic)

**File**: `backend3/lower.rs`

**Components**:
- Lower simple instructions (iconst, iadd, isub)
- Use block lowering order
- Create virtual registers for values
- Build VCode structure
- Handle edge blocks (phi moves)
- Track source locations from IR instructions
- Uses MachInst trait (implemented by RISC-V 32 MachInst)

**Source Location Tracking During Lowering**:

When lowering an IR instruction, capture its source location:

```rust
impl Lower {
    fn lower_inst(&mut self, inst: InstEntity) {
        // Get source location from IR instruction
        let ir_srcloc = self.func.srcloc(inst);
        
        // Convert to RelSourceLoc (relative to function's base source location)
        let base_srcloc = self.func.base_srcloc();
        let rel_srcloc = RelSourceLoc::from_base_offset(base_srcloc, ir_srcloc);
        
        // Lower the instruction
        let mach_inst = match inst_data.opcode {
            Opcode::Iadd => {
                // ... create MachInst ...
            }
            // ...
        };
        
        // Push instruction with source location
        self.vcode.push(mach_inst, rel_srcloc);
    }
}
```

**Handling Synthetic Instructions**:

For instructions created during lowering (e.g., constant materialization, phi moves):
- If they correspond to an IR instruction: use that instruction's source location
- If they're truly synthetic (no IR equivalent): use `RelSourceLoc::default()`

**Lowering Implementation**:

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

### 5. Constant handling (ISA-agnostic)

**File**: `backend3/constants.rs`

**Purpose**: Handle constant materialization and storage.

**Constant Materialization Strategies**:

1. **Inline Constants**: Small immediates embedded directly in instructions
   - **RISC-V 32**: 12-bit signed immediates for `addi`, `lw`, `sw`, etc.
   - **Range**: -2048 to +2047 (fits in 12 bits)
   - **Decision**: Use inline if `value >= -2048 && value <= 2047`
   - **No extra instructions needed**: Constant is part of instruction encoding

2. **LUI + ADDI Sequence**: Large constants that don't fit in 12 bits
   - **RISC-V 32**: Use `lui` (load upper 20 bits) + `addi` (add lower 12 bits)
   - **Range**: Full 32-bit signed range
   - **Decision**: Use if `value < -2048 || value > 2047`
   - **Cost**: 2 instructions (lui + addi)

3. **Constant Pool** (deferred): Very large constants or frequently used constants
   - **Future**: Store in data section, load via PC-relative addressing
   - **Use cases**: 64-bit constants, floating point constants, shared constants
   - **Not needed for initial implementation**

**Decision Criteria**:

```rust
impl Lower {
    /// Determine if a constant fits in 12-bit signed immediate
    fn fits_in_12_bits(&self, value: i32) -> bool {
        value >= -2048 && value <= 2047
    }
    
    /// Materialize a constant, choosing the appropriate strategy
    fn materialize_constant(&mut self, value: i32) -> VReg {
        if self.fits_in_12_bits(value) {
            // Strategy 1: Inline immediate
            // The constant will be embedded directly in the instruction
            // during lowering (e.g., addi rd, rs1, value)
            // Return a VReg that represents this constant value
            // (The actual instruction will use the immediate directly)
            self.materialize_inline_constant(value)
        } else {
            // Strategy 2: LUI + ADDI sequence
            self.materialize_large_constant(value)
        }
    }
    
    /// Materialize inline constant (fits in 12 bits)
    /// Returns a VReg that will be used in instructions with immediate operands
    fn materialize_inline_constant(&mut self, value: i32) -> VReg {
        // For inline constants, we don't need to emit instructions
        // The constant is embedded in the instruction itself
        // However, we still need a VReg to represent the value in the IR
        // This VReg will be marked as a constant/immediate in the instruction
        let vreg = self.vcode.alloc_vreg();
        // Store constant value for later use during instruction emission
        self.vcode.record_constant(vreg, Constant::Inline(value));
        vreg
    }
    
    /// Materialize large constant via LUI + ADDI
    fn materialize_large_constant(&mut self, value: i32) -> VReg {
        let vreg = self.vcode.alloc_vreg();
        
        // Split value into upper 20 bits and lower 12 bits
        // Note: RISC-V sign-extends the lower 12 bits, so we need to handle sign correctly
        let lower_12 = value & 0xFFF;
        let upper_20 = (value >> 12) & 0xFFFFF;
        
        // If lower 12 bits have sign bit set (bit 11), we need to adjust upper
        // because addi sign-extends the immediate
        let upper = if (lower_12 & 0x800) != 0 {
            // Sign bit set in lower, increment upper
            (upper_20 + 1) & 0xFFFFF
        } else {
            upper_20
        };
        
        // Emit LUI: load upper 20 bits
        let temp_vreg = self.vcode.alloc_vreg();
        self.vcode.push(MachInst::Lui {
            rd: temp_vreg,
            imm: (upper << 12) as u32,
        });
        
        // Emit ADDI: add lower 12 bits (sign-extended)
        self.vcode.push(MachInst::Addi {
            rd: vreg,
            rs1: temp_vreg,
            imm: lower_12 as i32,
        });
        
        vreg
    }
}
```

**Special Cases**:

1. **Zero Constant**: Can use `x0` register directly (no instruction needed)
2. **Small Positive Constants**: Use `addi rd, x0, imm` (loads immediate into register)
3. **Negative Constants**: Use `addi rd, x0, imm` (sign-extended, works for -2048 to -1)

**Future Optimizations** (Deferred):

- **Constant Pool**: For frequently used large constants
- **Constant Deduplication**: Share constants across instructions
- **64-bit Constants**: Would need constant pool or multiple instructions

**Implementation Notes**:

- Constants are materialized during lowering (not during emission)
- Inline constants don't require separate instructions
- Large constants require 2 instructions (lui + addi)
- The VReg for a constant represents the value, not a register (until regalloc assigns one)

### 6. Relocation handling (ISA-agnostic)

**File**: `backend3/reloc.rs`

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

**Note**: Relocation fixup happens during emission (Phase 3). See [Phase 3](17-backend3-3-emission.md) for emission-time relocation handling.

## Testing

- Unit tests for VCode structure
- Unit tests for block ordering
- Unit tests for lowering simple instructions
- Unit tests for constant materialization
- Integration test: Lower simple arithmetic function to VCode

## Success Criteria

- ✅ VCode structure can represent simple functions
- ✅ Block ordering computes correct order
- ✅ Can lower iconst, iadd, isub instructions
- ✅ Can materialize constants (inline and large)
- ✅ Can create VCode from simple IR function

