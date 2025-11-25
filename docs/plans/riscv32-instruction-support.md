# RISC-V 32-bit Instruction Support Analysis

## Current Implementation Status

### Encoder (`riscv32-encoder`)

**Implemented:**

- ✅ Arithmetic: `add`, `sub`, `mul`, `addi`
- ✅ Load/Store: `lw`, `sw`
- ✅ Control Flow: `jal`, `jalr`, `beq`, `bne`, `blt`, `bge`
- ✅ Immediate Generation: `lui`, `auipc`

**Missing:**

- ❌ Bitwise operations: `and`, `or`, `xor`, `andi`, `ori`, `xori`
- ❌ Shift operations: `sll`, `srl`, `sra`, `slli`, `srli`, `srai`
- ❌ Comparison operations: `slt`, `sltu`, `slti`, `sltiu`
- ❌ More arithmetic: `div`, `divu`, `rem`, `remu` (M extension)
- ❌ More load/store: `lb`, `lh`, `lbu`, `lhu`, `sb`, `sh`
- ❌ More branches: `bltu`, `bgeu`
- ❌ Fence instructions: `fence`, `fence.i`
- ❌ System instructions: `ecall`, `ebreak`, `csrrw`, `csrrs`, `csrrc`, etc.

### IR (`r5-ir`)

**Implemented:**

- ✅ Arithmetic: `Iadd`, `Isub`, `Imul`, `Idiv`, `Irem`
- ✅ Comparisons: `IcmpEq`, `IcmpNe`, `IcmpLt`, `IcmpLe`, `IcmpGt`, `IcmpGe`
- ✅ Constants: `Iconst`, `Fconst`
- ✅ Control Flow: `Jump`, `Br`, `Return`
- ✅ Memory: `Load`, `Store`

**Missing:**

- ❌ Bitwise operations: `Iand`, `Ior`, `Ixor`, `Inot`
- ❌ Shift operations: `Ishl`, `Ishr`, `Iashr` (arithmetic shift)
- ❌ Floating point arithmetic: `Fadd`, `Fsub`, `Fmul`, `Fdiv`
- ❌ Floating point comparisons: `FcmpEq`, `FcmpLt`, etc.
- ❌ Type conversions: `Itof`, `Ftoi`, `Itoi` (sign/zero extend)
- ❌ Function calls: `Call` instruction
- ❌ Memory barriers: `Fence`

### Lowerer (`r5-target-riscv32`)

**Implemented:**

- ✅ `Iadd` → `add`
- ✅ `Isub` → `sub`
- ✅ `Imul` → `mul`
- ✅ `Iconst` → `addi` or `lui` + `addi`
- ✅ `Return` → move to `a0` + `jalr`

**Missing (IR instructions not lowered):**

- ❌ `Idiv` → `div`
- ❌ `Irem` → `rem`
- ❌ All comparisons (`Icmp*`) → `slt` + branches
- ❌ `Jump` → `jal` or direct jump
- ❌ `Br` → conditional branches (`beq`, `bne`, etc.)
- ❌ `Load` → `lw` (and variants for different types)
- ❌ `Store` → `sw` (and variants)
- ❌ Function calls (need calling convention)
- ❌ Parameter passing (need to handle function parameters)

## Critical Missing Features

### 1. Control Flow Lowering

**Problem:** `Jump` and `Br` instructions are not lowered, so we can't compile functions with branches or loops.

**What's needed:**

- Lower `Jump` to `jal` or direct jump (if target is known)
- Lower `Br` to conditional branches:
  - `IcmpEq` → `beq`
  - `IcmpNe` → `bne`
  - `IcmpLt` → `slt` + `bne`
  - `IcmpGe` → `slt` + `beq`
  - etc.
- Handle block targets and PC-relative addressing
- Need to track block addresses during code generation

### 2. Memory Operations

**Problem:** `Load` and `Store` are not lowered, so we can't access memory.

**What's needed:**

- Lower `Load` based on type:
  - `Type::I32` → `lw`
  - `Type::I64` → `lw` + `lw` (or use 64-bit loads if available)
  - `Type::F32` → `flw` (if F extension)
  - `Type::F64` → `fld` (if D extension)
- Lower `Store` similarly
- Handle address calculation (base + offset)
- Support for different addressing modes

### 3. Comparison Operations

**Problem:** Comparisons are in IR but not lowered.

**What's needed:**

- Lower `IcmpEq` → `sub` + `beq` or `xor` + `beq`
- Lower `IcmpNe` → `sub` + `bne` or `xor` + `bne`
- Lower `IcmpLt` → `slt` (set less than)
- Lower `IcmpGe` → `slt` + `xori` (invert)
- Lower `IcmpGt` → `slt` with swapped operands
- Lower `IcmpLe` → `slt` with swapped operands + invert
- For branches, use `slt` to set a register, then branch on that

### 4. Function Calls and Calling Convention

**Problem:** No support for calling functions or handling parameters.

**What's needed:**

- Implement RISC-V calling convention:
  - Arguments in `a0`-`a7` (first 8 arguments)
  - Return value in `a0` (and `a1` for 64-bit)
  - Caller-saved registers: `t0`-`t6`, `a0`-`a7`
  - Callee-saved registers: `s0`-`s11`
- Add `Call` instruction to IR
- Lower `Call` to:
  1. Save caller-saved registers (if needed)
  2. Move arguments to `a0`-`a7`
  3. `jalr` to function address
  4. Restore caller-saved registers
  5. Move return value from `a0`
- Handle function parameters in entry block
- Stack frame management (prologue/epilogue)

### 5. Register Allocation

**Problem:** Current `SimpleRegAllocator` is very basic - just assigns registers sequentially.

**What's needed:**

- Proper register allocation algorithm (e.g., linear scan or graph coloring)
- Handle register pressure (spill to stack when needed)
- Respect calling convention (use `a0`-`a7` for arguments, `s0`-`s11` for callee-saved)
- Track live ranges
- Handle register conflicts

### 6. Bitwise and Shift Operations

**Problem:** No bitwise or shift operations in encoder or IR.

**What's needed:**

- Add to IR: `Iand`, `Ior`, `Ixor`, `Inot`, `Ishl`, `Ishr`, `Iashr`
- Add to encoder: `and`, `or`, `xor`, `andi`, `ori`, `xori`, `sll`, `srl`, `sra`, `slli`, `srli`, `srai`
- Lower IR operations to RISC-V instructions

### 7. Division and Remainder

**Problem:** `Idiv` and `Irem` are in IR but not lowered.

**What's needed:**

- Add `div`, `divu`, `rem`, `remu` to encoder (M extension)
- Lower `Idiv` → `div` (signed) or `divu` (unsigned)
- Lower `Irem` → `rem` (signed) or `remu` (unsigned)
- Handle division by zero (may need runtime check)

## Priority Order

### Phase 1: Essential for Basic Programs

1. **Control Flow Lowering** (`Jump`, `Br`) - Required for any non-trivial program
2. **Comparison Lowering** (`Icmp*`) - Required for conditional branches
3. **Memory Operations** (`Load`, `Store`) - Required for data structures

### Phase 2: Essential for Real Programs

4. **Function Calls** - Required for modular code
5. **Parameter Passing** - Required for function calls
6. **Register Allocation** - Required for efficient code

### Phase 3: Complete Basic Support

7. **Bitwise Operations** - Common in many programs
8. **Shift Operations** - Common in many programs
9. **Division/Remainder** - Common arithmetic operations

### Phase 4: Advanced Features

10. **Floating Point** - If needed for your use case
11. **More Load/Store Variants** - For byte/halfword access
12. **More Branch Variants** - For unsigned comparisons

## Implementation Strategy

### For Each Missing Feature:

1. **Add to Encoder** (if instruction doesn't exist):

   - Add encoding function
   - Add tests
   - Verify encoding matches RISC-V spec

2. **Add to IR** (if operation doesn't exist):

   - Add variant to `Inst` enum
   - Update `results()` and `args()` methods
   - Add builder methods

3. **Add to Lowerer**:

   - Add match case in `lower_inst()`
   - Implement lowering logic
   - Handle immediate vs register variants
   - Handle sign/zero extension as needed

4. **Test**:
   - Create test function in IR
   - Compile and verify generated code
   - Run on hardware or VM

## Notes

- RISC-V has a modular design - we're targeting `riscv32imac`:

  - `i` = base integer instructions
  - `m` = multiply/divide extension (M)
  - `a` = atomic instructions (A)
  - `c` = compressed instructions (C) - optional, we may skip for now

- For embedded use, we may not need:

  - Floating point (unless specifically needed)
  - Atomic instructions (unless doing multi-threading)
  - Compressed instructions (nice to have but not essential)

- The most critical missing pieces are:
  1. Control flow (branches/jumps)
  2. Memory operations
  3. Function calls

Without these, we can only compile very simple arithmetic functions.
