# Adopt Condition Codes and Traps

## Overview

This plan migrates LPIR from separate comparison opcodes (`IcmpEq`, `IcmpLt`, etc.) to Cranelift-style condition codes (`Icmp` + `IntCC`, `Fcmp` + `FloatCC`), and adds trap support for explicit error handling. This improves optimization opportunities and enables better error diagnostics.

**Important:** f32 types and `Fcmp` opcodes will be added to the IR, but the backend will remain i32/u32 only. Software floating point translation will be implemented in a future plan.

## Goals

1. **Condition Codes**: Replace 6 integer comparison opcodes with single `Icmp` opcode + condition code operand
2. **Floating Point Comparisons**: Add `Fcmp` opcode with `FloatCC` for IR support (backend lowering deferred)
3. **Trap Support**: Add trap codes and trap instructions (`trap`, `trapz`, `trapnz`)
4. **Backward Compatibility**: Maintain compatibility during migration (support both old and new formats)

## Phase 1: Add Condition Code Types

### 1.1 Create Condition Code Enums

**Files to create:**

- `crates/lpc-lpir/src/condcodes.rs`

**Implementation:**

- Add `IntCC` enum with variants: `Equal`, `NotEqual`, `SignedLessThan`, `SignedGreaterThanOrEqual`, `SignedGreaterThan`, `SignedLessThanOrEqual`, `UnsignedLessThan`, `UnsignedGreaterThanOrEqual`, `UnsignedGreaterThan`, `UnsignedLessThanOrEqual`
- Add `FloatCC` enum with variants: `Equal`, `NotEqual`, `LessThan`, `LessThanOrEqual`, `GreaterThan`, `GreaterThanOrEqual`, `Ordered`, `Unordered`, `OrderedNotEqual`, `UnorderedOrEqual`, `UnorderedOrLessThan`, `UnorderedOrLessThanOrEqual`, `UnorderedOrGreaterThan`, `UnorderedOrGreaterThanOrEqual`
- Implement `CondCode` trait with `complement()` and `swap_args()` methods
- Implement `Display` and `FromStr` for parsing/printing

**Reference:** `cranelift/codegen/src/ir/condcodes.rs`

## Phase 2: Add Trap Code Types

### 2.1 Create Trap Code Type

**Files to create:**

- `crates/lpc-lpir/src/trapcode.rs`

**Implementation:**

- Add `TrapCode` struct wrapping `NonZeroU8`
- Define standard trap codes:
    - `STACK_OVERFLOW`
    - `INTEGER_OVERFLOW`
    - `HEAP_OUT_OF_BOUNDS`
    - `INTEGER_DIVISION_BY_ZERO`
    - `BAD_CONVERSION_TO_INTEGER`
- Support user-defined trap codes (1-250)
- Implement `Display` and `FromStr` for parsing/printing

**Reference:** `cranelift/codegen/src/ir/trapcode.rs`

## Phase 3: Update Opcode Enum

### 3.1 Replace Comparison Opcodes

**Files to modify:**

- `crates/lpc-lpir/src/dfg/opcode.rs`

**Changes:**

- Remove: `IcmpEq`, `IcmpNe`, `IcmpLt`, `IcmpLe`, `IcmpGt`, `IcmpGe`
- Add: `Icmp { cond: IntCC }` - Integer comparison with condition code
- Add: `Fcmp { cond: FloatCC }` - Floating point comparison with condition code (IR-only, backend not supported yet)

**Migration strategy:**

- Keep old opcodes temporarily with `#[deprecated]` attribute
- Add helper methods to convert old opcodes to new format

### 3.2 Add Trap Opcodes

**Files to modify:**

- `crates/lpc-lpir/src/dfg/opcode.rs`

**Changes:**

- Add: `Trap { code: TrapCode }` - Unconditional trap
- Add: `Trapz { code: TrapCode }` - Trap if condition is zero
- Add: `Trapnz { code: TrapCode }` - Trap if condition is non-zero

## Phase 4: Update InstData Structure

### 4.1 Add Condition Code Storage

**Files to modify:**

- `crates/lpc-lpir/src/dfg/inst_data.rs`

**Changes:**

- Update `comparison()` method to take condition code parameter
- Add `trap()`, `trapz()`, `trapnz()` constructor methods

**Design decision:**

- Store condition codes in `Immediate` enum variant `CondCode(IntCC)` or `FloatCondCode(FloatCC)`
- This avoids adding fields to all instructions

### 4.2 Update Immediate Enum

**Files to modify:**

- `crates/lpc-lpir/src/dfg/inst_data.rs`

**Changes:**

- Add `IntCondCode(IntCC)` variant to `Immediate`
- Add `FloatCondCode(FloatCC)` variant to `Immediate`
- Add `TrapCode(TrapCode)` variant to `Immediate`

## Phase 5: Update Builder API

### 5.1 Update Comparison Builder Methods

**Files to modify:**

- `crates/lpc-lpir/src/builder/traits.rs`
- `crates/lpc-lpir/src/builder/block_builder.rs`

**Changes:**

- Replace `icmp_eq()`, `icmp_lt()`, etc. with single `icmp(cond: IntCC, arg1: Value, arg2: Value) -> Value`
- Add convenience methods: `icmp_eq()`, `icmp_lt()`, etc. that call `icmp()` with appropriate condition code
- Add `fcmp(cond: FloatCC, arg1: Value, arg2: Value) -> Value` for IR support (backend will error if lowered)

**Example:**

```rust
// New API
fn icmp(self, cond: IntCC, arg1: Value, arg2: Value) -> Value

// Convenience methods (backward compatible)
fn icmp_eq(self, arg1: Value, arg2: Value) -> Value {
    self.icmp(IntCC::Equal, arg1, arg2)
}
```

### 5.2 Add Trap Builder Methods

**Files to modify:**

- `crates/lpc-lpir/src/builder/traits.rs`
- `crates/lpc-lpir/src/builder/block_builder.rs`

**Changes:**

- Add `trap(code: TrapCode)` - Unconditional trap
- Add `trapz(condition: Value, code: TrapCode)` - Trap if zero
- Add `trapnz(condition: Value, code: TrapCode)` - Trap if non-zero

## Phase 6: Update Parser

### 6.1 Parse Condition Codes

**Files to modify:**

- `crates/lpc-lpir/src/parser/instructions.rs`

**Changes:**

- Update `parse_comparison()` to parse `icmp` with condition code: `v0 = icmp eq v1, v2`
- Parse integer condition codes: `eq`, `ne`, `slt`, `sge`, `sgt`, `sle`, `ult`, `uge`, `ugt`, `ule`
- Add `parse_fcmp()` for floating point comparisons: `v0 = fcmp eq v1, v2`
- Parse float condition codes: `eq`, `ne`, `lt`, `le`, `gt`, `ge`, `ord`, `uno`, `one`, `ueq`, `ult`, `ule`, `ugt`, `uge`
- Support both old format (`icmp_eq`) and new format (`icmp eq`) during migration

### 6.2 Parse Trap Instructions

**Files to modify:**

- `crates/lpc-lpir/src/parser/instructions.rs`

**Changes:**

- Add `parse_trap()` - `trap int_divz`
- Add `parse_trapz()` - `trapz v0, int_divz`
- Add `parse_trapnz()` - `trapnz v0, int_ovf`
- Parse trap code names: `int_divz`, `int_ovf`, `heap_oob`, `stk_ovf`, `bad_toint`, `user42`

## Phase 7: Lowerer
Do not update the lowerer yet.
## Phase 8: Update Verifier

### 8.1 Verify Condition Codes

**Files to modify:**

- `crates/lpc-lpir/src/verifier/format.rs`

**Changes:**

- Verify `Icmp` opcode has `IntCondCode` immediate
- Verify `Fcmp` opcode has `FloatCondCode` immediate (IR validation only - backend doesn't support yet)
- Verify `Fcmp` operands are f32 type (IR validation)
- Verify trap instructions have `TrapCode` immediate

## Phase 9: Migration and Testing

### 9.1 Update Tests

**Files to modify:**

- All test files using comparisons

**Changes:**

- Update tests to use new condition code API
- Add tests for condition code transformations (`complement()`, `swap_args()`)
- Add tests for trap instructions
- Add tests for `Fcmp` IR parsing and validation (but not lowering)
- Test that `Fcmp` lowering returns appropriate error
- Test backward compatibility (old opcodes still work)

### 9.2 Update Documentation

**Files to modify:**

- `crates/lpc-lpir/README.md`
- Any documentation referencing comparisons

**Changes:**

- Document new condition code API
- Document trap instructions
- Document that `Fcmp` is IR-only and cannot be lowered yet
- Provide migration guide

## Implementation Order

1. **Phase 1**: Add condition code types (foundation)
2. **Phase 2**: Add trap code types (foundation)
3. **Phase 3**: Update opcode enum (core change)
4. **Phase 4**: Update InstData (core change)
5. **Phase 5**: Update builder API (user-facing)
6. **Phase 6**: Update parser (user-facing)
7. **Phase 7**: Update lowerer (backend - with FP error handling)
8. **Phase 8**: Update verifier (safety)
9. **Phase 9**: Testing and migration

## Breaking Changes

- Old comparison opcodes (`IcmpEq`, etc.) will be deprecated
- Parser will support both old and new formats during transition
- Builder API will maintain convenience methods for backward compatibility

## Important Notes

### Floating Point Support Status

- **IR Support**: f32 types and `Fcmp` opcode will be added to the IR
- **Backend Support**: Backend remains i32/u32 only - no FP lowering yet
- **Future**: Software floating point translation will be implemented in a separate plan
- **Current Limitation**: `Fcmp` instructions will be validated in IR but will error if lowered to RISC-V

### Error Handling for Fcmp

When the lowerer encounters an `Fcmp` instruction, it should:

- Return a clear error message: "Floating point operations not supported in backend yet. Software FP translation coming in future plan."
- Or panic with descriptive message (depending on error handling strategy)

## Future Work (Not in This Plan)

- **Software floating point translation**: Implementation of f32 operations via software library calls
- **Fcmp backend lowering**: Will be part of software FP translation plan
- Optimizations using condition code transformations
- Runtime trap handling integration
- `br_table` support (explicitly excluded per requirements)