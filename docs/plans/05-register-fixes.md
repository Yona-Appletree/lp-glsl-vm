# Fix Register Management and ABI Implementation (TDD)

## TDD Approach

For each issue, we will:

1. **RED**: Write a failing test that demonstrates the problem
2. **GREEN**: Implement minimal fix to make test pass
3. **REFACTOR**: Clean up implementation while keeping tests green

## Test Infrastructure

We'll use:

- `R5FnTest::from_ir_module()` for high-level function tests
- `test_module()` helper from `caller_saved.rs` for multi-function tests
- Unit tests in `lower.rs` and `frame.rs` for prologue/epilogue logic
- Code inspection tests to verify instruction sequences

## Phase 1: Prologue/Epilogue Issues (TDD)

### Test 1.1: Prologue Adjusts SP Only Once

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_prologue_adjusts_sp_once`

- Create function with calls and callee-saved registers
- Compile and disassemble code
- Verify SP is adjusted exactly once in prologue
- Count `addi sp, sp, -N` instructions in prologue

**Expected failure:** Currently sees two SP adjustments

### Test 1.2: Epilogue Restores in Correct Order

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_epilogue_restores_correct_order`

- Function that uses callee-saved registers and makes calls
- Verify epilogue order: restore callee-saved → adjust SP → restore RA
- Check instruction sequence in epilogue

**Expected failure:** Current order is wrong

### Test 1.3: Prologue Saves Callee-Saved Registers

**File:** `crates/r5-target-riscv32/src/lower.rs` (unit test)

**Test:** `test_prologue_saves_callee_saved_registers`

- Create function that uses s0, s1
- Generate prologue
- Verify `sw s0, offset(sp)` and `sw s1, offset(sp)` are emitted
- Verify offsets are correct

**Expected failure:** May pass, but verify correctness

### Test 1.4: Large Frame Size Handling

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_large_frame_size`

- Function with many callee-saved registers and spill slots
- Frame size > 2047 bytes (exceeds addi immediate range)
- Verify prologue handles large frames correctly
- May need multiple instructions or lui+addi

**Expected failure:** Currently panics on large frames

**Implementation:**

- Fix `gen_prologue()` to adjust SP once
- Fix `gen_epilogue()` order and offsets
- Handle large frame sizes

## Phase 2: Stack Pointer Initialization (TDD)

### Test 2.1: SP Initialized Before Code Execution

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_sp_initialized_before_execution`

- Simple function that uses stack (has frame)
- Run in VM
- Verify SP is valid (not 0) before function executes
- Can check by inspecting VM state or adding debug output

**Expected failure:** SP is 0, causing stack corruption

### Test 2.2: SP Points to Valid Stack Memory

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_sp_points_to_valid_memory`

- Function that writes to stack (spill slots)
- Verify writes succeed without memory errors
- SP should be in RAM region, aligned to 16 bytes

**Expected failure:** SP points to invalid memory or causes crashes

**Implementation:**

- Initialize SP in `VmRunner::run_with_limits()` before creating interpreter
- Set SP to `RAM_OFFSET + ram_size - STACK_SIZE` (e.g., 64KB)
- Ensure 16-byte alignment

## Phase 3: Spill Slot Allocation Conflict (TDD)

### Test 3.1: Call-Site Spills Use Frame Layout Slots

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_call_site_spills_use_frame_slots`

- Function with live values in caller-saved registers
- Makes a call (values must be spilled)
- Verify spilled values use slots from frame layout
- Verify offsets match frame layout computation

**Expected failure:** Spill slots allocated dynamically conflict with frame layout

### Test 3.2: Multiple Calls with Spills

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_multiple_calls_with_spills`

- Function that makes multiple calls
- Each call has different live values
- Verify all spills use frame layout slots correctly
- No stack corruption between calls

**Expected failure:** Stack corruption or wrong offsets

### Test 3.3: Frame Layout Accounts for Call-Site Spills

**File:** `crates/r5-target-riscv32/src/frame.rs` (unit test)

**Test:** `test_frame_layout_includes_call_site_spills`

- Analyze function to determine max live values at call sites
- Verify frame layout reserves slots for call-site spills
- Verify `fixed_frame_storage_size` includes call-site spills

**Expected failure:** Frame layout doesn't account for call-site spills

**Implementation:**

- Modify `analyze_function()` to track max live values at call sites
- Reserve spill slots in frame layout for call-site spills
- Update `lower_call()` to use pre-allocated slots from frame layout
- Remove dynamic slot allocation in `lower_call()`

## Phase 4: Outgoing Arguments Analysis (TDD)

### Test 4.1: Frame Layout Includes Outgoing Args

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_frame_layout_with_many_outgoing_args`

- Function that calls another with >8 arguments
- Verify frame layout includes space for stack arguments
- Verify `outgoing_args_size` is correct

**Expected failure:** Currently hardcoded to 8

### Test 4.2: Multiple Calls with Different Arg Counts

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_multiple_calls_different_arg_counts`

- Function that makes multiple calls with different argument counts
- One call has 5 args, another has 12 args
- Verify frame layout reserves space for max (12 args = 4 stack args)

**Expected failure:** Doesn't analyze actual call sites

**Implementation:**

- Modify `analyze_function()` to track max arguments passed at call sites
- Use this for `outgoing_args_size` computation instead of hardcoded 8

## Phase 5: Integration Tests (TDD)

### Test 5.1: Complex Function with All Features

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_complex_function_all_features`

- Function that:
    - Uses callee-saved registers
    - Makes calls
    - Has register pressure (needs spills)
    - Passes >8 arguments
- Verify correct execution and return value

**Expected failure:** May fail due to any of the above issues

### Test 5.2: Nested Calls with Frame Layouts

**File:** `crates/r5-target-riscv32/tests/caller_saved.rs`

**Test:** `test_nested_calls_with_frame_layouts`

- Multiple levels of function calls
- Each function has its own frame layout
- Verify stack doesn't corrupt between calls
- Verify all frames are properly managed

**Expected failure:** Stack corruption in nested calls

## Implementation Order (TDD Cycle)

### Week 1: Foundation Tests and Fixes

**Day 1: Prologue/Epilogue Tests**

1. Write `test_prologue_adjusts_sp_once` → RED
2. Write `test_epilogue_restores_correct_order` → RED
3. Write `test_prologue_saves_callee_saved_registers` → RED
4. Fix prologue/epilogue → GREEN
5. Refactor

**Day 2: SP Initialization Tests**

1. Write `test_sp_initialized_before_execution` → RED
2. Write `test_sp_points_to_valid_memory` → RED
3. Fix SP initialization → GREEN
4. Refactor

**Day 3: Large Frame Tests**

1. Write `test_large_frame_size` → RED
2. Fix large frame handling → GREEN
3. Refactor

### Week 2: Spill Slot Tests and Fixes

**Day 1-2: Spill Slot Tests**

1. Write `test_call_site_spills_use_frame_slots` → RED
2. Write `test_multiple_calls_with_spills` → RED
3. Write `test_frame_layout_includes_call_site_spills` → RED
4. Analyze requirements

**Day 3-4: Spill Slot Implementation**

1. Modify `analyze_function()` to track call-site spills → GREEN
2. Update frame layout computation → GREEN
3. Fix `lower_call()` to use pre-allocated slots → GREEN
4. Refactor

**Day 5: Integration**

1. Run all tests → GREEN
2. Fix any regressions
3. Refactor

### Week 3: Outgoing Args and Polish

**Day 1: Outgoing Args Tests**

1. Write `test_frame_layout_with_many_outgoing_args` → RED
2. Write `test_multiple_calls_different_arg_counts` → RED
3. Fix outgoing args analysis → GREEN

**Day 2-3: Integration Tests**

1. Write `test_complex_function_all_features` → RED
2. Write `test_nested_calls_with_frame_layouts` → RED
3. Fix any remaining issues → GREEN

**Day 4-5: Polish**

1. Run full test suite
2. Performance testing
3. Documentation

## Test Helper Functions

Add to `crates/r5-target-riscv32/tests/caller_saved.rs`:

```rust
/// Helper to disassemble and inspect prologue
fn inspect_prologue(code: &[u8], expected_sp_adjustments: usize) -> Vec<String> {
    // Disassemble and count SP adjustments
}

/// Helper to verify epilogue order
fn verify_epilogue_order(code: &[u8]) -> bool {
    // Check instruction sequence
}

/// Helper to verify frame layout
fn verify_frame_layout(func: &Function, expected_slots: usize) {
    // Analyze function and verify frame layout
}
```

## Success Criteria

- All new tests pass
- All existing tests still pass
- Prologue adjusts SP exactly once
- Epilogue restores in correct order
- SP is initialized before execution
- Spill slots don't conflict with frame layout
- Outgoing args are analyzed correctly
- No stack corruption in any test case