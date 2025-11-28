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

**Answer**: Complete implementation added to plan:
- All required trait methods implemented
- Operand collection via `MachInst::get_operands()` during lowering
- Block structure provided (entry, succs, preds, params)
- Branch arguments handled via `branch_blockparams()`
- Clobbers provided via `inst_clobbers()`
- ABI machine spec provided via `VCode::abi.machine_env()`

**Status**: ✅ Resolved - Complete implementation documented in plan

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

**Answer**: Review completed, comparison added to plan:
- **Included**: All required fields for basic functionality (insts, operands, blocks, abi, etc.)
- **Deferred**: Optional fields (vreg_types, srclocs, debug_tags, user_stack_maps, debug_value_labels, facts)
- **Recommendation**: Start with included fields, add `vreg_types` if needed for validation, add others later if needed

**Status**: ✅ Resolved - Structure reviewed and documented in plan

---

### 7. Constant Materialization Strategy

**Question**: When do we use inline constants vs. lui+addi vs. constant pool?

**Answer**: Decision criteria added to plan:
- **Inline**: If `value >= -2048 && value <= 2047` (12-bit signed)
- **LUI + ADDI**: If `value < -2048 || value > 2047` (full 32-bit range)
- **Constant Pool**: Deferred (not needed for initial implementation)
- Special cases: Zero uses `x0`, small positives use `addi rd, x0, imm`
- 64-bit constants: Deferred (would need constant pool)

**Status**: ✅ Resolved - Decision criteria documented in plan

---

### 8. Source Location Tracking

**Question**: Do we need source location tracking, and if so, how?

**Answer**: Decision - **Deferred for initial implementation**
- Not needed for correctness
- Can be added later if debugging/error reporting requires it
- Would add `srclocs: Vec<RelSourceLoc>` field to VCode
- Track through lowering, emit during code generation
- See deferred features document for details

**Status**: ✅ Resolved - Decision: Defer to later

---

## Implementation Questions

### 9. InstBuffer Enhancement Details

**Question**: What exactly needs to be added to InstBuffer for label-based emission?

**Answer**: API design added to plan:
- `cur_offset()`: Get current code offset in bytes
- `emit_branch_with_label()`: Emit branch with placeholder offset, returns instruction index
- `patch_branch()`: Patch branch instruction at given index with computed offset
- Uses structured instructions (patch `imm` field directly)
- RISC-V offsets are in 2-byte units
- Conditional: ±4KB, Unconditional: ±1MB
- Panics if out of range (assumes < 4KB functions)

**Status**: ✅ Resolved - API design documented in plan

---

### 10. Branch Range Validation

**Question**: How do we validate that branches are within range, and what to do if not?

**Answer**: Validation strategy added to plan:
- Check during `patch_branch()` - assert if out of range
- Conditional branches: assert ±4KB range
- Unconditional jumps: assert ±1MB range
- Currently: Panic if out of range (assumes < 4KB functions)
- Future: Can add veneer/island insertion (deferred feature)

**Status**: ✅ Resolved - Validation strategy documented in plan

---

## Notes

- These questions should be resolved before or during implementation
- Some may be answered by studying Cranelift's implementation
- Others may require experimentation and iteration
- Update this document as questions are resolved

