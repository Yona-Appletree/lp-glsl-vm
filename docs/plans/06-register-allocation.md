# Register Allocation Pass (Cranelift-Style)

## Overview

This plan outlines the implementation of a proper register allocation pass that runs **before** instruction lowering, following Cranelift's architecture. The allocator will determine register assignments and insert spill/reload instructions explicitly, then lowering will simply emit code based on those assignments.

## Key Principles (from Cranelift)

1. **Separation of Concerns**: Register allocation is a separate pass from instruction lowering
2. **Modular Design**: Each component in its own module (liveness, regalloc, spill_reload, abi, lower)
3. **Explicit Spill/Reload**: Spill and reload instructions are inserted explicitly in the IR/code
4. **Liveness Analysis**: Track which values are live at each point to make allocation decisions
5. **Call-Site Handling**: Spill live values before calls, reload after
6. **Frame Layout Integration**: Use pre-computed frame layout for spill slot offsets
7. **Correctness First**: Handle all edge cases, no panics for valid inputs

## Current Problems

The old implementation mixed allocation with lowering:

1. **`allocate_or_panic`**: Tried to handle spilling/reloading during lowering
2. **On-the-fly allocation**: Registers allocated during instruction lowering
3. **Borrow checker issues**: Accessing frame layout while mutating allocator state
4. **No explicit spill/reload**: Spills happened implicitly, making code hard to reason about
5. **Monolithic code**: Everything in one or two files

## Architecture

```
┌─────────────────────────────────────────┐
│  Function IR (with values)              │
└─────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  1. Liveness Analysis (liveness.rs)     │
│     - Compute live ranges               │
│     - Build live sets per instruction  │
└─────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  2. Register Allocation (regalloc.rs)  │
│     - Linear scan allocation           │
│     - Build interference graph         │
│     - Assign registers/spill slots     │
└─────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  3. Spill/Reload Insertion             │
│     (spill_reload.rs)                  │
│     - Insert spills after defs         │
│     - Insert reloads before uses       │
│     - Handle call sites                │
└─────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  4. ABI Handling (abi.rs)              │
│     - Map parameters to arg regs       │
│     - Handle return values             │
│     - Compute callee-saved usage       │
└─────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  5. Instruction Lowering (lower.rs)     │
│     - Simple: lookup registers         │
│     - Emit spill/reload ops            │
│     - No allocation decisions           │
└─────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  6. Code Emission (emit.rs)             │
│     - Encode RISC-V instructions      │
│     - Generate prologue/epilogue       │
└─────────────────────────────────────────┘
```

## Module Structure

Following Cranelift's modular approach, we'll split into separate modules:

### 1. `liveness.rs` - Liveness Analysis

**Purpose**: Compute which values are live at each point in the function.

**Key Types**:
```rust
/// Live range for a value (from definition to last use)
pub struct LiveRange {
    pub def: InstPoint,
    pub last_use: InstPoint,
    pub uses: Vec<InstPoint>,
}

/// Instruction point (block index, instruction index)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstPoint {
    pub block: usize,
    pub inst: usize,
}

/// Liveness information for a function
pub struct LivenessInfo {
    /// Live range for each value
    pub live_ranges: BTreeMap<Value, LiveRange>,
    /// Set of live values at each instruction point
    pub live_sets: BTreeMap<InstPoint, BTreeSet<Value>>,
    /// Values defined at each instruction point
    pub defs: BTreeMap<InstPoint, Value>,
    /// Values used at each instruction point
    pub uses: BTreeMap<InstPoint, Vec<Value>>,
}
```

**Key Functions**:
```rust
/// Compute liveness for all values in a function
pub fn compute_liveness(func: &Function) -> LivenessInfo {
    // 1. Forward pass: collect all definitions
    // 2. Backward pass: compute last uses and live ranges
    // 3. Build live sets for each instruction point
    // 4. Handle block parameters (phi-like values)
}
```

**Edge Cases to Handle**:
- Values used before defined (block parameters)
- Values defined but never used
- Values used in multiple blocks
- Values live across calls
- Values used in return statements

**Tests**:
- `test_liveness_simple_sequential` - Sequential values
- `test_liveness_block_parameters` - Block params (phi-like)
- `test_liveness_unused_values` - Values defined but not used
- `test_liveness_multiple_uses` - Values used multiple times
- `test_liveness_across_calls` - Values live across function calls
- `test_liveness_loop` - Values live in loops
- `test_liveness_conditional` - Values in conditional branches

### 2. `regalloc.rs` - Register Allocation Core

**Purpose**: Assign registers to values using linear scan allocation.

**Key Types**:
```rust
/// Register allocation result
pub struct RegisterAllocation {
    /// Value -> Register mapping (for values in registers)
    pub value_to_reg: BTreeMap<Value, Gpr>,
    /// Value -> Spill slot mapping (for spilled values)
    pub value_to_slot: BTreeMap<Value, u32>,
    /// Register -> Value mapping (reverse lookup)
    pub reg_to_value: BTreeMap<Gpr, Value>,
    /// Which callee-saved registers are used
    pub used_callee_saved: Vec<Gpr>,
    /// Number of spill slots needed
    pub spill_slot_count: usize,
}

/// Active interval during linear scan
struct ActiveInterval {
    value: Value,
    reg: Gpr,
    live_range: LiveRange,
}

/// Linear scan register allocator
pub struct LinearScanAllocator {
    /// Available registers (caller-saved first, then callee-saved)
    available_regs: Vec<Gpr>,
    /// Currently active intervals
    active: Vec<ActiveInterval>,
    /// Spill slot counter
    next_spill_slot: u32,
}
```

**Key Functions**:
```rust
/// Allocate registers for a function
pub fn allocate_registers(
    func: &Function,
    liveness: &LivenessInfo,
) -> RegisterAllocation {
    // 1. Sort values by definition point
    // 2. Linear scan: allocate registers, spill when needed
    // 3. Track callee-saved register usage
    // 4. Assign spill slots
}
```

**Allocation Strategy**:
- **Linear Scan Algorithm**:
  1. Sort values by definition point (earliest first)
  2. For each value in order:
     - Expire intervals that end before this value's definition
     - Try to allocate a caller-saved register
     - If none available, try callee-saved register
     - If still none, spill the value with furthest next use
  3. Prefer caller-saved registers (a0-a7, t0-t6)
  4. Use callee-saved registers (s0-s11) when needed
  5. Spill when all registers are in use

**Edge Cases to Handle**:
- All registers in use (must spill)
- Values with very long live ranges
- Values used immediately after definition
- Values only used in return statements
- Block parameters (already in argument registers)
- Function return values (must be in a0-a7)
- Call arguments (must be in a0-a7)
- Values live across multiple calls

**Tests**:
- `test_allocate_simple` - Simple sequential allocation
- `test_allocate_many_values` - More values than registers
- `test_allocate_spill` - Verify spilling works
- `test_allocate_callee_saved` - Callee-saved register usage
- `test_allocate_block_params` - Block parameters
- `test_allocate_call_args` - Function call arguments
- `test_allocate_return_values` - Return values
- `test_allocate_long_live_ranges` - Values with long live ranges
- `test_allocate_interference` - Interfering values get different registers

### 3. `spill_reload.rs` - Spill/Reload Insertion

**Purpose**: Insert explicit spill and reload instructions at the right points.

**Key Types**:
```rust
/// Spill or reload operation
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpillReloadOp {
    /// Spill a value from register to stack
    Spill { value: Value, reg: Gpr, slot: u32 },
    /// Reload a value from stack to register
    Reload { value: Value, reg: Gpr, slot: u32 },
}

/// Spill/reload insertion plan
pub struct SpillReloadPlan {
    /// Operations to insert before each instruction
    pub before: BTreeMap<InstPoint, Vec<SpillReloadOp>>,
    /// Operations to insert after each instruction
    pub after: BTreeMap<InstPoint, Vec<SpillReloadOp>>,
    /// Operations to insert at block boundaries
    pub block_boundary: BTreeMap<usize, Vec<SpillReloadOp>>,
}
```

**Key Functions**:
```rust
/// Create spill/reload plan for a function
pub fn create_spill_reload_plan(
    func: &Function,
    allocation: &RegisterAllocation,
    liveness: &LivenessInfo,
) -> SpillReloadPlan {
    // For each spilled value:
    // 1. Spill after definition (if value is live across calls/blocks)
    // 2. Reload before use (if value is not in register)
    // 3. Handle call sites: spill caller-saved before call, reload after
    // 4. Handle block boundaries: reload spilled values used in block
}
```

**Insertion Strategy**:
1. **After Definition**: Spill immediately if value is spilled and will be used later
2. **Before Use**: Reload before use if value is spilled
3. **Before Call**: Spill all live caller-saved values (they'll be clobbered)
4. **After Call**: Reload spilled values that are still live
5. **Block Boundaries**: Reload spilled values used in successor blocks

**Edge Cases to Handle**:
- Values spilled but never reloaded (dead code)
- Values reloaded multiple times
- Values spilled multiple times
- Call sites with no live values
- Block parameters that are spilled
- Return values that are spilled

**Tests**:
- `test_spill_after_def` - Spill after definition
- `test_reload_before_use` - Reload before use
- `test_call_site_spill_reload` - Call site handling
- `test_block_boundary_reload` - Block boundary handling
- `test_multiple_reloads` - Multiple reloads of same value
- `test_dead_spilled_value` - Spilled value never reloaded
- `test_spilled_return_value` - Spilled return value

### 4. `abi.rs` - ABI Handling

**Purpose**: Handle calling conventions, argument passing, and return values.

**Key Types**:
```rust
/// ABI information for a function
pub struct AbiInfo {
    /// Parameter -> argument register mapping
    pub param_regs: BTreeMap<usize, Gpr>,
    /// Return value -> return register mapping
    pub return_regs: BTreeMap<usize, Gpr>,
    /// Which callee-saved registers are used
    pub used_callee_saved: Vec<Gpr>,
    /// Maximum outgoing arguments (for frame layout)
    pub max_outgoing_args: usize,
}

/// ABI helper functions
pub struct Abi;
```

**Key Functions**:
```rust
impl Abi {
    /// Get argument register for parameter index
    pub fn arg_reg(index: usize) -> Option<Gpr> {
        // a0-a7 for indices 0-7, None for >7 (stack)
    }
    
    /// Get return register for return value index
    pub fn return_reg(index: usize) -> Option<Gpr> {
        // a0-a7 for indices 0-7, None for >7 (stack)
    }
    
    /// Get all caller-saved registers
    pub fn caller_saved_regs() -> Vec<Gpr> {
        // a0-a7, t0-t6, ra
    }
    
    /// Get all callee-saved registers
    pub fn callee_saved_regs() -> Vec<Gpr> {
        // s0-s11
    }
    
    /// Check if register is caller-saved
    pub fn is_caller_saved(reg: Gpr) -> bool;
    
    /// Check if register is callee-saved
    pub fn is_callee_saved(reg: Gpr) -> bool;
    
    /// Compute ABI info for a function
    pub fn compute_abi_info(
        func: &Function,
        allocation: &RegisterAllocation,
    ) -> AbiInfo;
}
```

**Edge Cases to Handle**:
- More than 8 parameters (stack arguments)
- More than 8 return values (stack returns)
- No parameters
- No return values
- Functions that don't call other functions (no RA save needed)

**Tests**:
- `test_arg_regs` - Argument register mapping
- `test_return_regs` - Return register mapping
- `test_many_args` - More than 8 arguments
- `test_many_returns` - More than 8 return values
- `test_caller_saved` - Caller-saved register identification
- `test_callee_saved` - Callee-saved register identification

### 5. `lower.rs` - Instruction Lowering

**Purpose**: Lower IR instructions to RISC-V instructions using pre-computed allocation.

**Key Types**:
```rust
/// Lowerer that uses pre-computed allocation
pub struct Lowerer {
    /// Module context (for function calls)
    module: Option<Module>,
    /// Function addresses (for relocations)
    function_addresses: BTreeMap<String, u32>,
    /// Relocations to fix up
    relocations: Vec<Relocation>,
}

/// Relocation for function calls
pub struct Relocation {
    pub offset: usize,
    pub callee: String,
}
```

**Key Functions**:
```rust
impl Lowerer {
    /// Lower a function to RISC-V code
    pub fn lower_function(
        &mut self,
        func: &Function,
        allocation: &RegisterAllocation,
        spill_reload: &SpillReloadPlan,
        frame_layout: &FrameLayout,
    ) -> CodeBuffer {
        // 1. Generate prologue
        // 2. For each block:
        //    - Lower block parameters (if any)
        //    - For each instruction:
        //      - Emit spill/reload ops before instruction
        //      - Lower instruction (lookup registers)
        //      - Emit spill/reload ops after instruction
        // 3. Generate epilogue
    }
    
    /// Lower a single instruction
    fn lower_inst(
        &mut self,
        code: &mut CodeBuffer,
        inst: &Inst,
        allocation: &RegisterAllocation,
        frame_layout: &FrameLayout,
    );
    
    /// Lower iadd instruction
    fn lower_iadd(
        &mut self,
        code: &mut CodeBuffer,
        result: Value,
        arg1: Value,
        arg2: Value,
        allocation: &RegisterAllocation,
    );
    
    /// Lower call instruction
    fn lower_call(
        &mut self,
        code: &mut CodeBuffer,
        callee: &str,
        args: &[Value],
        results: &[Value],
        allocation: &RegisterAllocation,
        spill_reload: &SpillReloadPlan,
        frame_layout: &FrameLayout,
    );
    
    // ... other instruction lowerers
}
```

**Simplified Lowering**:
- No allocation decisions - just lookup from `allocation`
- No on-the-fly spilling - use `spill_reload` plan
- No frame layout access during allocation - already computed

**Edge Cases to Handle**:
- Instructions with spilled operands
- Instructions with spilled results
- Call instructions with spilled arguments
- Return instructions with spilled values
- Block parameters that are spilled
- Large constants (require lui + addi)
- Jump targets (PC-relative offsets)

**Tests**:
- `test_lower_simple_add` - Simple addition
- `test_lower_with_spills` - Instructions with spills
- `test_lower_call` - Function calls
- `test_lower_call_with_spills` - Calls with spilled args
- `test_lower_return` - Return instructions
- `test_lower_large_const` - Large constants
- `test_lower_jump` - Jump instructions
- `test_lower_block_params` - Block parameters

### 6. `emit.rs` - Code Emission (Already Exists)

**Purpose**: Encode RISC-V instructions and generate prologue/epilogue.

**Note**: This module already exists and is mostly correct. May need minor updates for:
- Spill/reload instruction encoding
- Prologue/epilogue generation with correct frame layout

## Implementation Plan

### Phase 1: Liveness Analysis (TDD)

**Files**: `crates/r5-target-riscv32/src/liveness.rs`

**Step 1.1**: Create module structure and types
- Define `LiveRange`, `InstPoint`, `LivenessInfo`
- Add module to `lib.rs`

**Step 1.2**: Implement basic liveness computation
- Forward pass: collect definitions
- Backward pass: compute last uses
- Build live ranges

**Step 1.3**: Add tests
- `test_liveness_simple_sequential`
- `test_liveness_block_parameters`
- `test_liveness_unused_values`

**Step 1.4**: Handle edge cases
- Values used before defined
- Values defined but never used
- Values live across calls

**Step 1.5**: Add more tests
- `test_liveness_multiple_uses`
- `test_liveness_across_calls`
- `test_liveness_loop`
- `test_liveness_conditional`

### Phase 2: Register Allocation (TDD)

**Files**: `crates/r5-target-riscv32/src/regalloc.rs`

**Step 2.1**: Create module structure and types
- Define `RegisterAllocation`, `LinearScanAllocator`
- Implement register ordering (caller-saved first)

**Step 2.2**: Implement basic linear scan
- Sort values by definition point
- Allocate registers sequentially
- Track active intervals

**Step 2.3**: Add tests
- `test_allocate_simple`
- `test_allocate_many_values`
- `test_allocate_spill`

**Step 2.4**: Handle spilling
- Implement spill decision (furthest next use)
- Assign spill slots
- Track callee-saved usage

**Step 2.5**: Add more tests
- `test_allocate_callee_saved`
- `test_allocate_block_params`
- `test_allocate_call_args`
- `test_allocate_return_values`
- `test_allocate_long_live_ranges`
- `test_allocate_interference`

### Phase 3: Spill/Reload Insertion (TDD)

**Files**: `crates/r5-target-riscv32/src/spill_reload.rs`

**Step 3.1**: Create module structure and types
- Define `SpillReloadOp`, `SpillReloadPlan`
- Add module to `lib.rs`

**Step 3.2**: Implement basic spill/reload insertion
- Spill after definition
- Reload before use

**Step 3.3**: Add tests
- `test_spill_after_def`
- `test_reload_before_use`

**Step 3.4**: Handle call sites
- Spill caller-saved before calls
- Reload after calls

**Step 3.5**: Handle block boundaries
- Reload spilled values at block entry

**Step 3.6**: Add more tests
- `test_call_site_spill_reload`
- `test_block_boundary_reload`
- `test_multiple_reloads`
- `test_dead_spilled_value`
- `test_spilled_return_value`

### Phase 4: ABI Handling (TDD)

**Files**: `crates/r5-target-riscv32/src/abi.rs`

**Step 4.1**: Create module structure
- Define `AbiInfo`, `Abi`
- Implement argument/return register mapping

**Step 4.2**: Add tests
- `test_arg_regs`
- `test_return_regs`
- `test_many_args`
- `test_many_returns`
- `test_caller_saved`
- `test_callee_saved`

### Phase 5: Instruction Lowering (TDD)

**Files**: `crates/r5-target-riscv32/src/lower.rs`

**Step 5.1**: Refactor existing lowering code
- Remove `allocate_or_panic`
- Remove on-the-fly allocation
- Add allocation/spill_reload parameters

**Step 5.2**: Simplify instruction lowerers
- Lookup registers from allocation
- Emit spill/reload ops from plan
- Handle edge cases

**Step 5.3**: Add tests
- `test_lower_simple_add`
- `test_lower_with_spills`
- `test_lower_call`
- `test_lower_call_with_spills`
- `test_lower_return`
- `test_lower_large_const`
- `test_lower_jump`
- `test_lower_block_params`

### Phase 6: Integration (TDD)

**Step 6.1**: Update `lower_function` in `lib.rs`
- Call liveness analysis
- Call register allocation
- Call spill/reload insertion
- Call ABI computation
- Call instruction lowering

**Step 6.2**: End-to-end tests
- `test_integration_simple`
- `test_integration_with_spills`
- `test_integration_with_calls`
- `test_integration_complex`

**Step 6.3**: Verify existing tests pass
- Run all existing tests
- Fix any regressions

### Phase 7: Cleanup and Documentation

**Step 7.1**: Remove old code
- Delete `allocate_or_panic`
- Delete `allocate_or_spill`
- Clean up unused code

**Step 7.2**: Add documentation
- Module-level docs
- Function docs
- Example usage

**Step 7.3**: Performance optimization
- Profile allocation
- Optimize hot paths
- Add benchmarks

## Success Criteria

- ✅ Register allocation is a separate pass before lowering
- ✅ Code is split into multiple focused modules (liveness, regalloc, spill_reload, abi, lower)
- ✅ Spill/reload instructions are explicit and visible
- ✅ No on-the-fly allocation decisions during lowering
- ✅ Borrow checker issues resolved
- ✅ All edge cases handled (no panics for valid inputs)
- ✅ Comprehensive test coverage (>90%)
- ✅ All existing tests pass
- ✅ Code is simpler and easier to reason about
- ✅ Aligns with Cranelift's architecture

## References

- Cranelift RISC-V code: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/inst/`
- Cranelift machinst: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/machinst/`
- Linear scan allocation algorithm (Poletto & Sarkar)
- Liveness analysis algorithms (Aho, Sethi, Ullman)

