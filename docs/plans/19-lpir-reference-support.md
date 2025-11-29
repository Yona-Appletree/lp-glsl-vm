# LPIR Reference Support: Stack Allocation for Out/Inout Parameters

## Overview

This plan outlines the implementation of proper reference support in LPIR to enable correct handling of `out` and `inout` parameters in GLSL functions. Currently, the GLSL compiler uses placeholder I32 constants for addresses, which don't represent actual stack-allocated memory. This plan adds a `StackAlloc` instruction to LPIR that explicitly allocates stack space and returns an address, enabling proper reference parameter passing.

## Current State

### LPIR Limitations

1. **No stack allocation instruction**: LPIR has `Load` and `Store` instructions but no way to allocate stack space
2. **Addresses are opaque I32 values**: There's no way to distinguish addresses from regular integers
3. **No SP/FP access**: LPIR doesn't provide access to stack pointer or frame pointer

### GLSL Compiler Workaround

The GLSL compiler currently uses a workaround that:

- Creates placeholder I32 constants (e.g., `iconst 0`, `iconst 1`) as addresses
- Passes these "addresses" to functions expecting `out`/`inout` parameters
- Uses these values with `Load`/`Store` instructions

**Problems with this approach:**

- Backend doesn't know these are addresses (treated as regular integers)
- Load/Store instructions hardcode offset to 0 (can't use constant as offset)
- No actual stack allocation occurs (addresses don't point to valid memory)
- Backend TODO comments indicate this needs proper address materialization

### Backend State

The RISC-V backend (`crates/lpc-codegen/src/isa/riscv32/backend3/lower.rs`):

- Has Load/Store lowering that hardcodes offset to 0
- Has TODO comments for detecting `iadd(base, const)` patterns to extract offsets
- Handles stack frames at machine code level (prologue/epilogue)
- Allocates stack space during register spilling, but not for explicit allocations

## Goals

1. **Add `StackAlloc` instruction to LPIR**: Explicitly allocate stack space and return an address
2. **Update parser**: Parse `stackalloc` syntax in LPIR text format
3. **Update backend**: Lower `StackAlloc` to SP adjustment + address computation
4. **Update GLSL codegen**: Replace placeholder constants with `StackAlloc` calls
5. **Maintain compatibility**: Ensure existing code continues to work

## Architecture

### StackAlloc Instruction

**Opcode:**

```rust
Opcode::StackAlloc {
    size: u32,  // Size in bytes to allocate
}
```

**Semantics:**

- Allocates `size` bytes on the stack
- Returns an I32 value representing the address of the allocated space
- Stack grows downward (address is SP after allocation)
- Alignment: Allocated space should be aligned according to target ABI (typically 4-byte aligned for RISC-V 32-bit)

**LPIR Text Format:**

```
v0 = stackalloc 4    ; Allocate 4 bytes, address in v0
v1 = stackalloc 8    ; Allocate 8 bytes, address in v1
```

### Lowering Strategy

**RISC-V 32-bit Lowering:**

1. **Compute allocation size**: Round up to alignment boundary (4 bytes for RISC-V 32-bit)
2. **Adjust stack pointer**: `addi sp, sp, -size` (stack grows downward)
3. **Materialize address**: The address is the new SP value
4. **Track allocation**: Backend needs to track total stack allocation for epilogue

**Example lowering:**

```rust
// LPIR: v0 = stackalloc 4
// Lowered to:
//   addi sp, sp, -4    ; Allocate 4 bytes
//   mv v0, sp          ; Address is current SP
```

**Frame Layout Integration:**

The backend's frame layout computation should account for `StackAlloc` instructions:

- Total stack allocation = spill slots + callee-saved regs + stack allocations
- Prologue adjusts SP by total frame size
- Epilogue restores SP (deallocates entire frame)

### Address Materialization

After `StackAlloc`, the address value can be:

- Used directly with `Load`/`Store` (if offset is 0)
- Used with `iadd` to compute offsets: `iadd(stackalloc_addr, offset)`

The backend's Load/Store lowering should detect:

- Direct `StackAlloc` result → use as base register with offset 0
- `iadd(stackalloc_result, const)` → extract base and offset

This addresses the TODO comments in the backend about detecting `iadd(base, const)` patterns.

## Implementation Phases

### Phase 1: Add StackAlloc to LPIR Core

**Goal:** Add the instruction to LPIR's core data structures.

**Files:**

- `crates/lpc-lpir/src/dfg/opcode.rs` - Add `StackAlloc` variant
- `crates/lpc-lpir/src/dfg/mod.rs` - Update type inference (returns I32)

**Tasks:**

1. Add `StackAlloc { size: u32 }` variant to `Opcode` enum
2. Update `Opcode::has_side_effects()` to return `true` (modifies stack)
3. Update type inference to return `Type::I32` for `StackAlloc`
4. Add tests for opcode properties

**Validation:**

- Opcode enum compiles
- Type inference works correctly
- Side effects detection works

### Phase 2: Add Parser Support

**Goal:** Parse `stackalloc` syntax in LPIR text format.

**Files:**

- `crates/lpc-lpir/src/parser/instructions.rs` - Add parser for `stackalloc`

**Tasks:**

1. Add `parse_stackalloc` function:
   ```rust
   fn parse_stackalloc(input: &str) -> IResult<&str, Opcode> {
       // Parse: "stackalloc" <size>
       // Returns: Opcode::StackAlloc { size }
   }
   ```
2. Add to instruction parser list (after constants, before memory ops)
3. Add parser tests

**Example syntax:**

```
v0 = stackalloc 4
v1 = stackalloc 8
```

**Validation:**

- Parser accepts valid syntax
- Parser rejects invalid syntax
- Roundtrip tests (parse → format → parse)

### Phase 3: Add Formatter Support

**Goal:** Format `StackAlloc` instructions in LPIR text output.

**Files:**

- `crates/lpc-lpir/src/function.rs` - Update `fmt::Display` for instructions

**Tasks:**

1. Add formatting for `Opcode::StackAlloc`:
   ```rust
   Opcode::StackAlloc { size } => write!(f, "stackalloc {}", size)
   ```
2. Add formatter tests

**Validation:**

- Instructions format correctly
- Roundtrip tests pass

### Phase 4: Backend Lowering

**Goal:** Lower `StackAlloc` to RISC-V instructions.

**Files:**

- `crates/lpc-codegen/src/isa/riscv32/backend3/lower.rs` - Add lowering logic
- `crates/lpc-codegen/src/backend3/lower.rs` - Track stack allocations

**Tasks:**

1. Add lowering case for `Opcode::StackAlloc`:
   ```rust
   Opcode::StackAlloc { size } => {
       // Round up to 4-byte alignment
       let aligned_size = (size + 3) & !3;

       // Adjust SP: addi sp, sp, -aligned_size
       // Materialize address: mv result, sp
   }
   ```
2. Track total stack allocation for frame layout
3. Update frame layout computation to include stack allocations
4. Add lowering tests

**Considerations:**

- Alignment: RISC-V 32-bit requires 4-byte alignment
- Frame tracking: Need to track allocations for epilogue
- Multiple allocations: Each allocates independently

**Validation:**

- Stack allocation lowers correctly
- SP adjustment is correct
- Address materialization works
- Frame layout includes allocations

### Phase 5: Enhance Load/Store Lowering

**Goal:** Improve address materialization for Load/Store instructions.

**Files:**

- `crates/lpc-codegen/src/isa/riscv32/backend3/lower.rs` - Enhance Load/Store lowering

**Tasks:**

1. Detect `iadd(base, const)` patterns where base is a `StackAlloc` result
2. Extract base register and offset constant
3. Use RISC-V offset addressing: `lw rd, offset(rs1)` / `sw rs2, offset(rs1)`
4. Handle both direct `StackAlloc` results and `iadd` patterns

**Example:**

```rust
// LPIR:
v0 = stackalloc 4
v1 = iadd v0, 8
v2 = load.i32 v1

// Lowered to:
//   addi sp, sp, -4    ; stackalloc
//   mv v0, sp
//   lw v2, 8(v0)      ; load with offset
```

**Validation:**

- Direct StackAlloc addresses work
- iadd patterns are detected
- Offsets are extracted correctly
- RISC-V instructions use proper addressing modes

### Phase 6: Update GLSL Codegen

**Goal:** Replace placeholder constants with `StackAlloc` calls.

**Files:**

- `crates/lpc-glsl/src/codegen.rs` - Update function call generation

**Tasks:**

1. Replace `iconst` address placeholders with `StackAlloc`:

   ```rust
   // Old:
   let address_value = self.builder.new_value();
   block_builder.iconst(address_value, address_counter);

   // New:
   let address_value = self.builder.new_value();
   let size = param_type.size_in_bytes(); // e.g., 4 for i32
   block_builder.stackalloc(address_value, size);
   ```

2. Remove `address_counter` tracking (no longer needed)
3. Update tests to expect `stackalloc` instead of `iconst`

**Files to update:**

- `generate_expr` for `Expr::FunCall` (caller-side)
- Tests in `crates/lpc-glsl/tests/function_tests.rs`

**Validation:**

- GLSL tests pass with new implementation
- Generated LPIR uses `stackalloc`
- Addresses are valid stack locations

### Phase 7: Testing and Validation

**Goal:** Comprehensive testing of stack allocation and reference parameters.

**Test Files:**

- `crates/lpc-lpir/tests/integration.rs` - LPIR parser/formatter tests
- `crates/lpc-codegen/tests/stack_tests.rs` - Backend lowering tests
- `crates/lpc-glsl/tests/function_tests.rs` - GLSL reference parameter tests

**Test Cases:**

1. **LPIR Parser Tests:**

   - Parse `stackalloc` with various sizes
   - Parse invalid syntax (missing size, wrong type)
   - Roundtrip tests

2. **Backend Lowering Tests:**

   - Single `stackalloc` lowers correctly
   - Multiple `stackalloc`s allocate independently
   - Load/Store with `stackalloc` addresses
   - Load/Store with `iadd(stackalloc, offset)`
   - Frame layout includes allocations

3. **GLSL Integration Tests:**
   - `out` parameter: caller allocates, callee writes
   - `inout` parameter: caller allocates and initializes, callee reads and writes
   - Multiple `out`/`inout` parameters
   - Nested function calls with references
   - Verify values are correctly passed back

**Example Test:**

```rust
#[test]
fn test_out_parameter_with_stackalloc() {
    let glsl = r#"
        void set_value(out int x, int val) {
            x = val;
        }
        int main() {
            int a = 0;
            set_value(a, 10);
            return a;
        }
    "#;

    let test = GlslTest::new(glsl).unwrap();
    test.assert_lpir(
        "main",
        r#"
        function %main() -> i32 {
        block0:
            v0 = iconst 0
            v1 = stackalloc 4      ; Allocate space for 'a'
            v2 = iconst 10
            store.i32 v1, v0        ; Initialize 'a' to 0
            call %set_value(v1, v2)
            v3 = load.i32 v1        ; Load updated value
            return v3
        }
        "#,
    );
}
```

## Design Decisions

### Why StackAlloc Instead of SP/FP Access?

**StackAlloc advantages:**

- Explicit allocation semantics (clear when/where allocation happens)
- Backend can track allocations for frame layout
- Works with existing Load/Store instructions
- Aligns with LLVM/Cranelift approach

**SP/FP access disadvantages:**

- Requires tracking which values represent SP/FP
- More complex pattern matching in backend
- Less explicit about allocation intent

### Alignment Handling

**Decision:** Round up to 4-byte alignment (RISC-V 32-bit requirement)

**Rationale:**

- RISC-V 32-bit requires 4-byte alignment for word accesses
- Simpler to always align (no need to track per-allocation alignment)
- Minimal overhead (at most 3 bytes wasted per allocation)

### Frame Layout Integration

**Decision:** Include `StackAlloc` allocations in frame layout computation

**Rationale:**

- Prologue adjusts SP by total frame size (including allocations)
- Epilogue restores SP (deallocates entire frame)
- Consistent with how spill slots are handled

## Migration Strategy

### Backward Compatibility

- Existing LPIR code without `StackAlloc` continues to work
- Parser gracefully handles missing `StackAlloc` support (if needed)
- Backend lowering is additive (doesn't break existing code)

### Rollout Plan

1. **Phase 1-3**: Core LPIR changes (no breaking changes)
2. **Phase 4-5**: Backend changes (additive, doesn't affect existing code)
3. **Phase 6**: GLSL codegen update (replaces workaround)
4. **Phase 7**: Testing and validation

## Success Criteria

1. ✅ `StackAlloc` instruction exists in LPIR
2. ✅ Parser accepts `stackalloc` syntax
3. ✅ Formatter outputs `stackalloc` correctly
4. ✅ Backend lowers `StackAlloc` to correct RISC-V instructions
5. ✅ Load/Store work with `StackAlloc` addresses
6. ✅ GLSL `out`/`inout` parameters work correctly
7. ✅ All tests pass
8. ✅ Frame layout correctly accounts for allocations

## Future Enhancements

### Variable-Sized Allocations

Currently `StackAlloc` takes a constant size. Future enhancement:

- Variable-size allocations (requires runtime SP adjustment)
- Alignment specification (e.g., `stackalloc 8, align=16`)

### Address Arithmetic

Enhance backend to better handle:

- Complex address expressions (`iadd(iadd(base, off1), off2)`)
- Address comparisons and arithmetic
- Pointer types in LPIR (if needed)

### Stack Frame Optimization

- Combine multiple small allocations
- Reuse stack slots when possible
- Optimize frame layout for cache locality

## References

- **LLVM Alloca**: https://llvm.org/docs/LangRef.html#alloca-instruction
- **Cranelift Stack**: Cranelift uses similar approach for stack-allocated values
- **RISC-V ABI**: Stack alignment requirements (4-byte for RV32)
- **GLSL Reference Implementation**: `docs/glsl/01-initial-work.md` (Phase 9)

## Notes

- This plan addresses the immediate need for `out`/`inout` parameters
- Future plans may add more sophisticated memory management
- Stack allocation is sufficient for GLSL's reference parameters (no dynamic allocation needed)
