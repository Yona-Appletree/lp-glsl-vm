# Fix Lazy Dominance IDom Computation

## Problem

The `find_dominators` function incorrectly computes immediate dominators by using `pred.idom` instead of the predecessor block itself. This causes blocks like Block(5) to have no IDom, requiring a hack fallback that picks any definition block.

## Root Cause Analysis

### Current Buggy Implementation

In `crates/lpc-glsl/src/codegen/ssa.rs`, the `find_dominators` function (lines 387-425) has this bug:

```rust
// WRONG: Uses pred.idom which may be None
if new_idom.is_none() {
    new_idom = pred.idom;  // If pred.idom is None, new_idom stays None!
} else if let (Some(idom1), Some(idom2)) = (new_idom, pred.idom) {
    new_idom = Some(self.intersect_dominators(idom1, idom2, block_list));
}
// If pred.idom is None, we skip the predecessor entirely!
```

### LLVM Reference Implementation

According to LLVM's `SSAUpdaterImpl.h` (lines 251-254), it uses the predecessor block itself:

```cpp
if (!NewIDom)
  NewIDom = Pred;  // Use predecessor BBInfo* directly
else
  NewIDom = IntersectDominators(NewIDom, Pred);  // Intersect with predecessor
```

The key insight: LLVM uses `Pred` (a `BBInfo*` pointer) directly, not `Pred->IDom`. The `IntersectDominators` function then walks up the dominator tree by following `IDom` pointers.

### Why This Matters

In Cooper-Harvey-Kennedy algorithm:

- IDom(B) = intersection of all predecessors' IDoms
- But if a predecessor doesn't have an IDom yet (first iteration), we use the predecessor itself
- The iterative algorithm converges: on later iterations, predecessors will have IDoms, and we intersect those

Our bug: We skip predecessors that don't have IDoms yet, so blocks never get IDoms assigned.

## Solution

### 1. Fix `find_dominators` to match LLVM exactly

**File**: `crates/lpc-glsl/src/codegen/ssa.rs` (lines 387-425)

**Key Change**: Use predecessor block itself, not `pred.idom`:

```rust
// CORRECT: Use predecessor block itself
if new_idom.is_none() {
    new_idom = Some(pred.block);  // Start with predecessor block
} else {
    // Intersect with predecessor: use pred.block as the candidate
    // IntersectDominators will walk up the dominator tree
    new_idom = Some(self.intersect_dominators(
        new_idom.unwrap(),
        pred.block,  // Use pred.block, not pred.idom!
        block_list
    ));
}
```

**Important**: The `intersect_dominators` function already handles walking up the IDom chain, so we just need to pass `pred.block` and it will follow `pred.idom` internally.

### 2. Verify `intersect_dominators` handles missing IDoms correctly

**File**: `crates/lpc-glsl/src/codegen/ssa.rs` (lines 427-464)

The current implementation should work, but verify:

- When a block has no IDom (`info.idom == None`), it returns the other block
- This matches LLVM's behavior: `if (!Blk1) return Blk2;`

### 3. Handle predecessor not in block_list

**File**: `crates/lpc-glsl/src/codegen/ssa.rs` (line 403)

Currently we skip predecessors not in `block_list`. According to LLVM, this shouldn't happen (all predecessors should be in BBMap), but we should add an assertion or handle gracefully.

### 4. Remove the hack fallback

**File**: `crates/lpc-glsl/src/codegen/ssa.rs` (lines 617-629)

Remove the fallback code that picks any definition block. This should no longer be needed once IDom computation is correct.

### 5. Add comprehensive failing tests

**File**: `crates/lpc-glsl/src/codegen/ssa_tests.rs`

Tests have been added that will fail with the current buggy implementation and pass after the fix. These tests verify IDom computation indirectly by checking that values are correctly propagated:

1. **`test_idom_diamond`**: Tests diamond pattern where Block(3) should get value from Block(0) via IDom
2. **`test_idom_loop_backedge`**: Tests loop back-edge case (the actual bug) where Block(5) should get value from Block(3) via IDom
3. **`test_idom_multiple_predecessors`**: Tests multiple predecessors where Block(3) should get value via PHI from Block(1) and Block(2)
4. **`test_idom_linear_chain`**: Tests simple linear chain where Block(2) should get value from Block(0) via IDom chain

These tests will fail with the current implementation because blocks won't have IDoms assigned, causing `get_value_at_end_of_block` to return `None` or incorrect values.

## Implementation Steps

1. **Fix `find_dominators`**:

   - Change line 411: `new_idom = pred.idom;` → `new_idom = Some(pred.block);`
   - Change line 413: Use `pred.block` instead of `pred.idom` in intersection
   - Handle case where `pred` is not in `block_list` (shouldn't happen, but add check)

2. **Verify `intersect_dominators`**:

   - Ensure it correctly handles `None` IDoms (returns other block)
   - Test that it walks up dominator tree correctly

3. **Add failing tests**:

   - Write tests that verify IDom computation
   - These should fail with current implementation
   - Should pass after fix

4. **Remove hack**:

   - Remove fallback code in `find_available_values`
   - Verify tests still pass

5. **Verify integration**:
   - Run `test_nested_control_flow` - should pass
   - Run all SSA tests - should all pass
   - Verify no regressions

## Success Criteria

1. All blocks in `block_list` have proper IDoms assigned (no `None` IDoms except pseudo-entry)
2. `test_nested_control_flow` passes without LPIR validation errors
3. All new IDom tests pass
4. All existing SSA tests pass
5. No hack fallback code remains
6. Code matches LLVM's SSAUpdater algorithm exactly

## Reference

- LLVM SSAUpdaterImpl.h lines 228-264: `FindDominators` implementation
- LLVM SSAUpdaterImpl.h lines 202-216: `IntersectDominators` implementation
- Cooper-Harvey-Kennedy algorithm: "A Simple, Fast Dominance Algorithm" (2001)
