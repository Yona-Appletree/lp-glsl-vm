# Backend3 VCode Review

**Date**: Current
**Reviewer**: AI Assistant
**Reference**: `docs/plans/17-backend3-1-foundation.md` and Cranelift codegen

## Executive Summary

The current VCode implementation is **substantially complete** for Phase 1 (Foundation). The core structure matches Cranelift's design, and most critical functionality is implemented. However, there are several gaps, missing tests, and TODOs that should be addressed before moving to the next phase.

## What's Implemented ✅

### Core VCode Structure
- ✅ VCode struct with all required fields (insts, operands, blocks, constants, relocations, srclocs)
- ✅ VCodeBuilder for incremental construction
- ✅ Block lowering order computation with critical edge splitting
- ✅ Operand collection from instructions
- ✅ Predecessor computation from successors (counting sort algorithm)
- ✅ Source location tracking
- ✅ Block metadata (cold blocks, indirect targets - structure present, logic deferred)
- ✅ Comprehensive validation in `build()` method

### Lowering Infrastructure
- ✅ Generic `Lower` struct (ISA-agnostic)
- ✅ `LowerBackend` trait for ISA-specific lowering
- ✅ Virtual register allocation (1:1 mapping with IR Values)
- ✅ Edge block handling (phi moves)
- ✅ Block parameter handling
- ✅ Branch argument tracking

### Testing
- ✅ Comprehensive invariant tests (`vcode_invariants_tests.rs`)
- ✅ Operand collection tests (`operand_tests.rs`)
- ✅ Block ordering tests (`blockorder_tests.rs`, `blockorder_edge_tests.rs`)
- ✅ Lowering tests (`lower_tests.rs`)
- ✅ Constant materialization tests (`constants_tests.rs`)
- ✅ Source location tests (`srcloc_tests.rs`)
- ✅ CFG pattern tests (`cfg_patterns_tests.rs`)
- ✅ Clobber tests (`clobber_tests.rs`)
- ✅ Relocation tests (`reloc_tests.rs`)

## What's Missing or Incomplete ❌

### 1. VCode Structure Gaps (vs. Cranelift)

#### Missing Fields (Deferred per Foundation Doc)
- ❌ `vreg_types: Vec<Type>` - VReg type information (for validation)
- ❌ `emit_info: I::Info` - ISA-specific emission info (needed for instruction encoding)
- ❌ `debug_value_labels` - Debug info (deferred, acceptable)
- ❌ `facts: Vec<Option<Fact>>` - Proof-carrying code (deferred, acceptable)
- ❌ `user_stack_maps` - GC safepoints (deferred, acceptable)

**Impact**: `emit_info` may be needed for Phase 3 (Emission). Should verify if it's truly optional or required.

### 2. Block Ordering TODOs

**Location**: `crates/lpc-codegen/src/backend3/blockorder.rs:87-103`

- ❌ **Cold block identification** - Currently returns empty set
  - TODO comment indicates this should use profile data, heuristics, or user annotations
  - **Impact**: Low for Phase 1, but needed for optimization phases

- ❌ **Indirect branch target tracking** - Currently returns empty set
  - TODO comment indicates this requires analysis of branch instructions
  - **Impact**: Low for Phase 1, but needed for proper block alignment in emission

### 3. Block Metadata

**Location**: `crates/lpc-codegen/src/backend3/vcode_builder.rs:482`

- ❌ **Alignment requirement** - Always set to `None`
  - Comment: "Not implemented yet"
  - **Impact**: May be needed for emission phase if blocks require alignment

### 4. Operand Constraint System

**Location**: `crates/lpc-codegen/src/backend3/vcode.rs:78-91`

- ⚠️ **Placeholder implementation** - Uses `u32` for fixed registers
- ⚠️ **PReg trait** - Defined but not implemented by ISA
- **Impact**: Works for regalloc2 integration, but ISA-specific constraints may need refinement

### 5. Clobber Handling

**Location**: `crates/lpc-codegen/src/backend3/tests/clobber_tests.rs:144`

- ⚠️ **Placeholder PRegSet** - Uses `BTreeSet<u32>` instead of ISA-specific types
- ⚠️ **RISC-V instructions don't implement `get_clobbers()`** - Per test comments
- **Impact**: Function calls won't properly track clobbered registers until implemented

### 6. Relocation Integration

**Location**: `crates/lpc-codegen/src/backend3/reloc.rs:23`

- ⚠️ **Note**: "Currently, relocations are recorded but not automatically used during lowering"
- **Impact**: Relocations are tracked but may not be fully integrated with lowering

### 7. Unimplemented Instructions

**Location**: `crates/lpc-codegen/src/backend3/tests/lower_tests.rs`

Multiple tests marked with "NOTE: This test will fail until X is implemented":
- ❌ Unsigned comparisons (`ult`, `ule`, `ugt`, `uge`)
- ❌ Function calls (`call`)
- ❌ Syscalls (`syscall`)
- ❌ Halt (`halt`)
- ❌ Trap instructions (`trapz`, `trapnz`)

**Impact**: These are ISA-specific lowering issues, not VCode structure issues. Should be tracked separately.

## Missing Tests 🔍

### 1. Edge Cases in Block Ordering

- ❌ **Test**: Block ordering with no critical edges
- ❌ **Test**: Block ordering with all critical edges
- ❌ **Test**: Block ordering with entry block having critical edges
- ❌ **Test**: Block ordering with exit blocks having critical edges
- ❌ **Test**: Block ordering preserves RPO property (defs before uses)

### 2. Edge Cases in Operand Collection

- ❌ **Test**: Instructions with Mod operands (read-write)
- ❌ **Test**: Instructions with fixed register constraints
- ❌ **Test**: Instructions with register class constraints
- ❌ **Test**: Operand collection with empty instruction list
- ❌ **Test**: Operand collection with single instruction

### 3. Edge Cases in VCode Building

- ❌ **Test**: Building VCode with no instructions (empty function)
- ❌ **Test**: Building VCode with single block, single instruction
- ❌ **Test**: Building VCode with blocks that have no predecessors
- ❌ **Test**: Building VCode with blocks that have no successors (exit blocks)
- ❌ **Test**: Building VCode with entry block having parameters

### 4. Edge Cases in Lowering

- ❌ **Test**: Lowering function with no parameters
- ❌ **Test**: Lowering function with no return value
- ❌ **Test**: Lowering function with multiple return paths
- ❌ **Test**: Lowering with phi nodes that have identical source values
- ❌ **Test**: Lowering with edge blocks that have no phi moves (all moves elided)

### 5. Integration Tests

- ❌ **Test**: End-to-end lowering of complex function with multiple blocks, critical edges, and phi nodes
- ❌ **Test**: Lowering preserves source locations across all instructions
- ❌ **Test**: Lowering with constants that require LUI+ADDI sequence
- ❌ **Test**: Lowering with mixed inline and large constants

### 6. Validation Tests

- ❌ **Test**: Validation catches invalid entry block index
- ❌ **Test**: Validation catches non-contiguous block ranges
- ❌ **Test**: Validation catches non-contiguous operand ranges
- ❌ **Test**: Validation catches mismatched source location count
- ❌ **Test**: Validation catches mismatched operand range count

### 7. Performance/Stress Tests

- ❌ **Test**: Lowering function with many blocks (100+)
- ❌ **Test**: Lowering function with many critical edges
- ❌ **Test**: Lowering function with many phi nodes
- ❌ **Test**: Operand collection performance with many instructions

## Comparison with Cranelift

### Structural Alignment ✅

The VCode structure closely matches Cranelift's design:
- ✅ Same flat array structure for operands, blocks, successors/predecessors
- ✅ Same Ranges-based indexing
- ✅ Same block lowering order approach
- ✅ Same critical edge splitting strategy

### Key Differences

1. **Build Direction**: Cranelift supports backward building (for instruction sinking), we only support forward building
   - **Impact**: May limit optimization opportunities, but acceptable for Phase 1

2. **VReg Types**: Cranelift tracks VReg types, we don't
   - **Impact**: May be needed for validation, but not critical for Phase 1

3. **Emit Info**: Cranelift has `emit_info: I::Info`, we don't
   - **Impact**: May be needed for Phase 3 (Emission), should verify

4. **Debug Info**: Cranelift has extensive debug info support, we defer it
   - **Impact**: Acceptable for Phase 1

## TODOs and Action Items

### High Priority (Before Phase 2)

1. **Verify `emit_info` requirement**
   - Check if Phase 3 (Emission) requires ISA-specific emit info
   - If yes, add to VCode structure and LowerBackend trait

2. **Add missing edge case tests**
   - Focus on block ordering edge cases
   - Focus on operand collection edge cases
   - Focus on VCode building edge cases

3. **Complete operand constraint system**
   - Verify ISA-specific constraints work correctly
   - Test fixed register constraints
   - Test register class constraints

### Medium Priority (Before Phase 3)

4. **Implement cold block identification**
   - Add basic heuristics (e.g., error handling paths)
   - Document approach for future profile-guided optimization

5. **Implement indirect branch target tracking**
   - Analyze branch instructions during block ordering
   - Track which blocks are indirect targets

6. **Implement block alignment support**
   - Add alignment field to BlockMetadata
   - Integrate with block ordering if needed

7. **Complete clobber handling**
   - Implement `get_clobbers()` for RISC-V instructions
   - Test function call clobber tracking

### Low Priority (Future Phases)

8. **Add VReg type tracking** (if needed for validation)
9. **Add debug value labels** (for debug info)
10. **Add proof-carrying code facts** (advanced feature)
11. **Add user stack maps** (only if GC is needed)

## Recommendations

### Immediate Actions

1. **Add comprehensive edge case tests** - The current test suite is good but missing edge cases
2. **Verify `emit_info` requirement** - Check with Phase 3 planning to see if this is needed
3. **Document operand constraint system** - Clarify how ISA-specific constraints should work

### Before Phase 2 (Register Allocation)

1. **Ensure operand collection is complete** - Verify all instruction types properly implement `get_operands()`
2. **Test with regalloc2** - Verify VCode structure works correctly with regalloc2
3. **Add validation for regalloc2 requirements** - Ensure VCode meets all regalloc2 expectations

### Code Quality

1. **Remove placeholder comments** - Replace "Not implemented yet" with proper TODOs or remove if not needed
2. **Document deferred features** - Ensure deferred.md is up to date
3. **Add inline documentation** - Document complex algorithms (e.g., predecessor computation)

## Conclusion

The VCode implementation is **solid and ready for Phase 1 completion**. The core structure is correct, most functionality is implemented, and test coverage is good. The main gaps are:

1. Edge case test coverage
2. Some deferred features (cold blocks, indirect targets, alignment)
3. ISA-specific details (clobbers, operand constraints)

**Recommendation**: Address high-priority items (edge case tests, emit_info verification) before moving to Phase 2. Defer medium/low priority items to appropriate phases.


