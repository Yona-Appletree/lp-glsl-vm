# Backend3 Phase 5: Advanced Features

**Goal**: Complete feature set

**Timeline**: Week 5+

**Deliverable**: Complete backend matching current backend features

## Cranelift References

**Primary Reference**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/machinst/`

- **Memory Operations**: `lower.rs` - Load/store instruction lowering
  - Stack frame access (SP-relative addressing)
  - Frame pointer usage
- **Block Layout Optimization**: `buffer.rs` - Block reordering and layout optimization
  - Cold block sinking
  - Fallthrough optimization
  - Block reordering based on branch probabilities
- **Branch Optimization**: `buffer.rs` - Advanced branch optimizations
  - Branch simplification
  - Unconditional branch elimination
  - Branch target optimization
- **Constant Pool**: `vcode.rs` - Constant handling and pooling
  - Large constant storage
  - PC-relative constant loading

**RISC-V Specific References**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/`

- **Memory Operations**: `inst/emit.rs` - Load/store emission
  - SP-relative addressing
  - Frame pointer usage
- **Constant Materialization**: `lower.rs` - Constant handling
  - LUI + ADDI sequences
  - Constant pool references

## Tasks

### 1. Memory operations

**Components**:

- Load/store lowering
- Stack frame access (SP-relative addressing)
- Frame pointer usage (if needed)

### 2. Block layout optimization

**Components**:

- Cold block sinking
- Fallthrough optimization
- Block reordering based on branch probabilities

**See**: Deferred features document (`17-backend3-deferred.md`)

### 3. Branch optimization

**Components**:

- Advanced branch simplification
- Unconditional branch elimination
- Branch target optimization

**See**: Deferred features document (`17-backend3-deferred.md`)

### 4. Constant pool (optional)

**Components**:

- Large constant storage in data section
- PC-relative constant loading

**See**: Deferred features document (`17-backend3-deferred.md`)

### 5. Module compilation

**Components**:

- Multi-function compilation
- Function address resolution
- Cross-function relocation fixup

## Testing

**Test Format Guidelines**:

- **Input**: Use textual LPIR format for clarity. Tests should define functions using the textual LPIR syntax to make the input code clear and readable.
- **Expected Output**: Use assembler format to clearly show the expected RISC-V 32 machine code. This is especially important for memory operations, block layout optimizations, and complex multi-function scenarios.

**Test Examples**:

```rust
#[test]
fn test_memory_operations() {
    // Input: textual LPIR format for clarity
    let lpir_text = r#"
        function @test(i32* %ptr, i32 %val) -> i32 {
        entry:
            store %val, %ptr
            %0 = load %ptr
            ret %0
        }
    "#;

    let func = parse_lpir_function(lpir_text);
    let vcode = Lower::new(func).lower(&block_order);
    let regalloc = vcode.run_regalloc();
    let buffer = vcode.emit(&regalloc);

    // Expected: assembler format showing load/store instructions
    let expected_asm = r#"
        # Prologue...

        # Store
        sw   a1, 0(a0)

        # Load
        lw   a0, 0(a0)

        # Epilogue...
    "#;
}

#[test]
fn test_block_layout_optimization() {
    // Input: textual LPIR format
    let lpir_text = r#"
        function @test(i32 %a) -> i32 {
        entry:
            %cond = icmp eq %a, 0
            br %cond, hot, cold
        hot:
            %0 = iadd %a, 1
            ret %0
        cold:
            %1 = imul %a, 100
            ret %1
        }
    "#;

    let func = parse_lpir_function(lpir_text);
    let vcode = Lower::new(func).lower(&block_order);
    let regalloc = vcode.run_regalloc();
    let buffer = vcode.emit(&regalloc);

    // Expected: assembler format showing optimized block layout
    // (hot path first, cold path at end)
    let expected_asm = r#"
        # Prologue...

        # Hot path (fallthrough)
        beq  a0, zero, .Lcold
        addi a0, a0, 1
        # Epilogue...

        # Cold path (at end)
    .Lcold:
        # ... cold path code ...
    "#;
}
```

**Test Categories**:

- Unit tests for memory operations
- Unit tests for block layout optimization
- Integration test: Compile complex functions
- Integration test: Compile multiple functions

## Success Criteria

- ✅ Can lower all memory operations
- ✅ Block layout optimization works
- ✅ Can compile complex functions
- ✅ Matches all current backend features
- ✅ Passes all existing tests
