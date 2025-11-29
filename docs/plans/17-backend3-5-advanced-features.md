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

**Test Infrastructure**: Tests use the filetest infrastructure in `crates/lpc-filetests/`. Tests are written as `.lpir` files that contain:

- Test command header (e.g., `test compile` or `test vcode`)
- Function definitions in textual LPIR format
- Expected output in comments (starting with `;`)
- Filecheck directives for flexible pattern matching (`check:`, `nextln:`, `sameln:`)

**Test Format Guidelines**:

- **Input**: Use textual LPIR format in `.lpir` test files. Functions are defined using the textual LPIR syntax, making the input code clear and readable.
- **Expected Output**: Use assembler format in comments to clearly show the expected RISC-V 32 machine code. Use filecheck directives for flexible matching when exact formatting may vary, especially for memory operations, block layout optimizations, and complex multi-function scenarios.
- **Filecheck Directives**:
  - `check: <pattern>` - Starts a check block, matches pattern
  - `nextln: <content>` - Expects content on next line (strict)
  - `sameln: <content>` - Expects content on same or next line (flexible, searches up to 3 lines ahead)
  - `}` - Ends a check block

**Test Examples**:

**Memory Operations Test** (`filetests/backend3/memory-operations.lpir`):

```lpir
test compile

function %test(i32* %ptr, i32 %val) -> i32 {
block0(v0: i32*, v1: i32):
    store v1, v0
    %0 = load v0
    return %0
}

; check: # Prologue
; sameln: addi sp, sp, -8
; check: # Store
; sameln: sw   a1, 0(a0)
; check: # Load
; sameln: lw   a0, 0(a0)
; check: # Epilogue
; sameln: lw   fp, 0(sp)
; sameln: lw   ra, 4(sp)
; sameln: addi sp, sp, 8
; sameln: jalr zero, ra, 0
```

**Block Layout Optimization Test** (`filetests/backend3/block-layout.lpir`):

```lpir
test compile

function %test(i32 %a) -> i32 {
block0(v0: i32):
    %cond = icmp eq v0, 0
    brif %cond, block1, block2
block1:
    %0 = iadd v0, 1
    return %0
block2:
    %1 = imul v0, 100
    return %1
}

; check: # Prologue
; sameln: addi sp, sp, -8
; check: # Hot path (fallthrough)
; sameln: beq  a0, zero, .Lblock2
; sameln: addi a0, a0, 1
; check: # Epilogue (hot path)
; sameln: lw   fp, 0(sp)
; sameln: lw   ra, 4(sp)
; sameln: addi sp, sp, 8
; sameln: jalr zero, ra, 0
; check: # Cold path (at end)
; sameln: .Lblock2:
; sameln: mul  a0, a0, 100
; check: # Epilogue (cold path)
; sameln: lw   fp, 0(sp)
; sameln: lw   ra, 4(sp)
; sameln: addi sp, sp, 8
; sameln: jalr zero, ra, 0
```

**Multi-Function Compilation Test** (`filetests/backend3/multi-function.lpir`):

```lpir
test compile

function %helper(i32 %x) -> i32 {
block0(v0: i32):
    %0 = iadd v0, 1
    return %0
}

function %main(i32 %a) -> i32 {
block0(v0: i32):
    %0 = call @helper(v0)
    %1 = call @helper(%0)
    return %1
}

; check: function %helper
; sameln: # Prologue
; check: function %main
; sameln: # Prologue
; check: # Call @helper
; sameln: mv   a0, a0
; sameln: jal  ra, helper
; check: # Call @helper again
; sameln: mv   a0, a0
; sameln: jal  ra, helper
; check: # Epilogue
```

**Test Categories**:

- Filetests for memory operations (`filetests/backend3/memory-operations.lpir`)
- Filetests for block layout optimization (`filetests/backend3/block-layout.lpir`)
- Filetests for constant pool handling (`filetests/backend3/constant-pool.lpir`)
- Filetests for multi-function compilation (`filetests/backend3/multi-function.lpir`)
- Integration filetests: Complex functions (`filetests/backend3/complex.lpir`)

## Success Criteria

- ✅ Can lower all memory operations
- ✅ Block layout optimization works
- ✅ Can compile complex functions
- ✅ Matches all current backend features
- ✅ Passes all existing tests
