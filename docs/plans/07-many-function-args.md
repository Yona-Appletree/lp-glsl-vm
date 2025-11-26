# Stack Arguments and Return Values Plan

## Overview

Implement support for functions with more than 8 arguments and/or more than 8 return values in the RISC-V 32-bit compiler, following the RISC-V calling convention and aligned with **Cranelift's RISC-V 64 implementation**.

## Current State

The compiler currently has partial support:

1. **Frame Layout**: Already computes `incoming_args_size` and `outgoing_args_size` ✅
2. **ABI Module**: Knows which args go on stack (returns `None` for index > 7) ✅
3. **Missing**:

    - Tracking which parameters/returns are on stack in `AbiInfo`
    - Computing actual outgoing args from call sites (hardcoded to 8)
    - Loading incoming stack args in prologue
    - Storing outgoing stack args at call sites
    - Handling stack return values

## RISC-V 32-bit Calling Convention (Stack Arguments)

### Argument Passing

- **First 8 arguments**: Passed in registers `a0-a7` (x10-x17)
- **Additional arguments**: Passed on the stack
    - Stack arguments start at `SP + 0` (first stack arg) in caller's frame
    - Each argument is 4 bytes (word-aligned)
    - Stack grows downward, but arguments are at positive offsets from caller's SP

### Return Value Passing

- **First 8 return values**: Returned in registers `a0-a7` (x10-x17)
- **Additional return values**: Returned on the stack
    - Caller allocates space for return values
    - Callee stores return values at the address passed (if any)
    - For simplicity, we'll use a similar approach to arguments

### Stack Layout (Cranelift Reference)

```
┌─────────────────────────────────────┐
│  Caller's Stack Frame               │
│  (before call)                      │
├─────────────────────────────────────┤
│  Outgoing Arguments (stack args)    │  ← SP (caller's SP before call)
│  [arg8, arg9, ...]                  │
├─────────────────────────────────────┤
│  ... caller's frame ...             │
└─────────────────────────────────────┘
         ↓ (after call)
┌─────────────────────────────────────┐
│  Callee's Stack Frame               │
│  (after call)                       │
├─────────────────────────────────────┤
│  Outgoing Arguments (for next call) │  ← SP (callee's SP)
├─────────────────────────────────────┤
│  Spill Slots                        │
├─────────────────────────────────────┤
│  Clobber Area (callee-saved regs)   │
├─────────────────────────────────────┤
│  Setup Area (FP/LR)                 │
├─────────────────────────────────────┤
│  Incoming Arguments (stack args)    │  ← Above callee's frame
│  [arg8, arg9, ...]                  │     (in caller's frame)
└─────────────────────────────────────┘
```

**Key Insight from Cranelift** (`/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/abi.rs`):

- Incoming stack arguments are accessed at positive offsets from SP **before** the prologue adjusts SP
- After prologue, incoming args are at `SP + frame_size + offset`
- Outgoing stack arguments are stored in the outgoing args area at negative offsets from SP
- Stack arguments use `AMode::IncomingArg(offset)` and `AMode::SPOffset(offset)` addressing modes

## Implementation Plan

### Phase 1: Enhance AbiInfo to Track Stack Arguments

**File**: `crates/r5-target-riscv32/src/abi.rs`

**Changes**:

1. Add fields to `AbiInfo`:
   ```rust
   /// Parameter -> stack offset mapping (for stack args, index >= 8)
   /// Offset is relative to SP before prologue (positive offset)
   pub param_stack_offsets: alloc::collections::BTreeMap<usize, i32>,
   
   /// Return value -> stack offset mapping (for stack returns, index >= 8)
   /// Offset is relative to SP before prologue (positive offset)
   pub return_stack_offsets: alloc::collections::BTreeMap<usize, i32>,
   ```

2. Update `compute_abi_info` to populate these maps:

    - For parameters with index >= 8, compute stack offset: `(index - 8) * 4`
    - For return values with index >= 8, compute stack offset: `(index - 8) * 4`

**Tests**:

- `test_abi_info_tracks_stack_params`
- `test_abi_info_tracks_stack_returns`
- `test_abi_info_mixed_reg_and_stack`

### Phase 2: Compute Actual Outgoing Arguments

**File**: `crates/r5-target-riscv32/src/lib.rs`

**Changes**:

1. Add helper function to analyze function for max outgoing args:
   ```rust
   fn compute_max_outgoing_args(func: &Function, module: &Module) -> usize {
       let mut max_args = 0;
       for block in &func.blocks {
           for inst in &block.insts {
               if let Inst::Call { callee, args, .. } = inst {
                   // Look up callee signature in module
                   if let Some(callee_func) = module.functions.get(callee) {
                       max_args = max_args.max(callee_func.signature.params.len());
                   }
                   // Also check direct args count
                   max_args = max_args.max(args.len());
               }
           }
       }
       max_args
   }
   ```

2. Replace hardcoded `8` in `compile_module` with call to `compute_max_outgoing_args`

**Tests**:

- `test_compute_max_outgoing_args_single_call`
- `test_compute_max_outgoing_args_multiple_calls`
- `test_compute_max_outgoing_args_nested_calls`

### Phase 3: Add Frame Layout Helpers for Stack Arguments

**File**: `crates/r5-target-riscv32/src/frame.rs`

**Changes**:

Add helper methods to `FrameLayout`:

```rust
impl FrameLayout {
    /// Get stack offset for incoming argument (index >= 8)
    /// 
    /// Returns offset relative to SP **before** prologue (positive offset).
    /// After prologue, the actual offset is: total_size() + offset
    pub fn incoming_arg_offset(&self, arg_index: usize) -> Option<i32> {
        if arg_index < 8 {
            return None; // In register
        }
        let stack_index = arg_index - 8;
        Some((stack_index * 4) as i32)
    }
    
    /// Get stack offset for outgoing argument (index >= 8)
    /// 
    /// Returns offset relative to SP (negative, like other frame slots)
    pub fn outgoing_arg_offset(&self, arg_index: usize) -> Option<i32> {
        if arg_index < 8 {
            return None; // In register
        }
        let stack_index = arg_index - 8;
        let base = self.setup_area_size 
   + self.clobber_size 
   + self.fixed_frame_storage_size;
        Some(-((base + stack_index * 4) as i32))
    }
    
    /// Get stack offset for return value (index >= 8)
    /// 
    /// Similar to incoming args, but caller allocates space
    pub fn return_value_offset(&self, ret_index: usize) -> Option<i32> {
        if ret_index < 8 {
            return None; // In register
        }
        let stack_index = ret_index - 8;
        Some((stack_index * 4) as i32)
    }
}
```

**Tests**:

- `test_incoming_arg_offset`
- `test_outgoing_arg_offset`
- `test_return_value_offset`

### Phase 4: Load Incoming Stack Arguments in Prologue

**File**: `crates/r5-target-riscv32/src/lower.rs`

**Changes**:

Update `gen_prologue` to load incoming stack arguments:

1. **Before** adjusting SP, incoming stack args are at positive offsets from SP
2. Load stack args into their allocated registers or spill slots
3. Then adjust SP and save callee-saved registers

**Implementation approach**:

- For stack args that are in registers (not spilled): Load directly into allocated register
- For stack args that are spilled: Load into temp register, then store to spill slot after SP adjustment
- Coordinate with register allocation to determine final location

**Key logic**:

```rust
// Step 1: Load incoming stack arguments (before SP adjustment)
for (idx, param) in entry_block.params.iter().enumerate() {
    if let Some(stack_offset) = abi_info.param_stack_offsets.get(&idx) {
        if let Some(allocated_reg) = allocation.value_to_reg.get(param) {
            // Load directly into allocated register
            code.emit(RiscvInst::Lw {
                rd: *allocated_reg,
                rs1: Gpr::SP,
                imm: *stack_offset, // Positive offset
            });
        } else {
            // Will be spilled - load into temp, store after SP adjustment
            // Store temp_reg and stack_offset for later
        }
    }
}

// Step 2: Adjust SP for entire frame
// ... existing code ...

// Step 3: Store spilled stack args to their spill slots
// ... handle temp registers stored above ...
```

**Tests**:

- `test_prologue_loads_stack_args`
- `test_prologue_handles_mixed_reg_and_stack_args`
- `test_prologue_stack_args_with_spills`

### Phase 5: Store Outgoing Stack Arguments at Call Sites

**File**: `crates/r5-target-riscv32/src/lower.rs`

**Changes**:

Update `lower_call` to store stack arguments:

1. Move register arguments (a0-a7) - existing code
2. **New**: Store stack arguments (index >= 8) to outgoing args area
3. Make the call - existing code
4. Load return values from registers - existing code
5. **New**: Load stack return values (if any)

**Implementation**:

```rust
// Step 2: Store stack arguments (index >= 8)
for (idx, arg) in args.iter().enumerate() {
    if idx >= 8 {
        // Compute stack offset for this argument
        let offset = frame_layout.outgoing_arg_offset(idx).unwrap();
        
        // Load argument value into temporary register
        let temp_reg = Gpr::T0;
        self.load_value_into_reg(code, *arg, temp_reg, allocation, frame_layout);
        
        // Store to outgoing args area
        code.emit(RiscvInst::Sw {
            rs1: Gpr::SP,
            rs2: temp_reg,
            imm: offset, // Negative offset
        });
    }
}
```

**Note**: Need to pass callee signature to `lower_call` to know how many args the callee expects.

**Tests**:

- `test_call_with_stack_args`
- `test_call_with_mixed_reg_and_stack_args`
- `test_call_with_many_args`

### Phase 6: Handle Stack Return Values

**File**: `crates/r5-target-riscv32/src/lower.rs`

**Changes**:

Update `lower_return` and `lower_call` to handle stack returns:

1. **In `lower_return`**: Store return values >= 8 to stack

    - Need to know where caller allocated return space
    - For now, assume return values are stored at positive offsets from SP (before epilogue)

2. **In `lower_call`**: Load return values >= 8 from stack

    - After call, load from stack at appropriate offsets
    - Store to result values (registers or spill slots)

**Implementation**:

```rust
// In lower_return:
for (idx, value) in values.iter().enumerate() {
    if idx >= 8 {
        if let Some(stack_offset) = abi_info.return_stack_offsets.get(&idx) {
            // Load value into temp register
            let temp_reg = Gpr::T0;
            self.load_value_into_reg(code, *value, temp_reg, allocation, frame_layout);
            
            // Store to stack (offset relative to SP before epilogue)
            code.emit(RiscvInst::Sw {
                rs1: Gpr::SP,
                rs2: temp_reg,
                imm: *stack_offset, // Positive offset
            });
        }
    }
}
```

**Tests**:

- `test_return_with_stack_returns`
- `test_call_receives_stack_returns`
- `test_mixed_reg_and_stack_returns`

### Phase 7: Update Function Signature Analysis

**File**: `crates/r5-target-riscv32/src/lib.rs`

**Changes**:

When computing frame layout, use actual function signature instead of just parameter count:

1. Pass function signature to `FrameLayout::compute`
2. Compute `incoming_args_size` based on actual signature params
3. Compute `outgoing_args_size` based on analyzed call sites (from Phase 2)

**Tests**:

- `test_frame_layout_with_stack_args`
- `test_frame_layout_with_stack_returns`

## Implementation Order

1. **Phase 1**: Enhance AbiInfo (foundation)
2. **Phase 2**: Compute outgoing args (needed for frame layout)
3. **Phase 3**: Frame layout helpers (needed for prologue/call)
4. **Phase 4**: Prologue loading (incoming args)
5. **Phase 5**: Call site storage (outgoing args)
6. **Phase 6**: Stack returns (complete the picture)
7. **Phase 7**: Integration and cleanup

## Key Design Decisions

1. **Stack argument offsets**: Use positive offsets for incoming args (relative to SP before prologue), negative offsets for outgoing args (relative to SP after prologue), matching Cranelift's approach.

2. **Loading strategy**: Load stack args directly into allocated registers when possible, otherwise use temp registers and store to spill slots after SP adjustment.

3. **Return values**: For simplicity, use similar approach to arguments. More complex ABI features (like sret) can be added later.

4. **Coordination with register allocator**: Stack args that are spilled need special handling - load from stack, then store to spill slot after frame setup.

## References

- Cranelift RISC-V 64 ABI: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/abi.rs`
- Cranelift addressing modes: `/Users/yona/dev/photomancer/wasmtime/cranelift/codege