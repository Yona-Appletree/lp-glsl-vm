# Backend3: Remaining Questions and Open Issues

This document tracks the big remaining questions and open issues for backend3 implementation that need to be resolved during development.

## Critical Questions

### 1. Clobber Computation Algorithm

**Question**: How exactly do we determine which callee-saved registers are "clobbered" (actually used) from regalloc2 results?

**Answer**: Algorithm added to plan (see `compute_clobbered_callee_saved()`):
1. Collect registers that are targets of moves (from regalloc edits)
2. Collect registers that are defs (written to) in instructions
3. Add explicitly clobbered registers from instruction clobber lists
4. Filter to only callee-saved registers

**Status**: ✅ Resolved - Algorithm documented in plan

---

### 2. SP Offset Tracking Details

**Question**: How exactly do we maintain and update `sp_offset` during emission?

**Answer**: Details added to plan:
- `sp_offset` tracks offset from entry SP (before prologue)
- Initialized to 0, set to -8 after setup area, then -(frame_size) after full frame allocation
- Remains constant during function body (SP doesn't change)
- Used to compute SP-relative offsets for stack slots and spills
- Reset to 0 during epilogue

**Status**: ✅ Resolved - Algorithm documented in plan

---

### 3. Block Alignment Strategy

**Question**: When do blocks need alignment, and how do we insert NOPs?

**Answer**: Details added to plan:
- RISC-V 32 instructions are naturally 4-byte aligned
- No special alignment needed for initial implementation
- `MachInst` trait provides `align_basic_block()` (default: no alignment)
- `MachInst` trait provides `gen_nop()` for padding if needed
- Future: Can add alignment for indirect branch targets if needed

**Status**: ✅ Resolved - Strategy documented in plan

---

### 4. Register Allocation Integration Details

**Question**: How exactly do we implement `regalloc2::Function` trait for VCode?

**Context**:
- Need to provide block structure, operands, constraints to regalloc2
- Need to specify register classes and ABI requirements
- Need to handle block parameters correctly

**Details Needed**:
- How to map operand constraints (use/def/modify) from MachInst?
- How to specify register classes for different value types?
- How to handle block parameters in regalloc2?
- What ABI configuration is needed?

**Status**: ⚠️ Needs implementation details

---

### 5. Two-Dest Branch Fallthrough Detection

**Question**: How do we accurately determine which branch target is fallthrough during emission?

**Answer**: Algorithm added to plan:
- `determine_fallthrough()` function checks emission order
- Finds current block position, checks if next block is a target
- Handles cases: one fallthrough, neither fallthrough, both fallthrough
- Integrated into `emit_branch()` function

**Status**: ✅ Resolved - Algorithm documented in plan

---

### 6. VCode Structure Completeness

**Question**: Are we missing any fields in VCode compared to Cranelift's implementation?

**Context**:
- Cranelift's VCode has many fields (source locations, debug tags, stack maps, etc.)
- Need to ensure we have what we need, but not over-engineer

**Fields to Consider**:
- Source locations per instruction (for debugging)?
- Debug tags (if needed)?
- User stack maps (for safepoints/garbage collection)?
- VReg types (for type checking)?

**Status**: ⚠️ Needs review

---

### 7. Constant Materialization Strategy

**Question**: When do we use inline constants vs. lui+addi vs. constant pool?

**Context**:
- Small constants can be inline (12-bit immediates)
- Large constants need lui+addi sequence
- Very large constants might need constant pool (deferred)

**Details Needed**:
- Decision criteria for each strategy
- How to handle 64-bit constants (if needed)?
- When to use constant pool (if implemented)?

**Status**: ⚠️ Needs decision criteria

---

### 8. Source Location Tracking

**Question**: Do we need source location tracking, and if so, how?

**Context**:
- Useful for debugging and error reporting
- Cranelift tracks source locations per instruction
- May not be needed initially

**Details Needed**:
- Do we need this for initial implementation?
- How to track source locations through lowering?
- How to emit source location info in machine code?

**Status**: ❓ Optional - decide if needed

---

## Implementation Questions

### 9. InstBuffer Enhancement Details

**Question**: What exactly needs to be added to InstBuffer for label-based emission?

**Details Needed**:
- Exact API for `emit_branch_with_label()` and `patch_branch()`
- How to store branch instructions with unresolved labels?
- How to patch instructions after emission?
- Do we need to change instruction encoding to support patching?

**Status**: ⚠️ Needs API design

---

### 10. Branch Range Validation

**Question**: How do we validate that branches are within range, and what to do if not?

**Context**:
- Conditional branches: ±4KB
- Unconditional jumps: ±1MB
- Currently assuming < 4KB functions

**Details Needed**:
- When to check branch ranges?
- What to do if out of range? (panic? add veneer? reorder blocks?)
- How to detect this during emission?

**Status**: ⚠️ Needs validation strategy

---

## Notes

- These questions should be resolved before or during implementation
- Some may be answered by studying Cranelift's implementation
- Others may require experimentation and iteration
- Update this document as questions are resolved

