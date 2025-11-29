# Fix SSA Return Block Dominance Issue

## Problem Analysis

The error "Value 7 defined in block3 is passed to block1 which is not dominated by block3" occurs when:

1. A return statement is inside a loop body (e.g., `if (i == 5) return i;` in a for loop)
2. The return block (block3) becomes a predecessor of a merge block
3. When collecting `updated_values` for loop phi nodes, `get_ssa_value()` includes the return block in the backwards-reachable set
4. The return block's value is then passed to the loop header's phi node, violating dominance

**Root Cause**: In `build_block_list()` in `crates/lpc-glsl/src/codegen/ssa.rs`, we include ALL predecessors when building backwards-reachable sets, including return/halt blocks that don't dominate the target block.

**LLVM Reference**: In `SSAUpdaterImpl.h`, unreachable predecessors (BlkNum == 0) are treated as definitions with 'undef' values (lines 242-249). However, LLVM's approach differs - they mark unreachable blocks during forward traversal, not filter them during backwards traversal.

## Investigation Plan

### Step 1: Create Minimal Test Case

- **File**: `crates/lpc-glsl/tests/ssa_return_block_tests.rs` (new file)
- Create a minimal test that reproduces the issue:

  ```rust
  fn test_for_loop_with_return_inside_if() {
      // for (int i = 0; i < 10; i++) {
      //     if (i == 5) return i;  // Return block becomes predecessor
      // }
      // return 0;
  }
  ```

- Verify it fails with the dominance violation error
- Add test to print the generated LPIR to understand block structure

### Step 2: Trace the Issue

- **File**: `crates/lpc-glsl/src/codegen/ssa.rs`
- Add debug logging in `build_block_list()` (line 234-270):
  - Log each predecessor added to the worklist
  - Mark which predecessors are terminated blocks (return/halt)
  - Track which blocks end up in the final `block_list`
- Add debug logging in `find_available_values()` (line 699-751):
  - Log which predecessors are being considered for PHI blocks
  - Mark which predecessors are terminated blocks
  - Show which values are being collected

### Step 3: Understand LLVM's Approach

- **Reference**: `/Users/yona/dev/photomancer/DirectXShaderCompiler/include/llvm/Transforms/Utils/SSAUpdaterImpl.h`
- Study `BuildBlockList()` (lines 98-196):
  - How does LLVM handle blocks that don't reach the target?
  - What is the `BlkNum == 0` mechanism for unreachable blocks?
- Study `FindDominators()` (lines 228-264):
  - How are unreachable predecessors handled (lines 242-249)?
  - They're treated as definitions with 'undef', not filtered out
- Study `FindAvailableVals()` (lines 319-374):
  - How are PHI operands collected from predecessors?
  - Are terminated blocks handled specially?

## Fix Strategy: LLVM-Style Approach

**Decision**: Implement exactly like LLVM to stay aligned with the reference implementation.

### LLVM's Approach (from SSAUpdaterImpl.h lines 242-249)

1. During forward traversal in `BuildBlockList()`, blocks that aren't visited get `BlkNum == 0`
2. In `FindDominators()`, when a predecessor has `BlkNum == 0`, it's treated as unreachable:
   - Assigns an undef value via `Traits::GetUndefVal()`
   - Sets `DefBB = Pred` (treats it as a definition)
   - Assigns `BlkNum = PseudoEntry->BlkNum` and increments
3. These unreachable predecessors are then used in IDom computation

### Our Implementation

- **File**: `crates/lpc-glsl/src/codegen/ssa.rs`
- In `find_dominators()`, detect terminated blocks with `postorder_num == 0` (unreachable from forward traversal)
- Treat them as unreachable definitions with undef values (matching LLVM exactly)
- Assign postorder numbers from pseudo-entry
- Include them in IDom computation

**Rationale**:

- Matches LLVM's proven approach exactly
- Handles the case where return blocks are predecessors but unreachable from forward traversal
- Keeps code aligned with reference implementation for easier maintenance

## Implementation Plan

### Phase 1: Add Helper Function

- **File**: `crates/lpc-glsl/src/codegen/ssa.rs`
- Add function `is_terminated_block(block: BlockEntity, function: &Function) -> bool`:
  ```rust
  fn is_terminated_block(&self, block: BlockEntity, function: &Function) -> bool {
      let insts: Vec<_> = function.block_insts(block).collect();
      if let Some(last_inst) = insts.last() {
          if let Some(inst_data) = function.dfg.inst_data(*last_inst) {
              matches!(inst_data.opcode, Opcode::Return | Opcode::Halt)
          } else {
              false
          }
      } else {
          false
      }
  }
  ```

### Phase 2: Create Undef Value Mechanism

- **File**: `crates/lpc-glsl/src/codegen/ssa.rs`
- Add a constant for undef sentinel value:
  ```rust
  // Sentinel value to represent undef (unreachable definitions)
  // Using u32::MAX as sentinel since it's unlikely to conflict with real values
  const UNDEF_VALUE: Value = Value::new(u32::MAX);
  ```
- Or add a helper function to create/get undef values if needed

### Phase 3: Modify `find_dominators()` to Handle Unreachable Terminated Blocks

- **File**: `crates/lpc-glsl/src/codegen/ssa.rs`, `find_dominators()` function (around line 457)
- Replace the current skip logic with LLVM-style handling:

  ```rust
  // Track unreachable blocks to assign postorder numbers
  let mut unreachable_count = 0u32;

  // In the predecessor loop:
  if pred.postorder_num == 0 {
      // Check if this is a terminated block (unreachable from forward traversal)
      if self.is_terminated_block(pred.block, function) {
          // Treat as unreachable definition with undef (per LLVM lines 242-249)
          block_list[pred_idx].available_val = Some(UNDEF_VALUE);
          block_list[pred_idx].def_block = Some(pred.block);
          block_list[pred_idx].postorder_num = pseudo_entry.postorder_num + unreachable_count;
          unreachable_count += 1;
          // Now use it in IDom computation (continue to line 465)
      } else {
          // Not terminated - truly unreachable, skip
          continue;
      }
  }
  ```

- This matches LLVM's approach: unreachable terminated blocks are treated as definitions with undef values

### Phase 4: Handle Undef Values in `find_available_values()`

- **File**: `crates/lpc-glsl/src/codegen/ssa.rs`, `find_available_values()` function
- When collecting values for PHI blocks, handle undef values appropriately:
  - Undef values from unreachable terminated blocks should be included in PHI operands
  - This matches LLVM's behavior where unreachable predecessors contribute undef to PHIs
- Note: The actual PHI creation in control flow codegen will need to handle undef values
  - For now, we can pass the undef sentinel value and let the validator catch issues
  - Or we can filter them out if they cause problems (but this deviates from LLVM)

### Phase 5: Add Tests

- **File**: `crates/lpc-glsl/tests/ssa_return_block_tests.rs`
- Test cases:

  1. `test_for_loop_with_return_inside_if` - Original failing case
  2. `test_while_loop_with_return_inside_if` - Similar case for while loops
  3. `test_nested_loops_with_return` - Return in nested loop
  4. `test_return_block_defines_variable` - Edge case: return block that defines variable (should still be included)
  5. `test_unreachable_return_block_handling` - Verify unreachable return blocks are handled correctly

### Phase 6: Verify Fix

- Run all existing tests to ensure no regressions
- Run the new test cases to verify they pass
- Run the failing edge case tests:
  - `test_for_loop_with_return`
  - `test_while_loop_with_return`
  - `test_unreachable_code_after_return`
  - `test_if_without_else_no_return`

## Success Criteria

1. All new test cases pass
2. All existing tests continue to pass
3. The dominance violation error is resolved
4. Implementation matches LLVM's SSAUpdater approach for unreachable terminated blocks
5. No performance regression (backwards traversal should be faster with fewer blocks)

## Implementation Notes

### Undef Value Representation

- Using `Value::new(u32::MAX)` as sentinel for undef values
- This is a simple approach that works for our use case
- If we need proper undef values later, we can add an `undef` instruction/opcode

### Postorder Number Assignment

- Unreachable terminated blocks get postorder numbers starting from `pseudo_entry.postorder_num + 1`
- This ensures they're ordered after the pseudo-entry but before regular blocks
- Matches LLVM's approach of incrementing `PseudoEntry->BlkNum`

### Edge Cases

- Return blocks that define variables: These are roots and will have `postorder_num != 0`, so they're handled normally
- Truly unreachable blocks (not terminated): These are skipped (not treated as definitions)
- Only terminated blocks with `postorder_num == 0` are treated as unreachable definitions

## Files to Modify

1. `crates/lpc-glsl/src/codegen/ssa.rs` - Add filtering logic
2. `crates/lpc-glsl/tests/ssa_return_block_tests.rs` - New test file
3. `crates/lpc-glsl/tests/edge_case_tests.rs` - Should pass after fix

## Reference Files

- LLVM Implementation: `/Users/yona/dev/photomancer/DirectXShaderCompiler/include/llvm/Transforms/Utils/SSAUpdaterImpl.h`
- Current Implementation: `crates/lpc-glsl/src/codegen/ssa.rs`
- Failing Test: `crates/lpc-glsl/tests/edge_case_tests.rs:242-256`
