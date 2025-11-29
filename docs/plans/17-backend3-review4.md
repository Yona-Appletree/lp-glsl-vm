# Backend3 Plan Review: Comparison with Cranelift Implementation

## Overview

This document reviews the backend3 plan (`17-backend3.md`) against Cranelift's actual implementation to identify missing details or stages.

## Main Stages Comparison

### ✅ Covered Stages

The plan correctly identifies and covers these main stages:

1. **Block Lowering Order Computation** - ✅ Covered
   - Critical edge splitting
   - Reverse postorder computation
   - Cold block identification
   - Indirect branch target tracking

2. **Lowering (IR → VCode)** - ✅ Covered
   - Virtual register creation
   - Instruction lowering
   - Block parameter handling
   - Constant materialization
   - Relocation tracking

3. **Register Allocation** - ✅ Covered
   - regalloc2 integration
   - ABI machine spec configuration

4. **Emission** - ✅ Mostly Covered (see details below)
   - Frame layout computation
   - Prologue/epilogue generation
   - Instruction emission with allocations
   - Edit emission
   - Branch resolution

## Missing or Incomplete Details

### 1. Function Call Tracking

**Cranelift**: Tracks function calls separately (`FunctionCalls` enum) during clobber computation. This affects frame layout (e.g., outgoing argument area size).

**Plan**: Mentions function calls but doesn't explicitly track them as a separate concern during clobber computation.

**Recommendation**: Add explicit function call tracking in `compute_clobbered_callee_saved()` or `compute_frame_layout()`:

```rust
enum FunctionCalls {
    None,
    Regular,  // Has regular function calls
    TailCall, // Has tail calls
}

fn compute_clobbers_and_function_calls(&self, regalloc: &regalloc2::Output) 
    -> (Vec<RealReg>, FunctionCalls) {
    // Track function calls while computing clobbers
    let mut function_calls = FunctionCalls::None;
    // ... compute clobbers ...
    for (inst_idx, _) in self.operand_ranges.iter() {
        function_calls.update(self.insts[inst_idx].call_type());
        // ... rest of clobber computation ...
    }
    (clobbered_regs, function_calls)
}
```

### 2. Constant Registration with Buffer

**Cranelift**: Registers constants with `MachBuffer` before emission starts:

```rust
buffer.register_constants(&self.constants);
```

**Plan**: Doesn't explicitly mention registering constants with the buffer before emission.

**Recommendation**: Add to emission steps:
- **Step 2.5**: Register constants with InstBuffer (if InstBuffer supports constant pools)

### 3. Block Start Instructions

**Cranelift**: Emits `gen_block_start()` instructions for certain blocks (e.g., indirect branch targets, CFI).

**Plan**: Mentions `gen_block_start()` in the MachInst trait but doesn't explicitly show it in the emission loop.

**Recommendation**: Already covered in plan (line 682-687), but could be more prominent in the emission steps.

### 4. Function Alignment

**Cranelift**: Handles function-level alignment (`log2_min_function_alignment`):

```rust
buffer.set_log2_min_function_alignment(self.log2_min_function_alignment);
```

**Plan**: Doesn't mention function-level alignment, only block alignment.

**Recommendation**: Add function alignment handling:
- Set function alignment on InstBuffer before emission
- Emit padding at function start if needed

### 5. Instruction Offset Tracking for Debug Info

**Cranelift**: Tracks instruction offsets for debug value labels:

```rust
let mut inst_offsets = vec![];
// ... during emission ...
inst_offsets[iix.index()] = buffer.cur_offset();
```

**Plan**: Mentions source location tracking but doesn't explicitly mention instruction offset tracking for debug info.

**Recommendation**: Add note that instruction offsets may be tracked for future debug info support (currently deferred).

### 6. Edit Counting Per Block (Optimization)

**Cranelift**: Counts edits per block ahead of time for lookahead island emission:

```rust
let mut ra_edits_per_block: SmallVec<[u32; 64]> = smallvec![];
// Count edits per block
```

**Plan**: Doesn't mention this optimization.

**Recommendation**: This is an optimization detail that can be added later. Not critical for initial implementation.

### 7. Source Location Lifecycle

**Cranelift**: Updates source location tracking during emission:

```rust
let srcloc = self.srclocs[iix.index()];
if cur_srcloc != Some(srcloc) {
    if cur_srcloc.is_some() {
        buffer.end_srcloc();
    }
    buffer.start_srcloc(srcloc);
    cur_srcloc = Some(srcloc);
}
```

**Plan**: Covers source location tracking but could be clearer about the lifecycle (start/end calls).

**Recommendation**: The plan already covers this in Phase 3 (lines 381-424), but could add a note about calling `start_srcloc()`/`end_srcloc()` on the buffer.

### 8. Safepoints and Stack Maps

**Cranelift**: Handles safepoints and stack maps during emission:

```rust
if self.insts[iix.index()].is_safepoint() {
    let user_stack_map = self.user_stack_maps.remove(&index);
    state.pre_safepoint(user_stack_map);
}
```

**Plan**: Correctly defers this (GC not needed initially).

**Status**: ✅ Correctly deferred

### 9. Debug Tags

**Cranelift**: Places debug tags before/after instructions (especially calls):

```rust
let debug_tag_pos = if self.insts[iix.index()].call_type() == CallType::Regular {
    MachDebugTagPos::Post
} else {
    MachDebugTagPos::Pre
};
```

**Plan**: Correctly defers this.

**Status**: ✅ Correctly deferred

### 10. MachBuffer vs InstBuffer Simplification

**Cranelift**: Uses `MachBuffer` with sophisticated features:
- Island insertion for out-of-range branches
- Veneer insertion
- Branch optimization (threading, inversion, etc.)
- Deadline tracking

**Plan**: Uses simplified `InstBuffer` with basic label-based branch resolution.

**Status**: ✅ Acknowledged simplification (line 1222-1230). This is fine for initial implementation.

### 11. Emission State Initialization

**Cranelift**: Creates emission state with ABI and control plane:

```rust
let mut state = I::State::new(&self.abi, std::mem::take(ctrl_plane));
```

**Plan**: Shows `EmitState::new()` but doesn't show what parameters it needs.

**Recommendation**: Clarify that `EmitState` needs ABI information (for SP offset computation, etc.).

### 12. Block Body Emission Order

**Cranelift**: Emits in this order:
1. Prologue (if entry block)
2. Bind label
3. Block alignment
4. Block start instruction
5. Instructions and edits
6. Epilogue (if return instruction)

**Plan**: Shows similar order but could be more explicit about the exact sequence.

**Recommendation**: The plan already covers this (lines 652-726), but could add a numbered list for clarity.

## Detailed Stage-by-Stage Comparison

### Stage 0: Block Lowering Order ✅

**Cranelift**: `BlockLoweringOrder::new()` computes:
- Critical edge splitting
- Reverse postorder
- Cold block identification
- Indirect branch target tracking

**Plan**: ✅ Covers all of these (lines 68-132)

### Stage 1: Lowering ✅

**Cranelift**: `Lower::lower()`:
- Creates virtual registers for all values
- Lowers instructions to MachInst
- Handles block parameters (phi moves)
- Tracks constants
- Records relocations
- Tracks source locations

**Plan**: ✅ Covers all of these (lines 133-574)

### Stage 2: Register Allocation ✅

**Cranelift**: `regalloc2::run()`:
- Implements `regalloc2::Function` trait
- Configures ABI machine spec
- Runs allocation algorithm
- Returns allocations and edits

**Plan**: ✅ Covers all of these (lines 151-164)

### Stage 3: Emission ⚠️ Mostly Covered

**Cranelift**: `VCode::emit()` does:

1. ✅ Reserve labels for blocks
2. ⚠️ Register constants with buffer (missing)
3. ✅ Compute final emission order (cold blocks at end)
4. ✅ Compute clobbers and function calls
5. ✅ Compute frame layout
6. ✅ Create emission state
7. ✅ Count edits per block (optimization, can defer)
8. ✅ For each block:
   - ✅ Call `on_new_block()` hook
   - ✅ Emit block alignment
   - ✅ Emit prologue (if entry)
   - ✅ Bind label
   - ✅ Emit block start instruction
   - ✅ For each instruction/edit:
     - ✅ Track instruction offsets (for debug info)
     - ✅ Update source location
     - ✅ Handle safepoints (deferred)
     - ✅ Place debug tags (deferred)
     - ✅ Emit epilogue (if return)
     - ✅ Apply allocations
     - ✅ Emit instruction
   - ✅ Emit branch (handled by MachBuffer)
9. ✅ Final fixups (handled by MachBuffer)

**Plan**: Covers most steps but missing:
- Constant registration (minor)
- Function call tracking detail (minor)
- Function alignment (minor)

## Recommendations

### Critical Additions

1. **Function Call Tracking**: Add explicit tracking of function calls during clobber computation
2. **Constant Registration**: Register constants with InstBuffer before emission (if supported)

### Nice-to-Have Additions

3. **Function Alignment**: Handle function-level alignment
4. **Emission State Initialization**: Clarify what parameters `EmitState::new()` needs
5. **Block Emission Order**: Add explicit numbered list of emission steps

### Already Correctly Deferred

- Safepoints and stack maps (GC not needed)
- Debug tags (debug info deferred)
- MachBuffer advanced features (islands, veneers, branch optimization)
- Edit counting per block (optimization detail)

## Conclusion

The plan is **comprehensive and covers all main stages** of Cranelift's compilation pipeline. The missing details are mostly minor optimizations or features that are correctly deferred. The plan correctly identifies:

- ✅ All major compilation stages
- ✅ The separation of concerns (lowering → regalloc → emission)
- ✅ Virtual register representation
- ✅ Edit-based register allocation
- ✅ Label-based branch resolution
- ✅ Frame layout computation
- ✅ Prologue/epilogue generation

The plan is **ready for implementation** with the following minor additions recommended:

1. Add function call tracking to clobber computation
2. Add constant registration step before emission
3. Add function alignment handling (if needed)
4. Clarify emission state initialization parameters

Overall, the plan is **well-structured and thorough**, providing a solid foundation for implementing a Cranelift-inspired backend.

