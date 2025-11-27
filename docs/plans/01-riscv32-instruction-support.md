# RISC-V 32-bit Instruction Support Implementation Plan

## Overview

This plan outlines the implementation of full RISC-V 32-bit instruction support for the compiler. The work is organized into phases, with each phase building on the previous one.

## Phase 1: Essential Control Flow and Memory

**Goal:** Enable compilation of programs with branches, loops, and memory access.

### 1.1 Control Flow Lowering

**Tasks:**

- [ ] Lower `Jump` instruction to `jal` or direct jump
- [ ] Lower `Br` instruction to conditional branches
- [ ] Track block addresses during code generation
- [ ] Handle PC-relative addressing for branches
- [ ] Test with simple if/else and loops

**Implementation:**

- Modify `Lowerer::lower_inst()` to handle `Jump` and `Br`
- Add block address tracking to `CodeBuffer` or `Lowerer`
- Lower `Br` based on comparison type:
  - `IcmpEq` → `beq`
  - `IcmpNe` → `bne`
  - `IcmpLt` → `slt` + `bne`
  - `IcmpGe` → `slt` + `beq`
  - etc.

**Files to modify:**

- `crates/r5-target-riscv32/src/lower.rs`
- `crates/r5-target-riscv32/src/emit.rs` (may need block tracking)

**Tests:**

- Simple if/else: `if a > b { return a } else { return b }`
- Simple loop: `while i < 10 { i = i + 1 }`

### 1.2 Comparison Lowering

**Tasks:**

- [ ] Lower comparison instructions (`Icmp*`) to RISC-V `slt` instructions
- [ ] Handle signed vs unsigned comparisons
- [ ] Lower comparisons used in branches vs comparisons used as values

**Implementation:**

- Add `slt`, `sltu`, `slti`, `sltiu` to encoder
- Lower `IcmpLt` → `slt` (signed) or `sltu` (unsigned)
- Lower `IcmpGe` → `slt` + `xori` (invert)
- Lower `IcmpGt` → `slt` with swapped operands
- Lower `IcmpLe` → `slt` with swapped operands + invert
- Lower `IcmpEq` → `sub` + `beq` or `xor` + `beq`
- Lower `IcmpNe` → `sub` + `bne` or `xor` + `bne`

**Files to modify:**

- `crates/lpc-riscv32/src/encode.rs` (add `slt`, `sltu`, `slti`, `sltiu`)
- `crates/r5-target-riscv32/src/lower.rs` (lower comparisons)

**Tests:**

- Comparison in branch: `if a < b { ... }`
- Comparison as value: `let c = a < b`

### 1.3 Memory Operations

**Tasks:**

- [ ] Lower `Load` instruction based on type
- [ ] Lower `Store` instruction based on type
- [ ] Handle address calculation (base + offset)
- [ ] Support different addressing modes

**Implementation:**

- Lower `Load` with `Type::I32` → `lw`
- Lower `Load` with `Type::I64` → `lw` + `lw` (or 64-bit loads if available)
- Lower `Load` with `Type::F32` → `flw` (if F extension)
- Lower `Load` with `Type::F64` → `fld` (if D extension)
- Lower `Store` similarly
- Handle immediate offsets vs register offsets
- For large offsets, use `lui` + `addi` to form address

**Files to modify:**

- `crates/r5-target-riscv32/src/lower.rs` (lower `Load` and `Store`)

**Tests:**

- Load/store i32: `let x = *ptr; *ptr = 42;`
- Load/store arrays: `arr[i] = arr[i] + 1`

## Phase 2: Function Calls and Calling Convention

**Goal:** Enable modular code with function calls.

### 2.1 RISC-V Calling Convention

**Tasks:**

- [ ] Document and implement RISC-V calling convention
- [ ] Argument passing (`a0`-`a7` for first 8 arguments)
- [ ] Return value handling (`a0` for single return, `a0`+`a1` for 64-bit)
- [ ] Register usage conventions (caller-saved vs callee-saved)

**Implementation:**

- Document calling convention in code comments
- Define register roles:
  - Arguments: `a0`-`a7` (x10-x17)
  - Return: `a0` (x10), `a1` (x11) for 64-bit
  - Caller-saved: `t0`-`t6` (x5-x7, x28-x31), `a0`-`a7`
  - Callee-saved: `s0`-`s11` (x8-x9, x18-x27)
  - Stack pointer: `sp` (x2)
  - Frame pointer: `fp` (x8, same as `s0`)

**Files to create/modify:**

- `crates/r5-target-riscv32/src/calling_convention.rs` (new)
- `crates/r5-target-riscv32/src/lib.rs` (export)

### 2.2 Function Call IR and Lowering

**Tasks:**

- [ ] Add `Call` instruction to IR
- [ ] Lower `Call` to function invocation
- [ ] Handle argument passing
- [ ] Handle return value

**Implementation:**

- Add `Inst::Call { callee: FunctionId, args: Vec<Value>, result: Value }` to IR
- Lower `Call`:
  1. Save caller-saved registers (if needed)
  2. Move arguments to `a0`-`a7`
  3. `jalr` to function address (or `jal` if direct call)
  4. Restore caller-saved registers
  5. Move return value from `a0` to result register
- Handle function addresses (may need relocation)

**Files to modify:**

- `crates/lpc-lpir/src/inst.rs` (add `Call`)
- `crates/r5-builder/src/block_builder.rs` (add `call` method)
- `crates/r5-target-riscv32/src/lower.rs` (lower `Call`)

**Tests:**

- Simple call: `let result = add(5, 10);`
- Call with multiple args: `let result = max(a, b, c);`

### 2.3 Function Parameters and Entry

**Tasks:**

- [ ] Handle function parameters in entry block
- [ ] Generate function prologue
- [ ] Generate function epilogue
- [ ] Stack frame setup

**Implementation:**

- Entry block parameters come from `a0`-`a7`
- Prologue:
  - Save callee-saved registers if used
  - Set up frame pointer if needed
  - Allocate stack space for locals
- Epilogue:
  - Restore callee-saved registers
  - Restore frame pointer
  - Return via `jalr x0, x1, 0`

**Files to modify:**

- `crates/r5-target-riscv32/src/lower.rs` (handle entry block specially)

**Tests:**

- Function with parameters: `fn add(a: i32, b: i32) -> i32 { a + b }`
- Function with many parameters: `fn f(a: i32, b: i32, ..., h: i32) -> i32`

### 2.4 Stack Frame Management

**Tasks:**

- [ ] Track stack frame size
- [ ] Generate stack allocation
- [ ] Handle stack-allocated variables
- [ ] Support for register spilling

**Implementation:**

- Track stack frame size in `Lowerer`
- Generate `addi sp, sp, -frame_size` in prologue
- Generate `addi sp, sp, frame_size` in epilogue
- Use `sp`-relative addressing for stack slots
- For spilling: allocate stack slot, use `sw`/`lw` to save/restore

**Files to modify:**

- `crates/r5-target-riscv32/src/lower.rs` (stack frame tracking)
- `crates/r5-target-riscv32/src/regalloc.rs` (spill support)

**Tests:**

- Function with many locals (force spilling)
- Nested function calls

## Phase 3: Register Allocation

**Goal:** Efficient register usage with spilling support.

### 3.1 Linear Scan Register Allocator

**Tasks:**

- [ ] Implement linear scan algorithm
- [ ] Track live ranges
- [ ] Handle register conflicts
- [ ] Implement spilling

**Implementation:**

- Replace `SimpleRegAllocator` with `LinearScanRegAllocator`
- Track live ranges for each value
- Allocate registers in order, evicting when needed
- Spill to stack when no registers available
- Use callee-saved registers (`s0`-`s11`) for long-lived values

**Files to modify:**

- `crates/r5-target-riscv32/src/regalloc.rs` (complete rewrite)

**Tests:**

- Function with many variables (test spilling)
- Function with nested scopes

### 3.2 Register Allocation Improvements

**Tasks:**

- [ ] Prefer caller-saved registers for short-lived values
- [ ] Prefer callee-saved registers for long-lived values
- [ ] Coalesce moves when possible
- [ ] Handle phi nodes properly

**Implementation:**

- Analyze value lifetimes
- Assign registers based on lifetime
- Coalesce `add rd, rs, x0` moves when possible
- Handle phi nodes by ensuring same register for merged values

**Files to modify:**

- `crates/r5-target-riscv32/src/regalloc.rs`

## Phase 4: Arithmetic Operations

**Goal:** Complete arithmetic instruction support.

### 4.1 Division and Remainder

**Tasks:**

- [ ] Add `div`, `divu`, `rem`, `remu` to encoder
- [ ] Lower `Idiv` to `div`/`divu`
- [ ] Lower `Irem` to `rem`/`remu`
- [ ] Handle division by zero (optional runtime check)

**Implementation:**

- Add encoding functions for M extension division instructions
- Lower `Idiv` → `div` (signed) or `divu` (unsigned)
- Lower `Irem` → `rem` (signed) or `remu` (unsigned)
- Consider adding runtime check for division by zero (optional)

**Files to modify:**

- `crates/lpc-riscv32/src/encode.rs` (add `div`, `divu`, `rem`, `remu`)
- `crates/r5-target-riscv32/src/lower.rs` (lower `Idiv`, `Irem`)

**Tests:**

- `let result = a / b;`
- `let result = a % b;`
- Handle division by zero gracefully

### 4.2 Bitwise Operations

**Tasks:**

- [ ] Add bitwise operations to IR (`Iand`, `Ior`, `Ixor`, `Inot`)
- [ ] Add encoding functions (`and`, `or`, `xor`, `andi`, `ori`, `xori`)
- [ ] Lower IR operations to RISC-V instructions

**Implementation:**

- Add `Iand`, `Ior`, `Ixor`, `Inot` to `Inst` enum
- Add `and`, `or`, `xor`, `andi`, `ori`, `xori` to encoder
- Lower:
  - `Iand` → `and` or `andi` (if immediate)
  - `Ior` → `or` or `ori` (if immediate)
  - `Ixor` → `xor` or `xori` (if immediate)
  - `Inot` → `xori` with -1

**Files to modify:**

- `crates/lpc-lpir/src/inst.rs` (add bitwise operations)
- `crates/r5-builder/src/block_builder.rs` (add builder methods)
- `crates/lpc-riscv32/src/encode.rs` (add encoding functions)
- `crates/r5-target-riscv32/src/lower.rs` (lower bitwise operations)

**Tests:**

- `let result = a & b;`
- `let result = a | b;`
- `let result = a ^ b;`
- `let result = !a;`

### 4.3 Shift Operations

**Tasks:**

- [ ] Add shift operations to IR (`Ishl`, `Ishr`, `Iashr`)
- [ ] Add encoding functions (`sll`, `srl`, `sra`, `slli`, `srli`, `srai`)
- [ ] Lower IR operations to RISC-V instructions

**Implementation:**

- Add `Ishl`, `Ishr`, `Iashr` to `Inst` enum
- Add `sll`, `srl`, `sra`, `slli`, `srli`, `srai` to encoder
- Lower:
  - `Ishl` → `sll` or `slli` (if immediate)
  - `Ishr` → `srl` or `srli` (if immediate, logical shift)
  - `Iashr` → `sra` or `srai` (if immediate, arithmetic shift)

**Files to modify:**

- `crates/lpc-lpir/src/inst.rs` (add shift operations)
- `crates/r5-builder/src/block_builder.rs` (add builder methods)
- `crates/lpc-riscv32/src/encode.rs` (add encoding functions)
- `crates/r5-target-riscv32/src/lower.rs` (lower shift operations)

**Tests:**

- `let result = a << 2;`
- `let result = a >> 2;` (logical)
- `let result = a >> 2;` (arithmetic)

## Phase 5: Additional Instructions

**Goal:** Complete instruction set coverage.

### 5.1 More Load/Store Variants

**Tasks:**

- [ ] Add byte/halfword load/store (`lb`, `lh`, `lbu`, `lhu`, `sb`, `sh`)
- [ ] Lower based on type size
- [ ] Handle sign/zero extension

**Implementation:**

- Add `lb`, `lh`, `lbu`, `lhu`, `sb`, `sh` to encoder
- Lower `Load` with smaller types:
  - `i8` → `lb` (signed) or `lbu` (unsigned)
  - `i16` → `lh` (signed) or `lhu` (unsigned)
- Handle sign extension for signed loads
- Lower `Store` similarly

**Files to modify:**

- `crates/lpc-riscv32/src/encode.rs` (add load/store variants)
- `crates/r5-target-riscv32/src/lower.rs` (lower based on type)

**Tests:**

- Load/store bytes: `let x: i8 = *ptr; *ptr = 42i8;`
- Load/store halfwords: `let x: i16 = *ptr; *ptr = 42i16;`

### 5.2 More Branch Variants

**Tasks:**

- [ ] Add unsigned comparison branches (`bltu`, `bgeu`)
- [ ] Lower unsigned comparisons properly

**Implementation:**

- Add `bltu`, `bgeu` to encoder
- Lower `IcmpLt` with unsigned flag → `bltu`
- Lower `IcmpGe` with unsigned flag → `bgeu`

**Files to modify:**

- `crates/lpc-riscv32/src/encode.rs` (add `bltu`, `bgeu`)
- `crates/r5-target-riscv32/src/lower.rs` (handle unsigned comparisons)

**Tests:**

- Unsigned comparison: `if (a as u32) < (b as u32) { ... }`

### 5.3 System Instructions

**Tasks:**

- [ ] Add `ecall`, `ebreak` for system calls
- [ ] Add `fence`, `fence.i` for memory ordering
- [ ] Add CSR instructions if needed (`csrrw`, `csrrs`, `csrrc`)

**Implementation:**

- Add `ecall`, `ebreak` to encoder
- Add `fence`, `fence.i` to encoder
- Add CSR instructions if needed for your use case

**Files to modify:**

- `crates/lpc-riscv32/src/encode.rs` (add system instructions)

**Tests:**

- System call: `ecall()`
- Memory fence: `fence.i`

## Phase 6: Floating Point (Optional)

**Goal:** Support floating point operations if needed.

### 6.1 Floating Point IR Operations

**Tasks:**

- [ ] Add floating point arithmetic to IR (`Fadd`, `Fsub`, `Fmul`, `Fdiv`)
- [ ] Add floating point comparisons (`FcmpEq`, `FcmpLt`, etc.)
- [ ] Add type conversions (`Itof`, `Ftoi`)

**Implementation:**

- Add floating point operations to `Inst` enum
- Add builder methods
- Lower to RISC-V F extension instructions (`fadd.s`, `fsub.s`, etc.)

**Files to modify:**

- `crates/lpc-lpir/src/inst.rs` (add FP operations)
- `crates/r5-builder/src/block_builder.rs` (add builder methods)
- `crates/lpc-riscv32/src/encode.rs` (add FP encoding)
- `crates/r5-target-riscv32/src/lower.rs` (lower FP operations)

**Tests:**

- `let result = a + b;` (f32)
- `let result = a * b;` (f32)
- `let result = (a as f32) + 1.0;`

## Implementation Notes

### Testing Strategy

For each phase:

1. Write test functions in IR using the builder
2. Compile to RISC-V code
3. Verify generated code matches expected instructions
4. Run on hardware or VM to verify correctness

### Code Organization

- Keep encoder functions simple and well-tested
- Lowerer should handle all edge cases (immediates, sign extension, etc.)
- Register allocator should be modular and replaceable

### Performance Considerations

- Prefer immediate instructions when possible (`addi` vs `add`)
- Use callee-saved registers for long-lived values
- Minimize register moves
- Optimize constant generation (use `lui` + `addi` efficiently)

## Success Criteria

Phase 1 complete when:

- ✅ Can compile functions with if/else and loops
- ✅ Can compile functions with memory access
- ✅ Can compile functions with comparisons

Phase 2 complete when:

- ✅ Can compile functions that call other functions
- ✅ Can compile functions with multiple parameters
- ✅ Stack frames work correctly

Phase 3 complete when:

- ✅ Register allocation handles register pressure
- ✅ Spilling works correctly
- ✅ Code is reasonably efficient

Phase 4 complete when:

- ✅ All basic arithmetic operations work
- ✅ Bitwise and shift operations work
- ✅ Division and remainder work

Phase 5 complete when:

- ✅ All common RISC-V instructions are supported
- ✅ Code generation is complete for typical programs

Phase 6 complete when:

- ✅ Floating point operations work (if needed)

## Estimated Effort

- Phase 1: 2-3 days (critical path)
- Phase 2: 3-4 days (complex but essential)
- Phase 3: 2-3 days (important for efficiency)
- Phase 4: 2-3 days (straightforward)
- Phase 5: 1-2 days (mostly adding encodings)
- Phase 6: 2-3 days (if needed)

Total: ~12-18 days for complete basic support, ~7-10 days for essential support (Phases 1-2).



