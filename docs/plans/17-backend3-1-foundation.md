# Backend3 Phase 1: Foundation

**Goal**: Basic structure and lowering

**Timeline**: Week 1

**Deliverable**: Can lower simple arithmetic functions to VCode

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

    // Optional fields (deferred):
    // vreg_types: Vec<Type>,  // VReg types (add if needed for validation)
    // srclocs: Vec<RelSourceLoc>,  // Source locations (deferred)
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
- ❌ `srclocs`: Source locations per instruction - **Deferred** (see notes)
- ❌ `debug_tags`: Debug tags - **Deferred** (not needed initially)
- ❌ `user_stack_maps`: Stack maps for safepoints - **Deferred** (GC not needed)
- ❌ `debug_value_labels`: Value labels for debug info - **Deferred** (debug info later)
- ❌ `facts`: Proof-carrying code facts - **Deferred** (advanced feature)
- ❌ `emit_info`: ISA-specific emission info - **May need** (for instruction encoding)

**Recommendation**: Start with the included fields. Add `vreg_types` if we need type validation, and `emit_info` if needed for RISC-V-specific emission details. All other fields can be added later if needed.

### 2. Block lowering order (ISA-agnostic)

**File**: `backend3/blockorder.rs`

**Components**:
- Critical edge detection
- Reverse postorder computation
- Basic implementation (no cold block optimization yet)

**See**: Main plan for BlockLoweringOrder details (`17-backend3.md`)

### 3. Create machine instruction type (RISC-V 32-specific)

**File**: `isa/riscv32/backend3/inst.rs`

**Components**:
- MachInst enum with VReg operands
- Implement basic instructions (add, addi, lw, sw)
- Implement MachInst trait for regalloc2 (operand visitor)

**See**: Main plan for MachInst details (`17-backend3.md`)

### 4. Basic lowering (ISA-agnostic)

**File**: `backend3/lower.rs`

**Components**:
- Lower simple instructions (iconst, iadd, isub)
- Use block lowering order
- Create virtual registers for values
- Build VCode structure
- Handle edge blocks (phi moves)
- Uses MachInst trait (implemented by RISC-V 32 MachInst)

**See**: Main plan for Lowering details (`17-backend3.md`)

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

