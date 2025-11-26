# Register Allocation Pass (Cranelift-Style)

## Overview

This plan outlines the implementation of a proper register allocation pass that runs **before** instruction lowering, following Cranelift's architecture. The allocator will determine register assignments and insert spill/reload instructions explicitly, then lowering will simply emit code based on those assignments.

## Key Principles (from Cranelift)

1. **Separation of Concerns**: Register allocation is a separate pass from instruction lowering
2. **Explicit Spill/Reload**: Spill and reload instructions are inserted explicitly in the IR/code
3. **Liveness Analysis**: Track which values are live at each point to make allocation decisions
4. **Call-Site Handling**: Spill live values before calls, reload after
5. **Frame Layout Integration**: Use pre-computed frame layout for spill slot offsets

## Current Problems

The current implementation mixes allocation with lowering:

1. **`allocate_or_panic`**: Tries to handle spilling/reloading during lowering
2. **On-the-fly allocation**: Registers allocated during instruction lowering
3. **Borrow checker issues**: Accessing frame layout while mutating allocator state
4. **No explicit spill/reload**: Spills happen implicitly, making code hard to reason about

## Architecture

```
┌─────────────────────────────────────────┐
│  Function IR (with values)              │
└─────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  Register Allocation Pass                │
│  - Liveness analysis                    │
│  - Register assignment                  │
│  - Spill/reload insertion               │
└─────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  Allocated Function IR                  │
│  - Values mapped to registers/spills    │
│  - Explicit spill/reload instructions   │
└─────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  Instruction Lowering                   │
│  - Simple: emit code based on mapping  │
│  - No allocation decisions              │
└─────────────────────────────────────────┘
```

## Phase 1: Liveness Analysis

### Goal
Determine which values are live at each instruction point.

### Implementation

**File:** `crates/r5-target-riscv32/src/regalloc.rs`

Add liveness analysis:

```rust
/// Compute liveness for all values in a function
pub fn compute_liveness(func: &Function) -> LivenessInfo {
    // For each value, track:
    // - Definition point (where it's created)
    // - Use points (where it's used)
    // - Live range (from definition to last use)
}
```

**Algorithm:**
1. Forward pass: Track definitions
2. Backward pass: Track uses and compute live ranges
3. Build live sets for each instruction

## Phase 2: Register Assignment

### Goal
Assign registers to values, spilling when necessary.

### Implementation

**File:** `crates/r5-target-riscv32/src/regalloc.rs`

Extend `SimpleRegAllocator`:

```rust
pub struct RegisterAllocation {
    /// Value -> Register mapping
    value_to_reg: BTreeMap<Value, Gpr>,
    /// Values that need to be spilled
    spilled_values: BTreeSet<Value>,
    /// Spill slot assignments
    spill_slots: BTreeMap<Value, u32>,
    /// Live ranges for each value
    live_ranges: BTreeMap<Value, LiveRange>,
}

pub fn allocate_registers(
    func: &Function,
    frame_layout: &FrameLayout,
) -> RegisterAllocation {
    // 1. Compute liveness
    // 2. Build interference graph (values that can't share registers)
    // 3. Allocate registers (graph coloring or linear scan)
    // 4. Determine spill candidates
    // 5. Assign spill slots
}
```

**Allocation Strategy:**
- Linear scan allocation (simpler than graph coloring)
- Track register pressure
- Spill when pressure exceeds available registers
- Prefer caller-saved registers, use callee-saved when needed

## Phase 3: Spill/Reload Insertion

### Goal
Insert explicit spill and reload instructions at the right points.

### Implementation

**File:** `crates/r5-target-riscv32/src/regalloc.rs`

```rust
pub struct SpillReloadInsertion {
    /// Instructions to insert before each instruction
    before: BTreeMap<usize, Vec<SpillReloadInst>>,
    /// Instructions to insert after each instruction
    after: BTreeMap<usize, Vec<SpillReloadInst>>,
}

pub enum SpillReloadInst {
    Spill { value: Value, slot: u32 },
    Reload { value: Value, slot: u32 },
}

pub fn insert_spill_reload(
    func: &Function,
    allocation: &RegisterAllocation,
    frame_layout: &FrameLayout,
) -> SpillReloadInsertion {
    // For each spilled value:
    // - Insert spill after definition (if value is live across calls/blocks)
    // - Insert reload before use (if value is not in register)
    // - Handle call sites: spill before call, reload after
}
```

**Insertion Points:**
1. **After definition**: If value is spilled and will be used later
2. **Before use**: If value is spilled and needs to be in register
3. **Before call**: Spill all live caller-saved values
4. **After call**: Reload spilled values that are still live

## Phase 4: Integration with Lowering

### Goal
Modify lowering to use pre-computed allocation.

### Implementation

**File:** `crates/r5-target-riscv32/src/lower.rs`

```rust
pub fn lower_function(&mut self, func: &Function) -> CodeBuffer {
    // 1. Compute frame layout (existing)
    let frame_layout = FrameLayout::compute(...);
    
    // 2. Register allocation pass (NEW)
    let allocation = allocate_registers(func, &frame_layout);
    let spill_reload = insert_spill_reload(func, &allocation, &frame_layout);
    
    // 3. Lower instructions (SIMPLIFIED)
    // - Just look up register/spill slot from allocation
    // - Emit spill/reload instructions from spill_reload
    // - No on-the-fly allocation decisions
}
```

**Simplified Lowering:**
- `lower_iadd`: Look up registers from allocation, emit `add`
- `lower_call`: Use allocation to find which values to spill/reload
- No `allocate_or_panic` - just use pre-computed mapping

## Implementation Plan

### Step 1: Liveness Analysis (TDD)

**Test:** `test_liveness_analysis`
- Simple function with sequential values
- Verify live ranges are computed correctly
- Verify live sets at each instruction

**Implementation:**
- Add `compute_liveness()` function
- Track definitions and uses
- Build live ranges

### Step 2: Register Assignment (TDD)

**Test:** `test_register_assignment`
- Function with many values
- Verify registers are assigned
- Verify spill decisions are made

**Implementation:**
- Linear scan allocation algorithm
- Track register pressure
- Make spill decisions

### Step 3: Spill/Reload Insertion (TDD)

**Test:** `test_spill_reload_insertion`
- Function with spilled values
- Verify spills inserted after definitions
- Verify reloads inserted before uses
- Verify call-site handling

**Implementation:**
- Insert spill/reload instructions
- Handle call sites correctly

### Step 4: Integration (TDD)

**Test:** `test_integration`
- End-to-end test with allocation + lowering
- Verify correct code generation
- Verify spill/reload instructions are correct

**Implementation:**
- Modify `lower_function()` to use allocation pass
- Simplify instruction lowering
- Remove `allocate_or_panic` complexity

### Step 5: Cleanup

- Remove `allocate_or_spill` from `analyze_function`
- Remove `allocate_or_panic` complexity
- Remove on-the-fly spilling from `lower_call`
- Keep frame layout computation (it's correct)

## Success Criteria

- Register allocation is a separate pass before lowering
- Spill/reload instructions are explicit and visible
- No on-the-fly allocation decisions during lowering
- Borrow checker issues resolved
- All existing tests pass
- Code is simpler and easier to reason about

## References

- Cranelift register allocation: `wasmtime/cranelift/codegen/src/regalloc/`
- Linear scan allocation algorithm
- Liveness analysis algorithms

