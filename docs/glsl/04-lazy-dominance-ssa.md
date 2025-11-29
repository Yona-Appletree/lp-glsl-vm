# Lazy Dominance-Based SSA Construction Plan

## Overview

This document outlines the plan to implement lazy, on-demand dominance computation and SSA construction in the GLSL frontend, inspired by LLVM's `SSAUpdater`. This approach computes dominance only for the blocks that matter for a particular value, avoiding the need for full-function dominance analysis during codegen.

## Current State

### What We Have

- `SSABuilder` structure with basic definition tracking (`defs: BTreeMap<String, BTreeMap<BlockEntity, Value>>`)
- `compute_dominance()` method that computes full-function dominance (but can't be called during codegen)
- `get_dominating_value()` method that requires pre-computed dominance
- Manual phi node creation in control flow codegen

### Problems

1. **Full-function dominance is expensive**: Computing dominance for the entire function requires the function to be complete, which isn't available during codegen.

2. **Dominance violations**: When collecting phi values, we use values from the variables map without checking if they dominate the merge point, leading to dominance violations.

3. **Inefficient**: We don't need full-function dominance - we only need dominance for blocks that affect a particular value.

## Reference: LLVM's SSAUpdater

LLVM's `SSAUpdater` solves this by:

1. **Lazy dominance computation**: Only computes dominance for blocks backwards-reachable from the target block
2. **On-demand phi insertion**: Inserts PHIs only where needed (in dominance frontiers)
3. **Incremental**: Processes only the subset of blocks that matter

### Key Methods

```cpp
// Add a value available in a block
void AddAvailableValue(BasicBlock *BB, Value *V);

// Get value at end of block (computes dominance lazily)
Value *GetValueAtEndOfBlock(BasicBlock *BB);

// Get value in middle of block (handles in-block definitions)
Value *GetValueInMiddleOfBlock(BasicBlock *BB);
```

### Algorithm Overview

When `GetValueAtEndOfBlock(BB)` is called:

1. **BuildBlockList**: Traverse backwards from BB, collecting all blocks that could affect the value
2. **FindDominators**: Compute dominators for just those blocks (Cooper-Harvey-Kennedy algorithm)
3. **FindPHIPlacement**: Determine where PHIs are needed (dominance frontiers)
4. **FindAvailableVals**: Insert PHIs and compute values for each block

## Implementation Plan

### Phase 1: Core Data Structures

#### 1.1 Block Information Structure

```rust
// In crates/lpc-glsl/src/codegen/ssa.rs

/// Per-block information for lazy dominance computation
struct BlockInfo {
    /// The block entity
    block: BlockEntity,
    /// Value available in this block (if any)
    available_val: Option<Value>,
    /// Block that defines the available value
    def_block: Option<BlockEntity>,
    /// Postorder number (for dominance computation)
    postorder_num: u32,
    /// Immediate dominator
    idom: Option<BlockEntity>,
    /// Number of predecessors
    num_preds: usize,
    /// Predecessor blocks
    preds: Vec<BlockEntity>,
    /// PHI node if this block needs one
    phi: Option<Value>,
}
```

#### 1.2 Update SSABuilder

```rust
pub struct SSABuilder {
    /// Map from variable name to map of block -> value
    defs: BTreeMap<String, BTreeMap<BlockEntity, Value>>,

    /// Set of variables that need phi nodes
    needs_phi: BTreeSet<String>,

    /// Block to index mapping (for dominance queries)
    block_to_index: BTreeMap<BlockEntity, usize>,

    /// Cached dominance tree (optional, for full-function queries)
    domtree: Option<DominatorTree>,

    /// Cached CFG (optional)
    cfg: Option<ControlFlowGraph>,
}
```

### Phase 2: Lazy Dominance Computation

#### 2.1 Build Backwards-Reachable Block List

```rust
impl SSABuilder {
    /// Build a list of blocks backwards-reachable from the target block.
    /// Returns a list of blocks that could affect the value at the target,
    /// along with a pseudo-entry block.
    fn build_block_list(
        &self,
        var: &str,
        target_block: BlockEntity,
        function: &Function,
    ) -> (Vec<BlockInfo>, BlockInfo) {
        // 1. Start from target_block
        // 2. Traverse backwards through predecessors
        // 3. Stop when we reach blocks that define the variable
        // 4. Assign postorder numbers during forward traversal
        // 5. Return list of blocks and pseudo-entry
    }
}
```

**Algorithm**:

- Use a worklist starting with `target_block`
- For each block, check if it defines the variable
- If yes, add to "root list" (blocks with definitions)
- If no, add predecessors to worklist
- Do forward DFS from roots to assign postorder numbers

#### 2.2 Cooper-Harvey-Kennedy Dominance Algorithm

```rust
impl SSABuilder {
    /// Compute immediate dominators for a subset of blocks.
    /// Uses Cooper-Harvey-Kennedy algorithm.
    fn find_dominators(
        &mut self,
        block_list: &mut [BlockInfo],
        pseudo_entry: &BlockInfo,
    ) {
        // Iterate until convergence:
        //   for each block in reverse postorder:
        //     idom = intersect of all predecessors' idoms
        //     if idom changed, mark changed and continue
    }

    /// Find common dominator of two blocks using postorder numbers.
    fn intersect_dominators(
        &self,
        block1: &BlockInfo,
        block2: &BlockInfo,
    ) -> Option<BlockEntity> {
        // Walk up dominator tree until postorder numbers match
        // Return the common dominator
    }
}
```

**Cooper-Harvey-Kennedy Algorithm**:

1. Initialize: Entry block has no dominator, all others point to first predecessor
2. Iterate until convergence:
   - For each block in reverse postorder:
     - `idom = intersect(predecessors' idoms)`
     - If changed, mark and continue
3. Intersect function: Walk up dominator tree using postorder numbers

### Phase 3: PHI Placement (Dominance Frontiers)

#### 3.1 Dominance Frontier Computation

```rust
impl SSABuilder {
    /// Determine where PHIs are needed using dominance frontiers.
    /// A PHI is needed in block B if:
    /// - B is in the dominance frontier of a definition
    /// - Multiple definitions reach B through different paths
    fn find_phi_placement(
        &mut self,
        block_list: &mut [BlockInfo],
    ) {
        // Iterate until convergence:
        //   for each block:
        //     if any predecessor is in dominance frontier of a definition:
        //       mark this block as needing a PHI
        //     else:
        //       use same def as immediate dominator
    }

    /// Check if a definition is in the dominance frontier.
    /// A block is in the dominance frontier of Def if:
    /// - Def dominates a predecessor of the block
    /// - Def does not dominate the block itself
    fn is_def_in_dom_frontier(
        &self,
        pred: &BlockInfo,
        idom: &BlockInfo,
    ) -> bool {
        // Walk up from pred to idom, checking for definitions
    }
}
```

**Dominance Frontier**:

- Block B is in DF(Def) if:
  - Def dominates a predecessor of B
  - Def does not dominate B itself
- PHIs are needed in dominance frontiers of definitions

### Phase 4: Value Computation and PHI Insertion

#### 4.1 Compute Available Values

```rust
impl SSABuilder {
    /// Compute available values for each block, inserting PHIs as needed.
    fn find_available_values(
        &mut self,
        block_list: &mut [BlockInfo],
        function: &mut FunctionBuilder,
    ) -> GlslResult<()> {
        // Forward pass: Create empty PHIs where needed
        // Reverse pass: Fill in PHI operands from predecessors
    }
}
```

**Algorithm**:

1. Forward pass (reverse postorder):
   - For blocks needing PHIs, create empty PHI nodes
   - For other blocks, use value from immediate dominator
2. Reverse pass (forward postorder):
   - For each block:
     - If has PHI, fill in operands from predecessors
     - Otherwise, propagate value to successors

#### 4.2 Public API

```rust
impl SSABuilder {
    /// Get the value of a variable at the end of a block.
    /// This lazily computes dominance and inserts PHIs as needed.
    pub fn get_value_at_end_of_block(
        &mut self,
        var: &str,
        block: BlockEntity,
        function: &Function,
        function_builder: &mut FunctionBuilder,
    ) -> GlslResult<Option<Value>> {
        // 1. Check if we already have a value for this block
        // 2. If not, build block list
        // 3. Compute dominators
        // 4. Find PHI placement
        // 5. Compute available values (insert PHIs)
        // 6. Return value for target block
    }

    /// Get the value of a variable in the middle of a block.
    /// Handles cases where the variable is defined later in the same block.
    pub fn get_value_in_middle_of_block(
        &mut self,
        var: &str,
        block: BlockEntity,
        function: &Function,
        function_builder: &mut FunctionBuilder,
    ) -> GlslResult<Option<Value>> {
        // If variable defined in this block, create PHI for live-in value
        // Otherwise, use get_value_at_end_of_block
    }
}
```

### Phase 5: Integration with CodeGen

#### 5.1 Update Control Flow Codegen

Replace manual phi collection with lazy SSA construction:

```rust
// In crates/lpc-glsl/src/control/codegen.rs

// OLD: Manual phi value collection
let mut true_values = Vec::new();
for var_name in &phi_var_names {
    let val = true_end_vars.get(var_name).unwrap();
    true_values.push(*val);
}

// NEW: Use lazy SSA construction
let mut true_values = Vec::new();
for var_name in &phi_var_names {
    let val = ctx.ssa_builder_mut().get_value_at_end_of_block(
        var_name,
        jump_source_block,
        ctx.builder().function(),
        ctx.builder_mut(),
    )?;
    true_values.push(val);
}
```

#### 5.2 Update Variable Reads

```rust
// In crates/lpc-glsl/src/expr/codegen.rs

// Variable reference
Expr::Variable(ident) => {
    let name = ident.0.as_str();
    let block = ctx.current_block()?;

    // Use lazy SSA construction
    if let Some(value) = ctx.ssa_builder_mut().get_value_at_end_of_block(
        name,
        block,
        ctx.builder().function(),
        ctx.builder_mut(),
    )? {
        Ok(value)
    } else {
        Err(GlslError::codegen(format!("Undefined variable '{}'", name)))
    }
}
```

### Phase 6: Optimization and Caching

#### 6.1 Cache Block Lists

```rust
impl SSABuilder {
    /// Cache of block lists per variable (to avoid recomputation)
    block_list_cache: BTreeMap<String, Vec<BlockInfo>>,

    /// Reuse block lists when possible
    fn get_or_build_block_list(
        &mut self,
        var: &str,
        target_block: BlockEntity,
        function: &Function,
    ) -> &mut Vec<BlockInfo> {
        // Check cache, build if needed
    }
}
```

#### 6.2 Incremental Updates

When a new definition is added:

- Invalidate cache for that variable
- Or incrementally update block list

## Implementation Steps

### Step 1: Core Data Structures (Week 1)

- [ ] Add `BlockInfo` struct
- [ ] Update `SSABuilder` with block list cache
- [ ] Add helper methods for block traversal

### Step 2: Block List Construction (Week 1)

- [ ] Implement `build_block_list()`
- [ ] Test with simple control flow
- [ ] Handle edge cases (unreachable blocks, etc.)

### Step 3: Dominance Computation (Week 2)

- [ ] Implement Cooper-Harvey-Kennedy algorithm
- [ ] Implement `intersect_dominators()`
- [ ] Test dominance computation on various CFGs

### Step 4: PHI Placement (Week 2)

- [ ] Implement dominance frontier computation
- [ ] Implement `find_phi_placement()`
- [ ] Test PHI placement correctness

### Step 5: Value Computation (Week 3)

- [ ] Implement `find_available_values()`
- [ ] Implement PHI insertion
- [ ] Test value computation

### Step 6: Public API (Week 3)

- [ ] Implement `get_value_at_end_of_block()`
- [ ] Implement `get_value_in_middle_of_block()`
- [ ] Add error handling

### Step 7: Integration (Week 4)

- [ ] Update control flow codegen
- [ ] Update variable reads
- [ ] Remove old manual phi code

### Step 8: Testing and Validation (Week 4)

- [ ] Test with `test_nested_control_flow`
- [ ] Test with all existing tests
- [ ] Fix any regressions
- [ ] Add new tests for edge cases

## Benefits

1. **Correctness**: Proper dominance checking ensures no dominance violations
2. **Efficiency**: Only computes dominance for blocks that matter
3. **Simplicity**: Codegen doesn't need to worry about dominance - SSABuilder handles it
4. **Maintainability**: Centralized SSA construction logic
5. **Extensibility**: Easy to add optimizations (PHI elimination, etc.)

## References

- **LLVM SSAUpdater**: `/Users/yona/dev/photomancer/DirectXShaderCompiler/include/llvm/Transforms/Utils/SSAUpdater.h`
- **SSAUpdaterImpl**: `/Users/yona/dev/photomancer/DirectXShaderCompiler/include/llvm/Transforms/Utils/SSAUpdaterImpl.h`
- **Cooper-Harvey-Kennedy Paper**: "A Simple, Fast Dominance Algorithm" (2001)
- **Dominance Frontiers**: Cytron et al., "Efficiently Computing Static Single Assignment Form and the Control Dependence Graph" (1991)

## Notes

- This approach is similar to LLVM's `SSAUpdater`, which is proven to work well
- The lazy approach means we don't need full-function dominance during codegen
- PHIs are inserted automatically where needed, reducing manual codegen complexity
- The algorithm handles nested control flow correctly by computing dominance for all relevant blocks
