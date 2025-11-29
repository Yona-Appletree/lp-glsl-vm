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

**Key Steps**: Critical edge detection, edge block creation, reverse postorder computation, cold block identification, indirect branch target tracking.

**See**: [Phase 1](17-backend3-1-foundation.md) for complete implementation details.

### Phase 1: Lowering (IR → VCode)

**Purpose**: Convert LPIR `Function` to `VCode` with virtual registers.

**Input**: `Function` (LPIR)
**Output**: `VCode<MachInst>`

**Key Steps**: Use block lowering order, handle edge blocks (phi moves), create virtual registers, lower instructions, build VCode structure, track operand constraints, handle constants, record relocations.

**See**: [Phase 1](17-backend3-1-foundation.md) for complete implementation details.

### Phase 2: Register Allocation

**Purpose**: Assign physical registers or spill slots to virtual registers.

**Input**: `VCode<MachInst>`
**Output**: `regalloc2::Output`

**Key Steps**: Implement `regalloc2::Function` trait for VCode, configure ABI machine spec, run regalloc2 algorithm, get allocations and edits.

**See**: [Phase 2](17-backend3-2-regalloc2-integration.md) for complete implementation details.

### Phase 3: Emission

**Purpose**: Apply register allocations and generate final machine code.

**Input**: `VCode<MachInst>` + `regalloc2::Output`
**Output**: `InstBuffer` (machine code)

**Architecture**: Streaming emission with label-based branch resolution (inspired by Cranelift's MachBuffer).

**Key Steps**: Reserve labels, register constants, compute emission order, compute clobbers and function calls, compute frame layout, initialize emission state, emit blocks in order (prologue, instructions, edits, branches, epilogue), resolve branches, fix external relocations.

**See**: [Phase 3](17-backend3-3-emission.md) for complete implementation details including emission state tracking, frame layout computation, prologue/epilogue generation, branch resolution, and edit emission.

## Key Components

### 1. VCode Structure

**File**: `crates/lpc-codegen/src/backend3/vcode.rs` (ISA-agnostic)

**Purpose**: Virtual-register code container with machine instructions, operands, block structure, and metadata.

**See**: [Phase 1](17-backend3-1-foundation.md) for complete structure definition and field details.

### 2. Machine Instruction Type

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/inst.rs` (RISC-V 32-specific)

**Purpose**: RISC-V 32-bit machine instructions with virtual register operands, implementing the MachInst trait for regalloc2.

**See**: [Phase 1](17-backend3-1-foundation.md) for complete implementation details.

### 3. Lowering

**File**: `crates/lpc-codegen/src/backend3/lower.rs` (ISA-agnostic, uses ISA-specific MachInst trait)

**Purpose**: Convert LPIR Function to VCode with virtual registers, handling block ordering, edge blocks, and instruction lowering.

**See**: [Phase 1](17-backend3-1-foundation.md) for complete implementation details.

### 4. Regalloc2 Integration

**File**: `crates/lpc-codegen/src/backend3/regalloc.rs` (ISA-agnostic)

**Purpose**: Implement `regalloc2::Function` trait for VCode to enable register allocation.

**See**: [Phase 2](17-backend3-2-regalloc2-integration.md) for complete implementation details.

### 5. ABI Machine Spec

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/abi.rs` (RISC-V 32-specific)

**Purpose**: RISC-V 32-bit ABI machine specification for regalloc2, defining register classes, callee-saved/caller-saved registers, and frame layout.

**See**: [Phase 2](17-backend3-2-regalloc2-integration.md) for complete implementation details.

### 6. Emission

**File**: `crates/lpc-codegen/src/backend3/emit.rs` (ISA-agnostic, uses ISA-specific MachInst trait)

**Purpose**: Apply register allocations and generate final machine code using streaming emission with label-based branch resolution.

**See**: [Phase 3](17-backend3-3-emission.md) for complete implementation details.

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

**See**: [Phase 1](17-backend3-1-foundation.md) for complete implementation details.

### 8. Constant Handling

**File**: `crates/lpc-codegen/src/backend3/constants.rs` (ISA-agnostic)

**Purpose**: Handle constant materialization and storage (inline constants, LUI+ADDI sequences, constant pool).

**See**: [Phase 1](17-backend3-1-foundation.md) for complete implementation details including decision criteria, special cases, and implementation notes.

### 9. Relocation Handling

**File**: `crates/lpc-codegen/src/backend3/reloc.rs` (ISA-agnostic)

**Purpose**: Track and resolve relocations (function calls, etc.) during lowering and emission.

**See**: [Phase 1](17-backend3-1-foundation.md) and [Phase 3](17-backend3-3-emission.md) for complete implementation details.

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
