# Register Management and ABI Implementation Plan (Cranelift-Aligned)

## Overview

This plan outlines the implementation of proper register management and ABI compliance for the RISC-V 32-bit compiler, aligned with **Cranelift's architecture**. We'll implement a stack-based register management system with frame layout computation, prologue/epilogue generation, and register allocator integration.

## Key Principles (from Cranelift)

1. **Frame Layout Pre-computation**: Compute complete frame layout before code generation
2. **Stack-Based Saving**: All register saving uses the stack, not callee-saved registers as temporary storage
3. **Prologue/Epilogue**: Save callee-saved registers once in prologue, restore in epilogue
4. **Register Allocator Integration**: Allocator handles caller-saved register spilling automatically
5. **Clobber Tracking**: Track which registers are clobbered by calls

## Current State

The compiler currently has several limitations:

1. **Frame layout**: Not computed - no prologue/epilogue
2. **Callee-saved registers**: Not saved to stack in prologue
3. **Caller-saved registers**: Manual save/restore using callee-saved registers (incorrect)
4. **Stack pointer**: Not initialized, not used for register saving
5. **Register spilling**: Panics when out of registers
6. **Multiple return values**: Only first return value handled

## RISC-V 32-bit Calling Convention

### Register Classification

**Caller-saved (temporary) registers:**

- `t0-t6` (x5-x7, x28-x31): May be modified by callee
- `a0-a7` (x10-x17): Argument/return registers, may be modified
- `ra` (x1): Return address (caller-saved)

**Callee-saved (saved) registers:**

- `s0-s11` (x8-x9, x18-x27): Must be preserved by callee
- `sp` (x2): Stack pointer
- `fp` (x8/s0): Frame pointer (if used)

**Special registers:**

- `zero` (x0): Always zero
- `gp` (x3): Global pointer (if used)
- `tp` (x4): Thread pointer (if used)

### Argument Passing

- First 8 arguments: `a0-a7` (x10-x17)
- Additional arguments: Stack (grows downward)
- Return values: `a0-a7` (x10-x17), additional on stack

## Architecture Overview (Cranelift Model)

```
┌─────────────────────────────────────┐
│  Caller's Stack Frame              │
├─────────────────────────────────────┤
│  Outgoing Arguments (if any)       │  ← SP (after call)
├─────────────────────────────────────┤
│  Spill Slots                        │
├─────────────────────────────────────┤
│  Clobber Area (callee-saved regs)  │
├─────────────────────────────────────┤
│  Setup Area (FP/LR)                 │  ← FP (if used)
├─────────────────────────────────────┤
│  Incoming Arguments (if any)       │
└─────────────────────────────────────┘
```

**Frame Layout Components:**

1. **Setup Area**: FP and LR save area (16 bytes if needed)
2. **Clobber Area**: Callee-saved registers that need saving
3. **Fixed Frame Storage**: Spill slots for register allocation
4. **Outgoing Arguments**: Space for arguments passed on stack

## Phase 1: Frame Layout Computation

**Goal:** Pre-compute complete frame layout before code generation.

### 1.1 Frame Layout Structure

**Test Location:** `crates/r5-target-riscv32/src/frame.rs` (new file)

```rust
pub struct FrameLayout {
    /// Word size in bytes (4 for RISC-V 32-bit)
    pub word_bytes: u32,

    /// Size of incoming arguments on stack (if > 8 args)
    pub incoming_args_size: u32,

    /// Size of setup area (FP/LR save area, 0 or 8 bytes)
    pub setup_area_size: u32,

    /// Size of clobber area (callee-saved registers)
    pub clobber_size: u32,

    /// Size of fixed frame storage (spill slots)
    pub fixed_frame_storage_size: u32,

    /// Size of outgoing arguments area
    pub outgoing_args_size: u32,

    /// List of callee-saved registers that need saving
    pub clobbered_callee_saves: Vec<Gpr>,

    /// Whether function makes calls
    pub has_function_calls: bool,
}
```

**Implementation:**

- Track which callee-saved registers are used by register allocator
- Compute sizes for each area
- Align to 16 bytes (RISC-V ABI requirement)

### 1.2 Compute Frame Layout

**Test:** `test_compute_frame_layout_no_calls`
**Test:** `test_compute_frame_layout_with_calls`
**Test:** `test_compute_frame_layout_with_spills`

**Implementation:**

```rust
impl FrameLayout {
    pub fn compute(
        used_callee_saved: &[Gpr],
        spill_slots: usize,
        has_calls: bool,
        incoming_args: usize,
        outgoing_args: usize,
    ) -> Self {
        // Determine if setup area needed
        let setup_area_size = if has_calls || !used_callee_saved.is_empty() || spill_slots > 0 {
            8 // FP/LR save area (8 bytes for RISC-V 32-bit)
        } else {
            0
        };

        // Compute clobber size (callee-saved registers)
        let clobber_size = align_to_16(used_callee_saved.len() * 4);

        // Compute fixed frame storage (spill slots)
        let fixed_frame_storage_size = align_to_16(spill_slots * 4);

        // Compute outgoing args size
        let outgoing_args_size = if outgoing_args > 8 {
            align_to_16((outgoing_args - 8) * 4)
        } else {
            0
        };

        FrameLayout { ... }
    }
}
```

## Phase 2: Prologue and Epilogue Generation

**Goal:** Generate function prologue and epilogue to save/restore callee-saved registers.

### 2.1 Prologue Generation

**Test:** `test_prologue_saves_callee_saved_registers`
**Test:** `test_prologue_saves_fp_lr`
**Test:** `test_prologue_adjusts_sp`

**Implementation:**

```rust
fn gen_prologue(
    code: &mut CodeBuffer,
    frame_layout: &FrameLayout,
) {
    if frame_layout.setup_area_size > 0 {
        // Save FP and LR
        // addi sp, sp, -8
        // sw ra, 4(sp)
        // sw fp, 0(sp)  (if FP used)
        // addi fp, sp, 0  (if FP used)
    }

    // Adjust SP for entire frame
    let total_size = frame_layout.setup_area_size
        + frame_layout.clobber_size
        + frame_layout.fixed_frame_storage_size
        + frame_layout.outgoing_args_size;

    if total_size > 0 {
        // addi sp, sp, -total_size
    }

    // Save callee-saved registers
    let mut offset = frame_layout.setup_area_size;
    for reg in &frame_layout.clobbered_callee_saves {
        // sw reg, offset(sp)
        offset += 4;
    }
}
```

### 2.2 Epilogue Generation

**Test:** `test_epilogue_restores_callee_saved_registers`
**Test:** `test_epilogue_restores_fp_lr`
**Test:** `test_epilogue_adjusts_sp`

**Implementation:**

```rust
fn gen_epilogue(
    code: &mut CodeBuffer,
    frame_layout: &FrameLayout,
) {
    // Restore callee-saved registers (reverse order)
    let mut offset = frame_layout.setup_area_size;
    for reg in frame_layout.clobbered_callee_saves.iter().rev() {
        // lw reg, offset(sp)
        offset += 4;
    }

    // Restore SP
    let total_size = frame_layout.setup_area_size
        + frame_layout.clobber_size
        + frame_layout.fixed_frame_storage_size
        + frame_layout.outgoing_args_size;

    if total_size > 0 {
        // addi sp, sp, total_size
    }

    if frame_layout.setup_area_size > 0 {
        // Restore FP and LR
        // lw fp, 0(sp)
        // lw ra, 4(sp)
        // addi sp, sp, 8
    }
}
```

## Phase 3: Stack Pointer Initialization

**Goal:** Initialize SP in bootstrap code and ensure it's valid for all functions.

### 3.1 Bootstrap SP Initialization

**Test:** `test_bootstrap_initializes_sp`
**Test:** `test_bootstrap_sp_is_valid`

**Implementation:**

- Initialize SP to a valid stack address in bootstrap function
- Use a fixed address (e.g., 0x80000000 + RAM_SIZE - STACK_SIZE)
- Or allocate stack space and set SP

### 3.2 SP Validation

**Test:** `test_sp_is_valid_for_all_functions`

**Implementation:**

- Ensure SP is initialized before any function call
- Validate SP is within valid memory range

## Phase 4: Register Allocator Integration

**Goal:** Integrate register allocator with frame layout and spilling.

### 4.1 Track Callee-Saved Register Usage

**Test:** `test_allocator_tracks_callee_saved_usage`
**Test:** `test_allocator_reports_used_callee_saved`

**Implementation:**

```rust
impl SimpleRegAllocator {
    /// Track which callee-saved registers are in use
    pub fn get_used_callee_saved(&self) -> Vec<Gpr> {
        self.value_to_reg
            .values()
            .filter(|&&reg| Self::is_callee_saved(reg))
            .copied()
            .collect()
    }

    fn is_callee_saved(reg: Gpr) -> bool {
        matches!(reg.num(), 8..=9 | 18..=27) // s0-s11
    }
}
```

### 4.2 Spill Slot Management

**Test:** `test_allocator_allocates_spill_slots`
**Test:** `test_allocator_spills_to_stack`
**Test:** `test_allocator_reloads_from_stack`

**Implementation:**

```rust
impl SimpleRegAllocator {
    /// Allocate a spill slot for a value
    pub fn spill(&mut self, value: Value, frame_layout: &FrameLayout) -> u32 {
        let slot = self.next_spill_slot;
        self.spill_slots.insert(value, slot);
        self.next_spill_slot += 1;

        // Update frame layout if needed
        // ...

        slot
    }

    /// Get stack offset for spill slot
    pub fn spill_slot_offset(&self, slot: u32, frame_layout: &FrameLayout) -> i32 {
        let base_offset = frame_layout.setup_area_size + frame_layout.clobber_size;
        -(base_offset + slot * 4) as i32
    }
}
```

### 4.3 Caller-Saved Register Spilling

**Test:** `test_allocator_spills_caller_saved_before_call`
**Test:** `test_allocator_reloads_after_call`

**Implementation:**

- When out of registers, spill caller-saved registers to stack
- Use spill slots from frame layout
- Reload after call if still live

## Phase 5: Call Site Handling

**Goal:** Properly handle function calls with clobber tracking.

### 5.1 Clobber Tracking

**Test:** `test_call_tracks_clobbers`
**Test:** `test_call_spills_live_values`

**Implementation:**

```rust
fn lower_call(
    &mut self,
    code: &mut CodeBuffer,
    callee: &str,
    args: &[Value],
    results: &[Value],
    frame_layout: &FrameLayout,
) {
    // Get clobbered registers (caller-saved)
    let clobbers = get_clobbered_registers();

    // Find live values in clobbered registers
    let live_values = find_live_values_in_clobbers(clobbers);

    // Spill live values to stack
    for value in live_values {
        let slot = self.regalloc.spill(value, frame_layout);
        let offset = self.regalloc.spill_slot_offset(slot, frame_layout);
        // sw reg, offset(sp)
    }

    // Set up arguments
    // ...

    // Make call
    // ...

    // Reload spilled values
    for value in live_values.iter().rev() {
        let slot = self.regalloc.get_spill_slot(value);
        let offset = self.regalloc.spill_slot_offset(slot, frame_layout);
        // lw reg, offset(sp)
    }
}
```

### 5.2 Return Address Handling

**Test:** `test_call_saves_ra`
**Test:** `test_call_restores_ra`

**Implementation:**

- `ra` is caller-saved, so save it before call if we need to return
- Save to stack (spill slot) or use frame layout
- Restore after call

## Phase 6: Multiple Return Values

**Goal:** Support functions returning multiple values (a0-a7).

### 6.1 Return Value Handling

**Test:** `test_function_returns_two_values`
**Test:** `test_function_returns_eight_values`
**Test:** `test_call_site_extracts_multiple_returns`

**Implementation:**

```rust
fn lower_return(
    &mut self,
    code: &mut CodeBuffer,
    values: &[Value],
) {
    // Move return values to a0-a7
    for (i, value) in values.iter().take(8).enumerate() {
        let ret_reg = match i {
            0 => Gpr::A0,
            1 => Gpr::A1,
            // ... up to A7
        };
        let value_reg = self.regalloc.get(*value).unwrap();
        if value_reg.num() != ret_reg.num() {
            code.emit(lpc_riscv32::add(ret_reg, value_reg, Gpr::ZERO));
        }
    }

    // Return
    code.emit(lpc_riscv32::jalr(Gpr::ZERO, Gpr::RA, 0));
}
```

## Phase 7: Integration and Testing

**Goal:** Integrate all components and test end-to-end.

### 7.1 End-to-End Tests

**Test:** `test_nested_calls_with_frame_layout`
**Test:** `test_function_with_spills_and_calls`
**Test:** `test_complex_function_with_many_registers`

### 7.2 Performance Tests

**Test:** Measure code size impact
**Test:** Measure performance impact

## Implementation Order

### Week 1: Foundation

**Day 1-2: Frame Layout**

1. ✅ Create `FrameLayout` structure
2. ✅ Implement `compute_frame_layout()`
3. ✅ Write tests for frame layout computation

**Day 3-4: Prologue/Epilogue**

1. ✅ Implement `gen_prologue()`
2. ✅ Implement `gen_epilogue()`
3. ✅ Write tests for prologue/epilogue

**Day 5: SP Initialization**

1. ✅ Initialize SP in bootstrap
2. ✅ Test SP initialization

### Week 2: Register Allocator Integration

**Day 1-2: Callee-Saved Tracking**

1. ✅ Track callee-saved register usage
2. ✅ Report used callee-saved registers
3. ✅ Tests

**Day 3-4: Spill Slot Management**

1. ✅ Implement spill slot allocation
2. ✅ Implement spill/reload to stack
3. ✅ Tests

**Day 5: Caller-Saved Spilling**

1. ✅ Spill caller-saved registers before calls
2. ✅ Reload after calls
3. ✅ Tests

### Week 3: Call Handling and Multiple Returns

**Day 1-2: Call Site Handling**

1. ✅ Implement clobber tracking
2. ✅ Implement save/restore around calls
3. ✅ Tests

**Day 3: Multiple Return Values**

1. ✅ Handle multiple return values
2. ✅ Tests

**Day 4-5: Integration and Polish**

1. ✅ End-to-end tests
2. ✅ Performance testing
3. ✅ Documentation

## Success Criteria

**Phase 1 complete when:**

- ✅ Frame layout is computed before code generation
- ✅ All frame components are correctly sized
- ✅ Tests pass

**Phase 2 complete when:**

- ✅ Prologue saves FP/LR and callee-saved registers
- ✅ Epilogue restores everything correctly
- ✅ SP is properly adjusted
- ✅ Tests pass

**Phase 3 complete when:**

- ✅ SP is initialized in bootstrap
- ✅ SP is valid for all functions
- ✅ Tests pass

**Phase 4 complete when:**

- ✅ Allocator tracks callee-saved usage
- ✅ Spill slots are allocated correctly
- ✅ Spill/reload works correctly
- ✅ Tests pass

**Phase 5 complete when:**

- ✅ Calls properly handle clobbers
- ✅ Live values are spilled before calls
- ✅ Values are reloaded after calls
- ✅ Tests pass

**Phase 6 complete when:**

- ✅ Functions can return multiple values
- ✅ Call sites extract multiple return values
- ✅ Tests pass

**Phase 7 complete when:**

- ✅ All end-to-end tests pass
- ✅ Performance is acceptable
- ✅ Code is well-documented

## Key Differences from Previous Approach

1. **Stack-Based**: All register saving uses the stack, not callee-saved registers
2. **Frame Layout**: Pre-computed before code generation
3. **Prologue/Epilogue**: Generated once per function, not per call
4. **Register Allocator**: Handles spilling automatically, integrated with frame layout
5. **Clobber Tracking**: Explicit tracking of which registers are clobbered

## References

- Cranelift ABI Implementation: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/abi.rs`
- Cranelift Frame Layout: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/machinst/abi.rs`
- RISC-V Calling Convention: [RISC-V ELF psABI specification](https://github.com/riscv-non-isa/riscv-elf-psabi-doc)
