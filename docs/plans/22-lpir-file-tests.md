# LPIR File-Based Test System

## Overview

This plan extends the file-based test system (similar to Cranelift's `.clif` filetests) to cover additional test types beyond the initial `test transform fixed-point` implementation. These tests allow us to verify IR transformations, analysis passes, and backend lowering without requiring a full execution stack.

## Current Status

**Implemented:**
- `test transform fixed-point` - Tests float-to-fixed-point transformation

**Infrastructure:**
- Test file parser (`parse_test_file()`)
- Test runner framework (`run_transform_test()`, `normalize_ir()`)
- Test file format support (`.lpir` files with expected output in comments)

## Test Types to Implement

### Priority 1: Simple and High-Value

#### 1. `test cat` - Roundtrip Parser/Printer Tests

**Purpose:** Verify that parsing and printing IR produces consistent output.

**Use Cases:**
- Parser correctness
- Printer correctness
- Roundtrip stability
- Formatting consistency

**Example Test File:**
```lpir
test cat

function %simple_add(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}

; function %simple_add(i32, i32) -> i32 {
; block0(v0: i32, v1: i32):
;     v2 = iadd v0, v1
;     return v2
; }
```

**Implementation:**
- Parse function from test file
- Format function using `format!("{}", func)`
- Compare with expected output using filecheck
- No transformation needed - just parse and print

**Files:**
- `crates/lpc-lpir/tests/filetests/cat/basic.lpir` - Basic roundtrip tests
- `crates/lpc-lpir/tests/filetests/cat/complex.lpir` - Complex functions

**Estimated Effort:** 0.5 days

---

#### 2. `test verifier` - Verifier Error Detection Tests

**Purpose:** Verify that the IR verifier correctly catches invalid IR.

**Use Cases:**
- Dominance violations
- Type mismatches
- Invalid instruction formats
- SSA violations

**Example Test File:**
```lpir
test verifier

function %non_dominating(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v2, v0   ; error: uses value v2 from non-dominating
    v2 = iadd v1, v0
    return v2
}

function %type_mismatch(i32) -> i32 {
block0(v0: i32):
    v1 = iadd v0, v0
    v2 = fadd v1, v0   ; error: type mismatch: i32 vs f32
    return v2
}
```

**Implementation:**
- Parse function from test file
- Scan for `; error:` annotations on instructions
- Run verifier with `verify()`
- Check that verifier produces expected errors
- Match error messages against annotations

**Files:**
- `crates/lpc-lpir/tests/filetests/verifier/dominance.lpir` - Dominance violations
- `crates/lpc-lpir/tests/filetests/verifier/types.lpir` - Type errors
- `crates/lpc-lpir/tests/filetests/verifier/ssa.lpir` - SSA violations

**Estimated Effort:** 1 day

---

### Priority 2: Analysis Tests

#### 3. `test domtree` - Dominator Tree Tests

**Purpose:** Verify dominator tree computation correctness.

**Use Cases:**
- Dominator tree construction
- Immediate dominator queries
- Dominance relationship verification

**Example Test File:**
```lpir
test domtree

function %test(i32) {
block0(v0: i32):
    jump block1              ; dominates: block1
block1:
    brif v0, block2, block3  ; dominates: block2 block3
block2:
    jump block3
block3:
    return
}

; check: domtree_preorder {
; nextln: block0: block1
; nextln: block1: block2 block3
; nextln: block2:
; nextln: block3:
; nextln: }
```

**Implementation:**
- Parse function from test file
- Build `ControlFlowGraph` and `DominatorTree`
- Extract `; dominates:` annotations from instructions
- Verify immediate dominator relationships match annotations
- Optionally print domtree structure for filecheck verification

**Files:**
- `crates/lpc-lpir/tests/filetests/domtree/basic.lpir` - Simple dominance
- `crates/lpc-lpir/tests/filetests/domtree/loops.lpir` - Loop dominance
- `crates/lpc-lpir/tests/filetests/domtree/complex.lpir` - Complex CFG

**Estimated Effort:** 1 day

---

#### 4. `test print-cfg` - Control Flow Graph Tests

**Purpose:** Verify CFG construction and printing.

**Use Cases:**
- CFG construction correctness
- Predecessor/successor relationships
- Block ordering (post-order, reverse post-order)

**Example Test File:**
```lpir
test print-cfg

function %test(i32) {
block0(v0: i32):
    brif v0, block1, block2
block1:
    jump block2
block2:
    return
}

; check: cfg_postorder:
; sameln: block2
; sameln: block1
; sameln: block0
; check: predecessors:
; check: block0: []
; check: block1: [block0]
; check: block2: [block0, block1]
```

**Implementation:**
- Parse function from test file
- Build `ControlFlowGraph`
- Print CFG structure (post-order, predecessors, successors)
- Verify with filecheck directives

**Files:**
- `crates/lpc-lpir/tests/filetests/cfg/basic.lpir` - Simple CFG
- `crates/lpc-lpir/tests/filetests/cfg/loops.lpir` - Loops
- `crates/lpc-lpir/tests/filetests/cfg/complex.lpir` - Complex control flow

**Estimated Effort:** 1 day

---

### Priority 3: Backend Tests

#### 5. `test lower` - Backend Lowering Tests

**Purpose:** Verify LPIR → VCode lowering correctness.

**Use Cases:**
- Instruction lowering
- Register allocation (if we want to test that)
- VCode generation
- Machine instruction selection

**Example Test File:**
```lpir
test lower

function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}

; vcode {
;     entry: block0
;     
;     block0(v0, v1):
;         v2 = add v0, v1
;         return v2
; }
```

**Implementation:**
- Parse function from test file
- Lower function using backend3 (`lower_function()`)
- Format VCode using VCode formatter
- Compare with expected VCode output

**Note:** This requires VCode text format support (may need to add formatter).

**Files:**
- `crates/lpc-lpir/tests/filetests/lower/arithmetic.lpir` - Arithmetic operations
- `crates/lpc-lpir/tests/filetests/lower/control-flow.lpir` - Branches and jumps
- `crates/lpc-lpir/tests/filetests/lower/memory.lpir` - Load/store operations

**Estimated Effort:** 2-3 days (includes VCode formatter if needed)

---

## Implementation Plan

### Phase 1: Core Test Infrastructure (1 day)

**Goal:** Extend test runner to support multiple test types.

**Tasks:**
1. Refactor `parse_test_file()` to support different test commands
2. Create `TestCommand` enum or struct to represent test types
3. Add `run_filecheck()` helper for filecheck-style verification
4. Add support for `; check:` directives (like Cranelift's filecheck)

**Files:**
- `crates/lpc-lpir/tests/filetests.rs` - Extend with test type support

---

### Phase 2: Simple Tests (1.5 days)

**Goal:** Implement `test cat` and `test verifier`.

**Tasks:**
1. Implement `test cat` runner
2. Create test files for parser/printer roundtrip
3. Implement `test verifier` runner with error annotation parsing
4. Create test files for verifier error cases

**Files:**
- `crates/lpc-lpir/tests/filetests.rs` - Add `test_cat()` and `test_verifier()` functions
- `crates/lpc-lpir/tests/filetests/cat/` - Cat test files
- `crates/lpc-lpir/tests/filetests/verifier/` - Verifier test files

---

### Phase 3: Analysis Tests (2 days)

**Goal:** Implement `test domtree` and `test print-cfg`.

**Tasks:**
1. Implement `test domtree` runner with annotation parsing
2. Create test files for dominator tree verification
3. Implement `test print-cfg` runner with CFG printing
4. Create test files for CFG verification

**Files:**
- `crates/lpc-lpir/tests/filetests.rs` - Add `test_domtree()` and `test_print_cfg()` functions
- `crates/lpc-lpir/tests/filetests/domtree/` - Domtree test files
- `crates/lpc-lpir/tests/filetests/cfg/` - CFG test files

---

### Phase 4: Backend Tests (2-3 days)

**Goal:** Implement `test lower` for backend verification.

**Tasks:**
1. Add VCode text formatter (if not already present)
2. Implement `test lower` runner
3. Create test files for lowering verification
4. Test various instruction types and control flow patterns

**Files:**
- `crates/lpc-lpir/tests/filetests.rs` - Add `test_lower()` function
- `crates/lpc-codegen/src/isa/riscv32/backend3/vcode_format.rs` - May need formatter
- `crates/lpc-lpir/tests/filetests/lower/` - Lowering test files

---

## Test File Format

All test files follow this general format:

```
test <command> [options]

function %name(...) -> ... {
    ...
}

; Expected output or annotations
; ...
```

### Test Commands

- `test cat` - Roundtrip parse/print
- `test verifier` - Verifier error detection
- `test domtree` - Dominator tree verification
- `test print-cfg` - CFG construction verification
- `test lower` - Backend lowering verification
- `test transform <name>` - Transformation pass (already implemented)

### Annotations

- `; error: <message>` - Expected verifier error
- `; dominates: <block> ...` - Expected immediate dominator
- `; check: <pattern>` - Filecheck directive for output matching

---

## File Structure

```
crates/lpc-lpir/tests/
├── filetests.rs                    # Main test runner (extend)
└── filetests/
    ├── transform/
    │   └── fixed-point.lpir        # Already implemented
    ├── cat/
    │   ├── basic.lpir
    │   └── complex.lpir
    ├── verifier/
    │   ├── dominance.lpir
    │   ├── types.lpir
    │   └── ssa.lpir
    ├── domtree/
    │   ├── basic.lpir
    │   ├── loops.lpir
    │   └── complex.lpir
    ├── cfg/
    │   ├── basic.lpir
    │   ├── loops.lpir
    │   └── complex.lpir
    └── lower/
        ├── arithmetic.lpir
        ├── control-flow.lpir
        └── memory.lpir
```

---

## Success Criteria

### Phase 1 Complete When:
- Test runner supports multiple test types
- Filecheck-style verification works
- Test command parsing is robust

### Phase 2 Complete When:
- `test cat` passes all test files
- `test verifier` correctly detects expected errors
- Test files cover common parser/printer and verifier cases

### Phase 3 Complete When:
- `test domtree` correctly verifies dominator relationships
- `test print-cfg` correctly prints and verifies CFG structure
- Test files cover various CFG patterns

### Phase 4 Complete When:
- `test lower` correctly verifies backend lowering
- VCode output matches expected format
- Test files cover major instruction types and patterns

---

## Future Enhancements

### Additional Test Types (Future)

1. **`test optimize`** - Optimization pass tests (when optimizations are added)
2. **`test inline`** - Inlining tests (when inlining is implemented)
3. **`test alias-analysis`** - Alias analysis tests (when alias analysis is added)

### Test Infrastructure Improvements

1. **Auto-update support** - Like Cranelift's `TEST_BLESS` environment variable
2. **Parallel test execution** - Run multiple test files in parallel
3. **Test discovery** - Automatically find and run all `.lpir` files
4. **Better error messages** - Show diffs, context, and suggestions

---

## Dependencies

### Existing Infrastructure (Available)
- `parse_function()` - IR parser
- `format!("{}", func)` - IR printer
- `verify()` - IR verifier
- `ControlFlowGraph` - CFG analysis
- `DominatorTree` - Dominance analysis
- Backend3 lowering - VCode generation

### New Infrastructure Needed
- VCode text formatter (may already exist)
- Filecheck pattern matching library (or simple implementation)
- Test command parser enhancements

---

## References

- Cranelift filetests: `/Users/yona/dev/photomancer/wasmtime/cranelift/filetests/`
- Existing transform tests: `crates/lpc-lpir/tests/filetests/transform/fixed-point.lpir`
- LPIR analysis: `crates/lpc-lpir/src/analysis/`
- LPIR verifier: `crates/lpc-lpir/src/verifier/`

