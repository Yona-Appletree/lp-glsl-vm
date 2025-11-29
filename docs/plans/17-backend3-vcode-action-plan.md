# Backend3 VCode Action Plan

**Date**: Current
**Based on**: Review in `17-backend3-vcode-review.md`

## Overview

This document provides a concrete action plan to address the shortcomings identified in the VCode review. Tasks are prioritized and organized by phase.

## Phase 1 Completion Tasks (Before Phase 2)

### Task 1: Verify `emit_info` Requirement
**Priority**: High
**Estimated Time**: 1-2 hours
**Owner**: TBD

**Action Items**:
1. Review Phase 3 (Emission) planning documents
2. Check Cranelift's usage of `emit_info` in emission
3. If required, add `emit_info: I::Info` to VCode structure
4. Add `emit_info` parameter to `LowerBackend` trait
5. Update VCodeBuilder to accept and store `emit_info`

**Files to Modify**:
- `crates/lpc-codegen/src/backend3/vcode.rs`
- `crates/lpc-codegen/src/backend3/vcode_builder.rs`
- `crates/lpc-codegen/src/backend3/lower.rs`
- `crates/lpc-codegen/src/isa/riscv32/backend3/mod.rs` (if needed)

**Acceptance Criteria**:
- [ ] Decision documented (required or deferred)
- [ ] If required, implementation complete and tested
- [ ] All tests pass

---

### Task 2: Add Edge Case Tests for Block Ordering
**Priority**: High
**Estimated Time**: 2-3 hours
**Owner**: TBD

**Action Items**:
1. Add test: Block ordering with no critical edges
2. Add test: Block ordering with all critical edges
3. Add test: Block ordering with entry block having critical edges
4. Add test: Block ordering with exit blocks having critical edges
5. Add test: Block ordering preserves RPO property (defs before uses)

**Files to Create/Modify**:
- `crates/lpc-codegen/src/backend3/tests/blockorder_edge_tests.rs` (extend)

**Test Cases**:
```rust
// Test 1: No critical edges
fn test_block_order_no_critical_edges() { ... }

// Test 2: All critical edges
fn test_block_order_all_critical_edges() { ... }

// Test 3: Entry block with critical edges
fn test_block_order_entry_critical_edges() { ... }

// Test 4: Exit blocks with critical edges
fn test_block_order_exit_critical_edges() { ... }

// Test 5: RPO property preservation
fn test_block_order_rpo_property() { ... }
```

**Acceptance Criteria**:
- [ ] All tests pass
- [ ] Tests cover edge cases not currently covered
- [ ] Tests verify correctness properties

---

### Task 3: Add Edge Case Tests for Operand Collection
**Priority**: High
**Estimated Time**: 2-3 hours
**Owner**: TBD

**Action Items**:
1. Add test: Instructions with Mod operands (read-write)
2. Add test: Instructions with fixed register constraints
3. Add test: Instructions with register class constraints
4. Add test: Operand collection with empty instruction list
5. Add test: Operand collection with single instruction

**Files to Create/Modify**:
- `crates/lpc-codegen/src/backend3/tests/operand_tests.rs` (extend)

**Test Cases**:
```rust
// Test 1: Mod operands
fn test_operand_collection_mod_operands() { ... }

// Test 2: Fixed register constraints
fn test_operand_collection_fixed_registers() { ... }

// Test 3: Register class constraints
fn test_operand_collection_reg_class() { ... }

// Test 4: Empty instruction list
fn test_operand_collection_empty() { ... }

// Test 5: Single instruction
fn test_operand_collection_single() { ... }
```

**Acceptance Criteria**:
- [ ] All tests pass
- [ ] Tests verify operand constraints are correctly collected
- [ ] Tests verify operand kinds (Use/Def/Mod) are correct

---

### Task 4: Add Edge Case Tests for VCode Building
**Priority**: High
**Estimated Time**: 2-3 hours
**Owner**: TBD

**Action Items**:
1. Add test: Building VCode with no instructions (empty function)
2. Add test: Building VCode with single block, single instruction
3. Add test: Building VCode with blocks that have no predecessors
4. Add test: Building VCode with blocks that have no successors (exit blocks)
5. Add test: Building VCode with entry block having parameters

**Files to Create/Modify**:
- `crates/lpc-codegen/src/backend3/tests/vcode_invariants_tests.rs` (extend)

**Test Cases**:
```rust
// Test 1: Empty function
fn test_vcode_build_empty_function() { ... }

// Test 2: Single block, single instruction
fn test_vcode_build_single_instruction() { ... }

// Test 3: Blocks with no predecessors
fn test_vcode_build_no_predecessors() { ... }

// Test 4: Exit blocks (no successors)
fn test_vcode_build_exit_blocks() { ... }

// Test 5: Entry block with parameters
fn test_vcode_build_entry_with_params() { ... }
```

**Acceptance Criteria**:
- [ ] All tests pass
- [ ] Tests verify VCode invariants hold in edge cases
- [ ] Tests don't panic on edge cases

---

### Task 5: Add Edge Case Tests for Lowering
**Priority**: Medium
**Estimated Time**: 2-3 hours
**Owner**: TBD

**Action Items**:
1. Add test: Lowering function with no parameters
2. Add test: Lowering function with no return value
3. Add test: Lowering function with multiple return paths
4. Add test: Lowering with phi nodes that have identical source values
5. Add test: Lowering with edge blocks that have no phi moves (all moves elided)

**Files to Create/Modify**:
- `crates/lpc-codegen/src/backend3/tests/lower_tests.rs` (extend)

**Test Cases**:
```rust
// Test 1: No parameters
fn test_lower_no_parameters() { ... }

// Test 2: No return value
fn test_lower_no_return() { ... }

// Test 3: Multiple return paths
fn test_lower_multiple_returns() { ... }

// Test 4: Phi with identical sources
fn test_lower_phi_identical_sources() { ... }

// Test 5: Edge blocks with no moves
fn test_lower_edge_blocks_no_moves() { ... }
```

**Acceptance Criteria**:
- [ ] All tests pass
- [ ] Tests verify lowering correctness in edge cases
- [ ] Tests verify VCode structure is correct

---

### Task 6: Add Integration Tests
**Priority**: Medium
**Estimated Time**: 2-3 hours
**Owner**: TBD

**Action Items**:
1. Add test: End-to-end lowering of complex function
2. Add test: Lowering preserves source locations
3. Add test: Lowering with constants requiring LUI+ADDI
4. Add test: Lowering with mixed inline and large constants

**Files to Create/Modify**:
- `crates/lpc-codegen/src/backend3/tests/integration_tests.rs` (extend)

**Test Cases**:
```rust
// Test 1: Complex function
fn test_lower_complex_function() {
    // Function with: multiple blocks, critical edges, phi nodes, constants
}

// Test 2: Source location preservation
fn test_lower_preserves_srclocs() { ... }

// Test 3: Large constants
fn test_lower_large_constants() { ... }

// Test 4: Mixed constants
fn test_lower_mixed_constants() { ... }
```

**Acceptance Criteria**:
- [ ] All tests pass
- [ ] Tests verify end-to-end correctness
- [ ] Tests cover realistic scenarios

---

### Task 7: Add Validation Tests
**Priority**: Medium
**Estimated Time**: 1-2 hours
**Owner**: TBD

**Action Items**:
1. Add test: Validation catches invalid entry block index
2. Add test: Validation catches non-contiguous block ranges
3. Add test: Validation catches non-contiguous operand ranges
4. Add test: Validation catches mismatched source location count
5. Add test: Validation catches mismatched operand range count

**Files to Create/Modify**:
- `crates/lpc-codegen/src/backend3/tests/vcode_invariants_tests.rs` (extend)

**Note**: These tests may require exposing validation internals or creating invalid VCode structures manually.

**Acceptance Criteria**:
- [ ] All tests pass
- [ ] Tests verify validation catches errors
- [ ] Tests use appropriate error handling (panics vs. Results)

---

## Phase 2 Preparation Tasks (Before Register Allocation)

### Task 8: Verify Operand Collection Completeness
**Priority**: High
**Estimated Time**: 2-3 hours
**Owner**: TBD

**Action Items**:
1. Audit all RISC-V instruction types
2. Verify all instructions implement `get_operands()` correctly
3. Verify operand constraints are correct for each instruction
4. Test with regalloc2 to ensure compatibility

**Files to Review**:
- `crates/lpc-codegen/src/isa/riscv32/backend3/inst.rs`

**Acceptance Criteria**:
- [ ] All instructions have correct operand collection
- [ ] Operand constraints match instruction semantics
- [ ] regalloc2 integration works correctly

---

### Task 9: Document Operand Constraint System
**Priority**: Medium
**Estimated Time**: 1 hour
**Owner**: TBD

**Action Items**:
1. Document how operand constraints work
2. Document ISA-specific constraint implementation
3. Add examples for each constraint type
4. Document how constraints interact with regalloc2

**Files to Create/Modify**:
- `crates/lpc-codegen/src/backend3/vcode.rs` (add module-level docs)
- `docs/plans/17-backend3-1-foundation.md` (extend)

**Acceptance Criteria**:
- [ ] Documentation is clear and complete
- [ ] Examples are provided
- [ ] ISA-specific details are documented

---

## Phase 3 Preparation Tasks (Before Emission)

### Task 10: Implement Cold Block Identification
**Priority**: Medium
**Estimated Time**: 3-4 hours
**Owner**: TBD

**Action Items**:
1. Implement basic heuristics for cold block identification
2. Add heuristics: error handling paths, blocks dominated by unlikely conditions
3. Add tests for cold block identification
4. Document approach for future profile-guided optimization

**Files to Modify**:
- `crates/lpc-codegen/src/backend3/blockorder.rs`

**Heuristics to Implement**:
- Blocks that are only reached via error/unlikely paths
- Blocks dominated by unlikely conditions (e.g., error checks)
- Blocks with no predecessors (entry block is hot, others may be cold)

**Acceptance Criteria**:
- [ ] Basic heuristics implemented
- [ ] Tests pass
- [ ] Documentation updated

---

### Task 11: Implement Indirect Branch Target Tracking
**Priority**: Medium
**Estimated Time**: 2-3 hours
**Owner**: TBD

**Action Items**:
1. Analyze branch instructions during block ordering
2. Identify indirect branches (computed jumps, switch statements)
3. Track which blocks are indirect targets
4. Add tests for indirect branch target tracking

**Files to Modify**:
- `crates/lpc-codegen/src/backend3/blockorder.rs`

**Note**: LPIR may not have indirect branches yet. If not, document the approach for when they're added.

**Acceptance Criteria**:
- [ ] Implementation complete (or documented if not applicable)
- [ ] Tests pass (or skipped if not applicable)
- [ ] Documentation updated

---

### Task 12: Implement Block Alignment Support
**Priority**: Low
**Estimated Time**: 2-3 hours
**Owner**: TBD

**Action Items**:
1. Add alignment field to BlockMetadata (already present, just needs logic)
2. Determine alignment requirements (if any) for RISC-V
3. Integrate alignment with block ordering if needed
4. Add tests for block alignment

**Files to Modify**:
- `crates/lpc-codegen/src/backend3/vcode_builder.rs`
- `crates/lpc-codegen/src/backend3/blockorder.rs`

**Acceptance Criteria**:
- [ ] Alignment support implemented
- [ ] Tests pass
- [ ] Documentation updated

---

### Task 13: Complete Clobber Handling
**Priority**: Medium
**Estimated Time**: 2-3 hours
**Owner**: TBD

**Action Items**:
1. Implement `get_clobbers()` for RISC-V function call instructions
2. Implement proper PRegSet type (replace `BTreeSet<u32>`)
3. Test function call clobber tracking
4. Verify clobbers are correctly collected during operand collection

**Files to Modify**:
- `crates/lpc-codegen/src/isa/riscv32/backend3/inst.rs`
- `crates/lpc-codegen/src/backend3/vcode.rs` (PRegSet type)
- `crates/lpc-codegen/src/backend3/tests/clobber_tests.rs`

**Acceptance Criteria**:
- [ ] `get_clobbers()` implemented for function calls
- [ ] PRegSet uses proper ISA-specific types
- [ ] Tests pass
- [ ] Clobbers are correctly tracked

---

## Code Quality Tasks

### Task 14: Remove Placeholder Comments
**Priority**: Low
**Estimated Time**: 30 minutes
**Owner**: TBD

**Action Items**:
1. Replace "Not implemented yet" comments with proper TODOs or remove
2. Update deferred.md if features are truly deferred
3. Ensure all TODOs are tracked

**Files to Review**:
- `crates/lpc-codegen/src/backend3/vcode_builder.rs:482`
- All files with placeholder comments

**Acceptance Criteria**:
- [ ] No placeholder comments remain
- [ ] All TODOs are tracked in deferred.md or action plan

---

### Task 15: Add Inline Documentation
**Priority**: Low
**Estimated Time**: 1-2 hours
**Owner**: TBD

**Action Items**:
1. Document complex algorithms (e.g., predecessor computation)
2. Add doc comments to public APIs
3. Add examples where helpful

**Files to Review**:
- `crates/lpc-codegen/src/backend3/vcode_builder.rs` (predecessor computation)
- `crates/lpc-codegen/src/backend3/blockorder.rs` (critical edge detection)
- `crates/lpc-codegen/src/backend3/lower.rs` (lowering algorithm)

**Acceptance Criteria**:
- [ ] Complex algorithms are documented
- [ ] Public APIs have doc comments
- [ ] Examples are provided where helpful

---

## Testing Summary

### Test Coverage Goals

**Current Coverage**: Good (comprehensive invariant tests, operand tests, etc.)
**Target Coverage**: Excellent (add edge cases, integration tests)

**New Tests Needed**:
- ~25-30 new test cases across multiple test files
- Focus on edge cases and integration scenarios
- Estimated 10-15 hours of test development

### Test Organization

Tests are well-organized in separate files:
- `vcode_invariants_tests.rs` - Structure validation
- `operand_tests.rs` - Operand collection
- `blockorder_tests.rs` - Block ordering
- `lower_tests.rs` - Lowering logic
- `integration_tests.rs` - End-to-end tests

**Recommendation**: Continue this organization, extend existing files rather than creating new ones.

---

## Timeline Estimate

### Phase 1 Completion (Before Phase 2)
- Tasks 1-7: ~15-20 hours
- Focus: Edge case tests, emit_info verification

### Phase 2 Preparation
- Tasks 8-9: ~3-4 hours
- Focus: regalloc2 compatibility

### Phase 3 Preparation
- Tasks 10-13: ~9-13 hours
- Focus: Block metadata, clobbers

### Code Quality
- Tasks 14-15: ~2-3 hours
- Focus: Documentation, cleanup

**Total Estimated Time**: ~29-40 hours

---

## Risk Assessment

### Low Risk
- Adding tests (well-understood, low impact)
- Documentation improvements
- Code quality improvements

### Medium Risk
- `emit_info` addition (may require ISA changes)
- Cold block identification (heuristics may need tuning)
- Clobber handling (requires ISA-specific work)

### High Risk
- None identified (all tasks are incremental improvements)

---

## Success Criteria

### Phase 1 Completion
- [ ] All high-priority tasks complete
- [ ] Test coverage increased to cover edge cases
- [ ] `emit_info` requirement verified
- [ ] All tests pass
- [ ] Code ready for Phase 2 (Register Allocation)

### Phase 2 Preparation
- [ ] Operand collection verified complete
- [ ] regalloc2 compatibility confirmed
- [ ] Documentation complete

### Phase 3 Preparation
- [ ] Block metadata features implemented
- [ ] Clobber handling complete
- [ ] Ready for emission phase

---

## Notes

- Tasks can be worked on in parallel (different test files, different features)
- Prioritize Phase 1 completion tasks before moving to Phase 2
- Some tasks may be deferred if not critical (e.g., cold block identification)
- All code changes should follow the project's commit message and formatting guidelines


