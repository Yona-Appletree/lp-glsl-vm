# LPIR Validation and Builder Improvements

## Overview

This plan implements trait-based builders (without meta-language code generation) and addresses all validation gaps identified in the code review. The improvements are organized into phases that can be implemented incrementally.

## Phase 1: Fix Immediate Issues

### 1.1 Fix `set_value_type` Implementation

**File**: `crates/lpc-lpir/src/dfg/mod.rs`

- Remove duplicate comment on lines 85-86
- Replace inefficient `while` loop with proper `PrimaryMap` growth
- Use `PrimaryMap::ensure_capacity` or similar efficient method
- Current code grows map with placeholder values unnecessarily

### 1.2 Complete TODOs

**File**: `crates/lpc-lpir/src/function.rs`

- Line 169: Track actual types for block parameters (enhance `BlockData` to store parameter types)
- Line 180: Implement proper instruction formatting in `Display` implementation
- Add type information to block parameter display

## Phase 2: Instruction Format Validation

### 2.1 Add Instruction Format Validation Module

**File**: `crates/lpc-lpir/src/verifier/format.rs` (new)

Create validation that checks `InstData` structure matches opcode expectations:

- **Arithmetic ops** (Iadd, Isub, Imul, Idiv, Irem): Must have 2 args, 1 result, no block_args, no ty, no imm
- **Comparison ops** (IcmpEq, etc.): Must have 2 args, 1 result, no block_args, no ty, no imm
- **Constants** (Iconst, Fconst): Must have 0 args, 1 result, imm present, no block_args, no ty
- **Jump**: Must have args matching block_args targets, 0 results, block_args present with 1 target
- **Br**: Must have 1 condition arg + args for both targets, 0 results, block_args present with 2 targets
- **Return**: Must have args matching results, results count matches function signature
- **Load**: Must have 1 arg (address), 1 result, ty present, no block_args
- **Store**: Must have 2 args (address, value), 0 results, ty present, no block_args
- **Call**: Must have args/results matching signature (when available), no block_args
- **Syscall**: Must have imm present (syscall number), no results, no block_args
- **Halt**: Must have 0 args, 0 results, no block_args, no ty, no imm

### 2.2 Integrate Format Validation

**File**: `crates/lpc-lpir/src/verifier/mod.rs`

- Add `verify_format` function call to main `verify()` function
- Export `verify_format` from module

### 2.3 Add Format Validation Tests

**File**: `crates/lpc-lpir/src/verifier/format.rs`

- Test each opcode with correct format
- Test each opcode with incorrect formats (wrong arg count, missing fields, etc.)
- Test edge cases (empty args/results where required, etc.)

## Phase 3: Entity Existence Validation

### 3.1 Add Entity Validation Module

**File**: `crates/lpc-lpir/src/verifier/entities.rs` (new)

Validate all referenced entities exist:

- **Values**: All values in `inst.args`, `inst.results`, `block_args.targets[].1` must exist in DFG
- **Blocks**: All blocks in `block_args.targets[].0` must exist in function
- **Functions** (for Call): Function name must exist in module (requires module context in verifier)

### 3.2 Enhance Verifier to Accept Module Context

**File**: `crates/lpc-lpir/src/verifier/mod.rs`

- Add optional `Module` parameter to `verify()` function
- Pass module context to entity validation for function call checks
- Keep backward compatibility with function-only verification

### 3.3 Add Entity Validation Tests

**File**: `crates/lpc-lpir/src/verifier/entities.rs`

- Test using non-existent values
- Test branching to non-existent blocks
- Test calling non-existent functions
- Test valid entity references

## Phase 4: Complete Dominance Verification

### 4.1 Implement Full Dominance Verification

**File**: `crates/lpc-lpir/src/verifier/dominance.rs`

Replace placeholder implementation with full dominance checking:

- Use `analysis::DominatorTree` to compute dominance
- For each value definition, verify all uses are in dominated blocks
- Track value definitions: (block, inst) pairs
- For each use, check if use block is dominated by definition block
- Handle block parameters: can be used in their own block and dominated blocks
- Handle entry block parameters: can be used anywhere

### 4.2 Add Dominance Tests

**File**: `crates/lpc-lpir/src/verifier/dominance.rs`

- Test values used before definition (within same block)
- Test values used in non-dominated blocks
- Test block parameters used correctly (in own block and dominated blocks)
- Test block parameters used incorrectly (in non-dominated blocks)
- Test entry block parameters used anywhere (should be valid)

## Phase 5: CFG Integrity Checks

### 5.1 Add CFG Integrity Validation

**File**: `crates/lpc-lpir/src/verifier/cfg.rs`

Enhance existing CFG verification:

- **Entry block check**: Entry block cannot be branched to (no predecessors)
- **Instruction-block consistency**: Verify `inst_block()` matches `block_insts()` iterator
- **CFG-predecessor consistency**: All CFG predecessors must have branches to the block
- **Branch-CFG consistency**: All branches must be present in CFG

### 5.2 Add CFG Integrity Tests

**File**: `crates/lpc-lpir/src/verifier/cfg.rs`

- Test entry block being branched to (should error)
- Test instructions in wrong blocks
- Test missing predecessors in CFG
- Test branches not in CFG

## Phase 6: Enhanced Type Checking

### 6.1 Track Block Parameter Types

**File**: `crates/lpc-lpir/src/block.rs`

Enhance `BlockData` to store parameter types:

```rust
pub struct BlockData {
    pub params: Vec<Value>,
    pub param_types: Vec<Type>, // New field
}
```

### 6.2 Validate Block Argument Types

**File**: `crates/lpc-lpir/src/verifier/types.rs`

Complete `verify_block_argument_types`:

- Check that argument types match parameter types
- Validate type compatibility (i32 vs f32, etc.)
- Report type mismatches with clear error messages

### 6.3 Validate Function Call Types

**File**: `crates/lpc-lpir/src/verifier/types.rs`

Add function call type validation:

- Check call argument types match function signature parameter types
- Check call result types match function signature return types
- Requires module context (add to verifier signature)

### 6.4 Add Enhanced Type Tests

**File**: `crates/lpc-lpir/src/verifier/types.rs`

- Test block argument type mismatches
- Test function call type mismatches
- Test valid type matches

## Phase 7: Trait-Based Builders

### 7.1 Define Base Trait

**File**: `crates/lpc-lpir/src/builder/traits.rs` (new)

Create `InstBuilderBase` trait:

```rust
pub trait InstBuilderBase<'f>: Sized {
    fn data_flow_graph(&self) -> &DFG;
    fn data_flow_graph_mut(&mut self) -> &mut DFG;
    fn build(self, data: InstData) -> (InstEntity, &'f mut DFG);
}
```

### 7.2 Define InstBuilder Trait

**File**: `crates/lpc-lpir/src/builder/traits.rs`

Manually define `InstBuilder` trait with methods for each instruction:

- Arithmetic: `iadd`, `isub`, `imul`, `idiv`, `irem` (all return `Value`)
- Comparisons: `icmp_eq`, `icmp_ne`, etc. (all return `Value`)
- Constants: `iconst(value: i64)`, `fconst(value: f32)` (return `Value`)
- Control flow: `jump`, `br`, `return_`, `halt` (return `()`)
- Memory: `load`, `store` (load returns `Value`, store returns `()`)
- Calls: `call`, `syscall` (call returns `Vec<Value>`, syscall returns `()`)

Each method constructs appropriate `InstData` and calls `build()`.

### 7.3 Implement Blanket Implementation

**File**: `crates/lpc-lpir/src/builder/traits.rs`

```rust
impl<'f, T: InstBuilderBase<'f>> InstBuilder<'f> for T {}
```

### 7.4 Create InsertBuilder

**File**: `crates/lpc-lpir/src/builder/insert.rs` (new)

Create `InsertBuilder` that inserts instructions:

- Wraps `InstInserterBase` trait
- Implements `InstBuilderBase`
- Provides `with_result()` and `with_results()` for value reuse
- Inserts instruction into layout at current position

### 7.5 Create ReplaceBuilder

**File**: `crates/lpc-lpir/src/builder/replace.rs` (new)

Create `ReplaceBuilder` that replaces existing instructions:

- Takes `&mut DFG` and `InstEntity`
- Implements `InstBuilderBase`
- Replaces instruction data while preserving `InstEntity` ID
- Handles result value reuse

### 7.6 Create InstInserterBase Trait

**File**: `crates/lpc-lpir/src/builder/traits.rs`

```rust
pub trait InstInserterBase<'f>: Sized {
    fn data_flow_graph(&self) -> &DFG;
    fn data_flow_graph_mut(&mut self) -> &mut DFG;
    fn insert_built_inst(self, inst: InstEntity) -> &'f mut DFG;
}
```

### 7.7 Add Cursor Support (Optional)

**File**: `crates/lpc-lpir/src/cursor.rs` (new, optional)

Create cursor for positioning:

- `CursorPosition` enum (Nowhere, At(Inst), Before(Block), After(Block))
- `FuncCursor` struct for navigating function
- `Cursor` trait for common operations
- Integrate with `InsertBuilder` for positioned insertion

### 7.8 Update Existing Builders (Optional)

**File**: `crates/lpc-lpir/src/builder/block_builder.rs`

Optionally refactor `BlockBuilder` to use trait-based builders internally, or keep both APIs available.

### 7.9 Add Builder Tests

**File**: `crates/lpc-lpir/src/builder/traits.rs`

- Test `InsertBuilder` creates valid instructions
- Test `ReplaceBuilder` replaces instructions correctly
- Test result value reuse
- Test type safety
- Test all instruction methods

## Phase 8: Comprehensive Test Suite

### 8.1 Verifier Test Coverage

**Files**: All `crates/lpc-lpir/src/verifier/*.rs`

Add tests for:

- All validation checks (format, entities, dominance, CFG, types)
- Edge cases (empty functions, single block, etc.)
- Error message clarity
- Multiple errors reported correctly

### 8.2 Integration Tests

**File**: `crates/lpc-lpir/tests/verifier_integration.rs` (new)

- Test complete verification on complex functions
- Test verification catches all error types
- Test valid functions pass all checks

### 8.3 Property-Based Tests (Optional)

**File**: `crates/lpc-lpir/tests/property_tests.rs` (new, optional)

- Generate random valid IR and verify it passes
- Generate random invalid IR and verify it fails appropriately
- Round-trip tests (build → verify → serialize → parse → verify)

## Implementation Notes

1. **Backward Compatibility**: Keep existing `BlockBuilder`/`FunctionBuilder` APIs working
2. **Incremental Implementation**: Each phase can be implemented and tested independently
3. **Error Messages**: Ensure all validation errors include clear location information
4. **Performance**: Validation should be fast enough for development use (can be disabled in release builds if needed)
5. **Module Context**: Some validations require module context - make this optional for function-only verification

## Testing Strategy

- Unit tests for each validation module
- Integration tests for complete verification
- Builder tests for trait-based API
- Existing tests should continue passing
- Add tests before implementing features (TDD where possible)