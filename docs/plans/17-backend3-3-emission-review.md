# Backend3 Phase 3: Emission - Review and Work Plan

**Date**: Current  
**Status**: In Progress  
**Reference Plan**: `docs/plans/17-backend3-3-emission.md`  
**Reference Code**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64`

## Current State Review

### ✅ Implemented Components

#### 1. Emission State Tracking (`isa/riscv32/backend3/emit.rs`)

- ✅ `EmitState` struct with:
  - SP offset tracking
  - Label offsets mapping
  - Pending fixups for branches
  - External relocations
  - Frame size and clobbered registers
  - Current source location
- ✅ Label binding and resolution
- ✅ Forward reference handling (pending fixups)
- ✅ Fixup resolution methods

#### 2. Frame Layout Computation (`isa/riscv32/backend3/emit.rs`)

- ✅ `FrameLayout` struct (in `abi.rs`)
- ✅ `compute_frame_layout()` method:
  - Spill slot counting
  - Clobbered callee-saved register detection
  - Outgoing args area computation
- ✅ `spill_slot_offset()` helper

#### 3. Prologue/Epilogue Generation (`isa/riscv32/backend3/emit.rs`)

- ✅ `gen_prologue()`:
  - Setup area (FP + RA save)
  - Frame size adjustment
  - Callee-saved register saves
- ✅ `gen_epilogue()`:
  - Callee-saved register restore
  - SP restore
  - FP + RA restore
  - Return (JALR)

#### 4. Instruction Emission (`isa/riscv32/backend3/emit.rs`)

- ✅ Block emission order computation (cold blocks at end)
- ✅ Block alignment handling
- ✅ Instruction emission with register allocation application
- ✅ Edit emission (moves, spills, reloads)
- ✅ Source location tracking
- ✅ Branch emission with label resolution
- ✅ Special instruction handling:
  - Traps (Trapz, Trapnz, Trap)
  - System calls (Ecall)
  - Function calls (Jal) with relocation

#### 5. InstBuffer (`isa/riscv32/inst_buffer.rs`)

- ✅ Structured instruction storage
- ✅ Label binding support
- ✅ Branch patching (`patch_branch()`)
- ✅ Source location range tracking
- ✅ Lazy encoding (`as_bytes()`)

#### 6. Tests (`backend3/tests/emission_tests.rs`)

- ✅ Prologue/epilogue tests
- ✅ Instruction emission tests (arithmetic, logical, shifts, load/store)
- ✅ Edit emission tests
- ✅ Branch emission tests
- ✅ End-to-end function tests
- ✅ Trap emission tests (partial)

### ⚠️ Partial Implementation / Issues

#### 1. Function Call Relocation Resolution

**Status**: Implemented but needs verification

- ✅ Relocation recording during emission
- ✅ `fix_external_relocations()` implemented
- ⚠️ **Issue**: Relocation patching logic may have bugs
  - AUIPC/LUI conversion logic is complex
  - PC-relative vs absolute addressing needs testing
  - Symbol table lookup may fail silently

**Action Needed**:

- Add comprehensive tests for function call relocations
- Test both local (PC-relative) and external (absolute) calls
- Verify AUIPC/ADDI sequence is correct

#### 2. Branch Fallthrough Detection

**Status**: Simplified implementation

- ✅ Two-dest branch conversion to single-dest
- ⚠️ **Issue**: Fallthrough detection is simplified
  - Currently assumes false branch is fallthrough if offset matches
  - Should check actual block order instead

**Action Needed**:

- Improve fallthrough detection using block order
- Add tests for various branch patterns

#### 3. Conditional Trap Emission

**Status**: Implemented but patching may be incorrect

- ✅ Trapz/Trapnz emission with conditional branches
- ⚠️ **Issue**: Branch patching for skip offset may be wrong
  - Uses `buffer.cur_offset()` which may not be correct
  - Should compute offset relative to branch instruction

**Action Needed**:

- Fix trap conditional branch patching
- Add tests verifying trap emission correctness

#### 4. System Call Emission

**Status**: Basic implementation

- ✅ Ecall instruction emission
- ✅ Argument/return value handling
- ⚠️ **Issue**: Syscall number handling is TODO
  - Currently assumes constant immediate
  - TODO comment: "Handle case where number is in a register"

**Action Needed**:

- Implement syscall number in register case
- Add tests for syscall emission

#### 5. Source Location Tracking

**Status**: Implemented but not fully utilized

- ✅ Source location ranges tracked in InstBuffer
- ✅ Source location updates during emission
- ⚠️ **Issue**: Not used for debugging/error reporting yet
  - Could be used for better error messages
  - Could be used for debug info generation (future)

**Action Needed**:

- Use source locations in error messages
- Consider debug info generation (deferred)

### ❌ Missing / Not Implemented

#### 1. Out-of-Range Branch Handling

**Status**: Not implemented (deferred per plan)

- ❌ Island/veneer insertion for branches > ±4KB
- ❌ Deadline tracking for forward branches
- **Current**: Panics if branch offset exceeds range

**Action Needed**:

- Implement island insertion (deferred feature)
- For now: Add validation/error messages for out-of-range branches

#### 2. Advanced Branch Optimization

**Status**: Not implemented (deferred per plan)

- ❌ Branch threading (eliminate empty blocks)
- ❌ Latest-branches tracking
- ❌ Conditional branch inversion optimization
- ❌ Unnecessary jump elimination

**Action Needed**:

- Deferred (as per plan)

#### 3. Function Call Tests

**Status**: Test stub exists but not implemented

- ❌ `test_emit_function_call()` is empty
- ❌ No tests for:
  - Argument passing (registers + stack)
  - Return value handling
  - Relocation resolution
  - Multiple function calls

**Action Needed**:

- Implement comprehensive function call tests
- Test with symbol table resolution

#### 4. System Call Tests

**Status**: Test stub exists but not implemented

- ❌ `test_emit_syscall()` is empty
- ❌ No tests for syscall emission

**Action Needed**:

- Implement syscall emission tests

#### 5. Frame Layout Edge Cases

**Status**: Basic implementation, edge cases not tested

- ❌ No tests for:
  - Large frame sizes
  - Many callee-saved registers
  - Many spill slots
  - Large outgoing args area
  - Frame size alignment requirements

**Action Needed**:

- Add tests for frame layout edge cases
- Verify frame size calculations are correct

#### 6. Edit Emission Edge Cases

**Status**: Basic implementation, edge cases not tested

- ❌ No tests for:
  - Multiple spills/reloads in sequence
  - Spills/reloads with different slot indices
  - Moves between many registers
  - Edit ordering correctness

**Action Needed**:

- Add tests for edit emission edge cases
- Verify edit ordering matches regalloc output

#### 7. Block Alignment

**Status**: Implemented but not tested

- ✅ Block alignment handling in emission
- ❌ No tests for alignment requirements
- ❌ No tests for alignment padding correctness

**Action Needed**:

- Add tests for block alignment
- Verify padding instructions are correct

#### 8. Error Handling

**Status**: Basic, may panic in edge cases

- ⚠️ Some edge cases may panic instead of returning errors
- ⚠️ Unresolved labels panic (should be caught earlier)
- ⚠️ Invalid allocations panic (should be validated)

**Action Needed**:

- Add validation/error handling for edge cases
- Return errors instead of panicking where possible

## Work Plan

### Priority 1: Critical Fixes and Tests

#### 1.1 Fix Conditional Trap Branch Patching

**File**: `isa/riscv32/backend3/emit.rs`
**Issue**: Trap conditional branch patching uses wrong offset
**Fix**:

```rust
// Current (wrong):
let skip_offset = buffer.cur_offset();
buffer.patch_branch(branch_inst_idx, skip_offset, BranchType::Conditional);

// Should be:
let branch_offset = (branch_inst_idx * 4) as u32;
let skip_offset = branch_offset + 4; // Skip EBREAK (4 bytes)
buffer.patch_branch(branch_inst_idx, skip_offset, BranchType::Conditional);
```

**Tests**: Add tests verifying trap emission correctness

#### 1.2 Implement Function Call Tests

**File**: `backend3/tests/emission_tests.rs`
**Tasks**:

- Test function call with register arguments (a0-a7)
- Test function call with stack arguments (>8 args)
- Test function call return value handling
- Test local function call (PC-relative relocation)
- Test external function call (absolute relocation)
- Test multiple function calls in one function
- Test function call with symbol table resolution

**Reference**: Use Cranelift's function call tests as reference

#### 1.3 Implement System Call Tests

**File**: `backend3/tests/emission_tests.rs`
**Tasks**:

- Test syscall with constant number
- Test syscall with register number (when implemented)
- Test syscall argument passing
- Test syscall return value handling

#### 1.4 Fix Branch Fallthrough Detection

**File**: `isa/riscv32/backend3/emit.rs`
**Issue**: Simplified fallthrough detection
**Fix**: Use block order to determine fallthrough

```rust
fn determine_fallthrough(
    &self,
    current_block: BlockIndex,
    target_true: BlockIndex,
    target_false: BlockIndex,
    block_order: &[BlockIndex],
) -> (BlockIndex, bool) {
    // Find current block index in order
    let current_idx = block_order.iter().position(|&b| b == current_block)?;
    let next_block = block_order.get(current_idx + 1)?;

    if *next_block == target_false {
        (target_true, false) // False is fallthrough, branch to true
    } else {
        (target_false, true) // True is fallthrough, branch to false (invert)
    }
}
```

**Tests**: Add tests for various branch patterns

### Priority 2: Test Coverage Expansion

#### 2.1 Frame Layout Edge Case Tests

**File**: `backend3/tests/emission_tests.rs`
**Tests**:

- `test_frame_layout_large_frame()` - Test with many spill slots
- `test_frame_layout_many_callee_saved()` - Test with many clobbered registers
- `test_frame_layout_large_outgoing_args()` - Test with many stack arguments
- `test_frame_layout_alignment()` - Test frame size alignment

#### 2.2 Edit Emission Edge Case Tests

**File**: `backend3/tests/emission_tests.rs`
**Tests**:

- `test_edit_multiple_spills()` - Test multiple spills in sequence
- `test_edit_multiple_reloads()` - Test multiple reloads in sequence
- `test_edit_spill_reload_ordering()` - Test edit ordering correctness
- `test_edit_many_reg_moves()` - Test many register moves

#### 2.3 Block Alignment Tests

**File**: `backend3/tests/emission_tests.rs`
**Tests**:

- `test_block_alignment_4_bytes()` - Test 4-byte alignment
- `test_block_alignment_8_bytes()` - Test 8-byte alignment
- `test_block_alignment_16_bytes()` - Test 16-byte alignment
- `test_block_alignment_padding()` - Verify padding instructions

#### 2.4 Branch Pattern Tests

**File**: `backend3/tests/emission_tests.rs`
**Tests**:

- `test_branch_fallthrough_true()` - Test true branch is fallthrough
- `test_branch_fallthrough_false()` - Test false branch is fallthrough
- `test_branch_no_fallthrough()` - Test neither branch is fallthrough
- `test_branch_backward()` - Test backward branches
- `test_branch_forward()` - Test forward branches
- `test_branch_long_range()` - Test branches near range limit

### Priority 3: Improvements and Edge Cases

#### 3.1 Improve Error Handling

**File**: `isa/riscv32/backend3/emit.rs`
**Tasks**:

- Add validation for unresolved labels before final fixup resolution
- Add validation for invalid register allocations
- Return errors instead of panicking where possible
- Add error messages with source locations

#### 3.2 Implement Syscall Number in Register

**File**: `isa/riscv32/backend3/emit.rs`
**Issue**: TODO for syscall number in register
**Fix**: Handle case where syscall number is in a register

```rust
match number {
    SyscallNumber::Constant(n) => {
        buffer.push_addi(Gpr::A7, Gpr::Zero, *n);
    }
    SyscallNumber::Register(reg) => {
        let reg_gpr = self.reg_to_gpr(*reg);
        if reg_gpr != Gpr::A7 {
            buffer.push_addi(Gpr::A7, reg_gpr, 0);
        }
    }
}
```

**Tests**: Add test for syscall number in register

#### 3.3 Add Out-of-Range Branch Validation

**File**: `isa/riscv32/backend3/emit.rs`
**Tasks**:

- Add validation for branch offset ranges
- Return error instead of panic for out-of-range branches
- Add helpful error message with branch distance

#### 3.4 Improve Source Location Usage

**File**: `isa/riscv32/backend3/emit.rs`
**Tasks**:

- Use source locations in error messages
- Add source location to panic messages
- Consider debug info generation (deferred)

### Priority 4: Deferred Features (Future)

#### 4.1 Out-of-Range Branch Handling

- Island/veneer insertion
- Deadline tracking
- Support for functions > 4KB

#### 4.2 Advanced Branch Optimization

- Branch threading
- Latest-branches tracking
- Conditional branch inversion
- Unnecessary jump elimination

#### 4.3 Debug Information

- Value label ranges
- Debug tags
- CFG metadata
- DWARF debug info generation

## Test Plan

### Test Categories

#### Unit Tests (Emission Components)

1. **EmitState Tests**

   - Label binding and resolution
   - Fixup recording and resolution
   - Source location tracking

2. **FrameLayout Tests**

   - Frame size computation
   - Spill slot offset calculation
   - Clobbered register detection

3. **Prologue/Epilogue Tests**
   - Prologue instruction sequence
   - Epilogue instruction sequence
   - Frame size handling
   - Callee-saved register handling

#### Integration Tests (End-to-End)

1. **Simple Function Tests**

   - Single block functions
   - Multiple block functions
   - Functions with branches
   - Functions with loops

2. **Function Call Tests**

   - Direct calls (local)
   - Indirect calls (external)
   - Calls with many arguments
   - Calls with return values

3. **System Call Tests**

   - Syscall emission
   - Syscall argument handling
   - Syscall return value handling

4. **Edge Case Tests**
   - Large frames
   - Many spills/reloads
   - Complex control flow
   - Alignment requirements

### Test Format

Following the plan document's guidelines:

- **Input**: Textual LPIR format for clarity
- **Expected Output**: Assembler format showing expected machine code
- **Verification**: Check instruction sequences, not just success

### Example Test Structure

```rust
#[test]
fn test_function_call_with_stack_args() {
    // Input: textual LPIR format
    let lpir_text = r#"
        function %test(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32 {
        block0(v0: i32, v1: i32, v2: i32, v3: i32, v4: i32, v5: i32, v6: i32, v7: i32, v8: i32, v9: i32):
            v10 = call @other(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9)
            return v10
        }
    "#;

    let test = LowerTest::from_lpir(lpir_text);
    let vcode = test.vcode();
    let regalloc = vcode.run_regalloc().expect("regalloc should succeed");

    // Create symbol table with other function
    let mut symtab = SymbolTable::new();
    symtab.add_local(Symbol::local("other"), 0x1000);

    let buffer = vcode.emit(&regalloc, Some(&mut symtab), Some("test"));

    // Expected: assembler format
    // Verify:
    // 1. First 8 args in a0-a7
    // 2. Args 9-10 on stack (outgoing args area)
    // 3. AUIPC + ADDI + JALR sequence
    // 4. Return value in a0
    // ...
}
```

## Reference Implementation Notes

### Cranelift RISC-V 64 Emission (`wasmtime/cranelift/codegen/src/isa/riscv64`)

Key differences and similarities:

- **Similar**: Label-based emission, forward reference handling
- **Similar**: Frame layout computation
- **Similar**: Prologue/epilogue generation
- **Different**: RISC-V 32 vs 64 (register sizes, some instructions)
- **Different**: Cranelift uses byte-patching, we use structured instructions
- **Different**: Cranelift has island insertion, we defer it

### Key Files to Reference

- `inst/emit.rs` - Instruction emission
- `abi.rs` - Frame layout and ABI helpers
- `buffer.rs` - MachBuffer with EmitState (if exists)

## Success Criteria

### Phase 3 Completion Checklist

- [x] Can emit prologue/epilogue
- [x] Can emit instructions with allocated registers
- [x] Can emit edits (moves, spills, reloads)
- [x] Can compile simple function end-to-end
- [ ] Generated code executes correctly (needs execution tests)
- [ ] Function calls work correctly (needs tests)
- [ ] System calls work correctly (needs tests)
- [ ] Edge cases handled gracefully (needs tests)

### Test Coverage Goals

- [ ] 80%+ code coverage for emission module
- [ ] All instruction types tested
- [ ] All edit types tested
- [ ] All branch patterns tested
- [ ] Function call scenarios tested
- [ ] System call scenarios tested
- [ ] Edge cases tested

## Next Steps

1. **Immediate** (Priority 1):

   - Fix conditional trap branch patching
   - Implement function call tests
   - Implement system call tests
   - Fix branch fallthrough detection

2. **Short-term** (Priority 2):

   - Expand test coverage
   - Add edge case tests
   - Add block alignment tests

3. **Medium-term** (Priority 3):

   - Improve error handling
   - Implement syscall number in register
   - Add out-of-range branch validation

4. **Long-term** (Priority 4):
   - Implement deferred features (islands, optimizations)
   - Add debug info generation
