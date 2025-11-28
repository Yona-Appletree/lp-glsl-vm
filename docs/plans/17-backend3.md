# Backend3: Cranelift-Style Backend Architecture

## Overview

This document outlines the design and implementation plan for `backend3`, a new RISC-V 32-bit backend that follows Cranelift's architecture. The new backend separates concerns cleanly: **Lowering → VCode (virtual registers) → Register Allocation → Emission**.

**Architecture Separation**: The backend3 implementation is split into two parts:

- **ISA-agnostic code** (`crates/lpc-codegen/src/backend3/`): Generic backend infrastructure that works with any ISA through traits
- **RISC-V 32-specific code** (`crates/lpc-codegen/src/isa/riscv32/backend3/`): RISC-V 32-specific implementations (MachInst enum, ABI spec)

## Architecture Comparison

### Current Backend (`isa/riscv32/backend/`)

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

**Pipeline** (Cranelift-style):

```
IR Function
  ↓ [Lowering]
VCode<MachInst> (virtual registers)
  ↓ [regalloc2]
regalloc2::Output (allocations + edits)
  ↓ [Emission]
Machine Code
```

**Benefits**:

- Clean separation: lowering → regalloc → emission
- Virtual registers enable better register allocation
- Edits represent allocation decisions explicitly
- Easier to test and debug each phase
- Proper multi-return support from the start

## Compilation Pipeline

### Phase 1: Lowering (IR → VCode)

**Purpose**: Convert LPIR `Function` to `VCode` with virtual registers.

**Input**: `Function` (LPIR)
**Output**: `VCode<MachInst>`

**Key Steps**:

1. Create virtual registers for each IR value
2. Lower each IR instruction to machine instructions
3. Build VCode structure (blocks, instructions, operands)
4. Track operand constraints for regalloc

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

1. Iterate blocks and instructions
2. Apply register allocations to operands
3. Insert edits (moves, spills, reloads) between instructions
4. Generate prologue/epilogue
5. Fix up relocations

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

    /// Constants
    constants: VCodeConstants,
}
```

**Key Features**:

- Machine instructions with `VReg` operands (not physical registers)
- Flat operand array for efficient regalloc2 access
- Block structure preserved from IR
- Branch arguments tracked separately

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

    /// Value to virtual register mapping
    value_to_vreg: BTreeMap<Value, VReg>,

    /// Block to block index mapping
    block_to_index: BTreeMap<Block, BlockIndex>,

    /// ABI information (ISA-specific, provided via MachInst trait)
    abi: Callee<I::ABIMachineSpec>,
}

impl Lower {
    /// Lower a function to VCode
    pub fn lower(mut self) -> VCode<MachInst> {
        // 1. Create virtual registers for all values
        self.create_virtual_registers();

        // 2. Lower each block
        for block in self.func.blocks() {
            self.lower_block(block);
        }

        // 3. Build VCode
        self.vcode.build()
    }

    /// Create virtual registers for all values
    fn create_virtual_registers(&mut self) {
        // Function parameters (block 0 params)
        // Block parameters
        // Instruction results
        // ...
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

- Creates virtual registers for all IR values
- Maps IR instructions to machine instructions
- Handles block parameters (phi-like values)
- Tracks operand constraints

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

### 6. Emission

**File**: `crates/lpc-codegen/src/backend3/emit.rs` (ISA-agnostic, uses ISA-specific MachInst trait)

```rust
/// Emit VCode to machine code
impl VCode<MachInst> {
    pub fn emit(
        self,
        regalloc: &regalloc2::Output,
    ) -> InstBuffer {
        let mut buffer = InstBuffer::new();

        // Generate prologue
        self.gen_prologue(&mut buffer, regalloc);

        // Emit each block
        for block in self.blocks() {
            // Emit block start (if needed)

            // Emit instructions and edits
            for inst_or_edit in regalloc.block_insts_and_edits(&self, block) {
                match inst_or_edit {
                    InstOrEdit::Inst(inst_idx) => {
                        // Apply register allocations to operands
                        let mut inst = self.insts[inst_idx].clone();
                        inst.apply_allocations(&regalloc.allocs[inst_idx]);

                        // Emit instruction
                        buffer.emit(inst.to_physical());
                    }
                    InstOrEdit::Edit(Edit::Move { from, to }) => {
                        // Emit move/spill/reload
                        match (from.as_reg(), to.as_reg()) {
                            (Some(from), Some(to)) => {
                                // Reg-to-reg move
                                buffer.emit(MachInst::Add {
                                    rd: to,
                                    rs1: from,
                                    rs2: zero,
                                });
                            }
                            (Some(from), None) => {
                                // Spill
                                let slot = to.as_stack().unwrap();
                                buffer.emit(self.abi.gen_spill(slot, from));
                            }
                            (None, Some(to)) => {
                                // Reload
                                let slot = from.as_stack().unwrap();
                                buffer.emit(self.abi.gen_reload(to, slot));
                            }
                            _ => unreachable!(),
                        }
                    }
                }
            }
        }

        buffer
    }

    fn gen_prologue(&self, buffer: &mut InstBuffer, regalloc: &regalloc2::Output) {
        // Compute clobbers from regalloc
        let clobbers = self.compute_clobbers(regalloc);

        // Generate frame setup
        // Generate callee-saved register saves
        // ...
    }
}
```

**Key Features**:

- Applies register allocations to instructions
- Inserts edits (moves, spills, reloads)
- Generates prologue/epilogue
- Handles relocations

## Implementation Phases

### Phase 1: Foundation (Week 1)

**Goal**: Basic structure and lowering

1. **Create VCode structure** (ISA-agnostic)

   - `backend3/vcode.rs`: Core VCode type (generic over MachInst)
   - `backend3/vcode_builder.rs`: Builder for constructing VCode
   - Basic block and instruction tracking

2. **Create machine instruction type** (RISC-V 32-specific)

   - `isa/riscv32/backend3/inst.rs`: MachInst enum with VReg operands
   - Implement basic instructions (add, addi, lw, sw)
   - Implement MachInst trait for regalloc2 (operand visitor)

3. **Basic lowering** (ISA-agnostic)
   - `backend3/lower.rs`: Lower simple instructions (iconst, iadd, isub)
   - Create virtual registers for values
   - Build VCode structure
   - Uses MachInst trait (implemented by RISC-V 32 MachInst)

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

1. **Emission implementation** (ISA-agnostic)

   - `backend3/emit.rs`: Apply allocations and emit code
   - Handle edits (moves, spills, reloads)
   - Basic prologue/epilogue
   - Uses MachInst trait for instruction conversion

2. **Instruction conversion** (RISC-V 32-specific)

   - Convert VReg operands to physical registers in MachInst
   - Handle stack slots
   - Implement MachInst methods for emission

3. **End-to-end test**
   - Lower → Regalloc → Emit → Execute

**Deliverable**: Can compile simple functions end-to-end

### Phase 4: Control Flow (Week 4)

**Goal**: Branches and calls

1. **Branch lowering**

   - Lower Jump and Br instructions
   - Handle block parameters
   - Branch target resolution

2. **Call lowering**

   - Lower Call instructions
   - Argument preparation
   - Return value handling

3. **Multi-return support**
   - Return area mechanism
   - Handle >2 return values

**Deliverable**: Can compile functions with branches and calls

### Phase 5: Advanced Features (Week 5+)

**Goal**: Complete feature set

1. **Memory operations**

   - Load/store lowering
   - Stack frame access

2. **Frame layout**

   - Prologue/epilogue generation
   - Callee-saved register handling
   - Stack slot management

3. **Relocations**

   - Function call relocations
   - Branch relocations

4. **Module compilation**
   - Multi-function compilation
   - Function address resolution

**Deliverable**: Complete backend matching current backend features

## File Structure

### ISA-Agnostic Code (Generic Backend Infrastructure)

```
crates/lpc-codegen/src/backend3/
├── mod.rs                 # Main module, compile_function entry point
├── vcode.rs               # VCode structure (generic over MachInst)
├── vcode_builder.rs       # VCode builder
├── lower.rs               # Lowering (IR → VCode, generic)
├── regalloc.rs            # Regalloc2 integration (generic)
├── emit.rs                # Emission (VCode → Machine code, generic)
└── tests/
    ├── mod.rs
    ├── lower_tests.rs     # Lowering tests
    ├── regalloc_tests.rs  # Regalloc tests
    └── emit_tests.rs      # Emission tests
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

**Decision**: Flat operand array with ranges (Cranelift-style).

**Rationale**:

- Efficient access for regalloc2
- Simple to implement
- Matches Cranelift's proven design

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
- Remove old backend (`isa/riscv32/backend/`) or keep as reference
- Update documentation

## Dependencies

### External Crates

- `regalloc2`: Register allocation
- `cranelift-entity`: Entity maps (if needed)

### Internal Dependencies

- `lpc-lpir`: IR types
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

   - Start with layout order, optimize later

2. **Constant handling**: Inline or pool?

   - Start with inline, add pooling later

3. **Debug information**: How to preserve?

   - Add later, focus on correctness first

4. **Error handling**: Panic or Result?
   - Use Result for user-facing APIs
   - Panic for internal invariants

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
3. Implement `VCode` and `VCodeBuilder` (ISA-agnostic)
4. Implement `MachInst` enum with VReg operands (RISC-V 32-specific)
5. Implement `MachInst` trait for regalloc2 integration
6. Implement basic lowering (ISA-agnostic, uses MachInst trait)
7. Implement `Riscv32ABI` (RISC-V 32-specific)
8. Integrate regalloc2 (ISA-agnostic)
9. Implement emission (ISA-agnostic, uses MachInst trait)
10. Test incrementally

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
