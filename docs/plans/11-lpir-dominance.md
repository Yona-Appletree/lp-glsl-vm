# Align LPIR to CLIF Dominance-Based Value Scoping

## Overview

Currently, LPIR enforces strict block-scoped value usage: values can only be used within their defining block or must be explicitly passed as block parameters. CLIF uses dominance-based scoping: values can be used anywhere they're dominated by their definition. Block parameters are only needed for phi-like merging from different control flow paths.

This plan restructures the codebase to add dominance analysis as a separate, well-tested module, then updates validation to use it.

## Architecture

### New Module Structure

```
crates/lpc-lpir/src/
├── analysis/              # New analysis module
│   ├── mod.rs           # Module exports
│   ├── cfg.rs           # Control Flow Graph construction
│   └── dominance.rs     # Dominance analysis (Cooper's algorithm)
├── parser/
│   └── validation.rs    # Updated to use analysis modules
```

### Separation of Concerns

1. **CFG Module** (`analysis/cfg.rs`): Pure CFG construction, no validation logic
2. **Dominance Module** (`analysis/dominance.rs`): Pure dominance computation, reusable
3. **Validation Module** (`parser/validation.rs`): Uses CFG and dominance for validation

## Implementation Plan

### Phase 1: Create Analysis Module Structure

**File**: `crates/lpc-lpir/src/analysis/mod.rs`

Create new module with exports:

```rust
pub mod cfg;
pub mod dominance;

pub use cfg::ControlFlowGraph;
pub use dominance::DominatorTree;
```

**File**: `crates/lpc-lpir/src/lib.rs`

Add analysis module:

```rust
mod analysis;
pub use analysis::{ControlFlowGraph, DominatorTree};
```

### Phase 2: Implement Control Flow Graph

**File**: `crates/lpc-lpir/src/analysis/cfg.rs`

Purpose: Build CFG from function blocks. Pure data structure, no validation.

**API**:

```rust
pub struct ControlFlowGraph {
    /// Map from block index to set of predecessor block indices
    predecessors: Vec<BTreeSet<usize>>,
    /// Map from block index to set of successor block indices
    successors: Vec<BTreeSet<usize>>,
    /// Entry block index (always 0)
    entry: usize,
}

impl ControlFlowGraph {
    /// Build CFG from function
    pub fn from_function(func: &Function) -> Self;

    /// Get predecessors of a block
    pub fn predecessors(&self, block: usize) -> &BTreeSet<usize>;

    /// Get successors of a block
    pub fn successors(&self, block: usize) -> &BTreeSet<usize>;

    /// Get all blocks in reverse post-order (for dominance computation)
    pub fn reverse_post_order(&self) -> Vec<usize>;

    /// Check if block is reachable from entry
    pub fn is_reachable(&self, block: usize) -> bool;
}
```

**Implementation notes**:

- Extract predecessors/successors from `Jump` and `Br` instructions
- Handle entry block (no predecessors)
- Compute reverse post-order using DFS
- Pure function - no side effects, easily testable

**Tests**: Unit tests for CFG construction, predecessor/successor queries, RPO ordering

### Phase 3: Implement Dominance Analysis

**File**: `crates/lpc-lpir/src/analysis/dominance.rs`

Purpose: Compute dominator tree using Cooper's "Simple, Fast Dominator Algorithm" (same as CLIF).

**API**:

```rust
pub struct DominatorTree {
    /// Immediate dominator for each block (None = unreachable or entry)
    idom: Vec<Option<usize>>,
    /// Reverse post-order numbers for efficient dominance queries
    rpo_numbers: Vec<u32>,
    /// Entry block index
    entry: usize,
}

impl DominatorTree {
    /// Compute dominator tree from CFG
    pub fn from_cfg(cfg: &ControlFlowGraph) -> Self;

    /// Check if block_a dominates block_b
    pub fn dominates(&self, block_a: usize, block_b: usize) -> bool;

    /// Get immediate dominator of a block
    pub fn immediate_dominator(&self, block: usize) -> Option<usize>;

    /// Get all blocks dominated by a given block
    pub fn dominated_blocks(&self, block: usize) -> BTreeSet<usize>;
}
```

**Algorithm** (Cooper's algorithm):

1. Compute reverse post-order of CFG
2. Initialize: entry block has no dominator, all others initially point to first predecessor
3. Iterate until fixed point:
   - For each block B (in reverse post-order):
     - idom(B) = common dominator of all predecessors
     - Common dominator = walk up dominator tree from each predecessor until finding common ancestor
4. Build dominance map for efficient queries

**Implementation details**:

- Use same algorithm as CLIF (`dominator_tree/simple.rs`)
- Store RPO numbers for O(depth) dominance queries (walking up tree)
- Handle unreachable blocks gracefully
- Pure computation - no validation logic

**Tests**:

- Unit tests for dominance relationships in various CFG shapes
- Test with loops, branches, unreachable blocks
- Test edge cases (single block, linear chain, diamond pattern)

### Phase 4: Update Value Scoping Validation

**File**: `crates/lpc-lpir/src/parser/validation.rs`

Replace `validate_value_scoping` to use dominance analysis:

**New implementation**:

```rust
pub fn validate_value_scoping(func: &Function) -> Result<(), String> {
    // 1. Build CFG
    let cfg = ControlFlowGraph::from_function(func);

    // 2. Compute dominance
    let domtree = DominatorTree::from_cfg(&cfg);

    // 3. Track value definitions (block + instruction)
    let mut value_definitions: BTreeMap<Value, (usize, usize)> = ...;

    // 4. Validate each value use
    for (use_block_idx, block) in func.blocks.iter().enumerate() {
        // Check if block is reachable
        if !cfg.is_reachable(use_block_idx) {
            // Skip unreachable blocks (or validate separately)
            continue;
        }

        for (inst_idx, inst) in block.insts.iter().enumerate() {
            for arg_value in inst.args() {
                if let Some((def_block_idx, def_inst_idx)) = value_definitions.get(&arg_value) {
                    // Check dominance: def_block must dominate use_block
                    if !domtree.dominates(*def_block_idx, use_block_idx) {
                        return Err(format!(
                            "Value {} used in block{} but defined in block{}. \
                             Value must be dominated by its definition.",
                            arg_value.index(),
                            use_block_idx,
                            def_block_idx
                        ));
                    }

                    // Check that definition comes before use (within same block)
                    if *def_block_idx == use_block_idx && *def_inst_idx >= inst_idx {
                        return Err(format!(
                            "Value {} used before definition in block{}",
                            arg_value.index(),
                            use_block_idx
                        ));
                    }
                } else {
                    return Err(format!(
                        "Value {} used but not defined",
                        arg_value.index()
                    ));
                }
            }
        }
    }

    Ok(())
}
```

**Key changes**:

- Uses `ControlFlowGraph` and `DominatorTree` from analysis module
- Checks dominance instead of block-scoped availability
- Still validates SSA property (single definition)
- Still validates use-before-def within blocks

### Phase 5: Update Tests

**File**: `crates/lpc-lpir/src/analysis/cfg.rs` (tests)

Add comprehensive CFG tests:

- Simple linear chain
- Diamond pattern (if-then-else)
- Loop with backedge
- Multiple entry points (should be invalid, but test CFG construction)
- Unreachable blocks

**File**: `crates/lpc-lpir/src/analysis/dominance.rs` (tests)

Add comprehensive dominance tests:

- Entry dominates all blocks
- Self-dominance (block dominates itself)
- Transitive dominance
- Diamond pattern dominance
- Loop dominance relationships
- Compare against known correct results

**File**: `crates/lpc-lpir/src/parser/validation.rs` (tests)

Update existing tests:

- `test_validate_cross_block_usage_direct` - Should now PASS
- `test_validate_cross_block_usage` - Should now PASS
- `test_validate_value_scoping_invalid_cross_block` - Update to test actual dominance violations
- Add new tests for:
  - Valid CLIF-style cross-block usage
  - Invalid non-dominated usage
  - Block parameters still required for phi merging

**File**: `crates/lpc-lpir/src/parser/mod.rs` (tests)

Update parser tests similarly.

### Phase 6: Update Documentation

**File**: `crates/lpc-lpir/README.md`

Update to reflect dominance-based scoping:

- Line 7: Change "explicit value scoping" → "dominance-based value scoping"
- Line 9: Update description - block parameters only for phi nodes
- Line 23: Change to "Values can be used anywhere dominated by their definition"
- Line 28: Change "No Dominance Validation" → "Dominance Validation: Values must be dominated"
- Update example to show CLIF-style cross-block usage

### Phase 7: Backend Compatibility Check

**Files**:

- `crates/lpc-codegen/src/backend/liveness.rs`
- `crates/lpc-codegen/src/backend/lower/mod.rs`

Verify backend handles cross-block values correctly:

- Liveness analysis already tracks cross-block values (should work)
- Register allocation handles values across blocks (should work)
- `copy_args_to_params` only handles explicit block parameters (phi nodes) - correct

**Action**: Review and add tests if needed, but should work as-is.

## Testing Strategy

### Unit Tests (per module)

1. **CFG Module**:

   - Test CFG construction from various function shapes
   - Test predecessor/successor queries
   - Test reverse post-order computation
   - Test reachability analysis

2. **Dominance Module**:

   - Test dominance relationships in known CFG patterns
   - Test against CLIF's dominance results (if possible)
   - Test edge cases (single block, unreachable blocks)

3. **Validation Module**:
   - Test valid CLIF-style IR
   - Test invalid dominance violations
   - Test SSA property still enforced
   - Test block parameters still validated

### Integration Tests

- Parse and validate CLIF-style examples
- Verify backend can compile validated IR
- Test round-trip: parse → validate → compile

## Migration Notes

This is a **breaking change** for IR that was written with explicit parameter passing. However:

1. **Backward compatibility**: IR with explicit parameters will still work (parameters are still valid)
2. **Forward compatibility**: New IR can use CLIF-style cross-block values
3. **Migration path**: Existing IR can be gradually updated to remove unnecessary parameters

## Implementation Order

1. Create `analysis` module structure
2. Implement `cfg.rs` with tests
3. Implement `dominance.rs` with tests
4. Update `validate_value_scoping` to use new modules
5. Update all validation tests
6. Update documentation
7. Verify backend compatibility

## Success Criteria

- [ ] CFG module implemented and tested
- [ ] Dominance module implemented and tested (using Cooper's algorithm)
- [ ] Validation uses dominance instead of block scoping
- [ ] All tests pass (updated + new)
- [ ] Documentation updated
- [ ] Backend still works correctly
- [ ] Can parse/validate CLIF-style examples

## References

- CLIF Dominance Implementation: `cranelift/codegen/src/dominator_tree/simple.rs`
- CLIF Verifier: `cranelift/codegen/src/verifier/mod.rs` (lines 1007-1053)
- Cooper's Algorithm: "A Simple, Fast Dominance Algorithm" (2001)

