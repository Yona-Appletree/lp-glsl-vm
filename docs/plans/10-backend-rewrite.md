# RISC-V 32-bit Backend Implementation Plan

## Overview

Re-implement the RISC-V 32-bit backend emitter for the compiler, closely following cranelift's riscv64 implementation (`/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64`) but adapted for riscv32. The implementation must support:

- Large call sizes (> 8 words)
- Multi-return (> 8 words)
- Proper ABI compliance per `docs/riscv32-abi.md`
- Only 32-bit signed/unsigned integer math (no floating point)

## Architecture

The backend follows cranelift's structure with these key components:

1. **ABI Module** (`backend/abi.rs`) - ABI calculations and argument/return handling
2. **Frame Layout** (`backend/frame.rs`) - Stack frame layout computation
3. **Lowering** (`backend/lower/`) - IR instruction lowering to RISC-V instructions
4. **Register Allocation** (`backend/regalloc.rs`) - Register allocation (may already exist)
5. **Code Emission** (`backend/emit.rs`) - Final instruction emission

## Implementation Phases

### Phase 1: Core Infrastructure

#### 1.1 Frame Layout (`backend/frame.rs`)

**Purpose**: Compute stack frame layout following RISC-V 32-bit ABI.

**Key Structures**:

- `FrameLayout` struct matching cranelift's structure but with `word_bytes: 4` (32-bit)
- Fields: `setup_area_size`, `clobber_size`, `fixed_frame_storage_size`, `stackslots_size`, `outgoing_args_size`, `incoming_args_size`, `tail_args_size`, `clobbered_callee_saves`

**Implementation**:

- `compute_frame_layout()` function following cranelift's logic (lines 630-681 in `abi.rs`)
- Setup area: 8 bytes (FP + RA) for RV32 (vs 16 bytes for RV64)
- Clobber size calculation with 4-byte alignment (vs 8-byte for RV64)
- Stack alignment: 16 bytes (same as RV64)

**Tests**: Unit tests in `backend/tests/frame_tests.rs`:

- Simple frame (no calls, no clobbers)
- Frame with calls
- Frame with clobbered callee-saved registers
- Frame with large outgoing args (> 8 words)
- Frame with incoming stack args

#### 1.2 ABI Module (`backend/abi.rs`)

**Purpose**: Handle argument and return value passing per RISC-V 32-bit ABI.

**Key Functions**:

- `compute_arg_locs()` - Calculate argument locations (registers vs stack)
- `compute_ret_locs()` - Calculate return value locations
- `gen_prologue_frame_setup()` - Generate prologue code
- `gen_epilogue_frame_restore()` - Generate epilogue code
- `gen_clobber_save()` - Save callee-saved registers
- `gen_clobber_restore()` - Restore callee-saved registers

**RV32 Differences from RV64**:

- Word size: 4 bytes (not 8)
- Setup area: 8 bytes (FP at offset 0, RA at offset 4)
- Integer registers: x10-x17 (a0-a7) for args, x10-x11 (a0-a1) for returns
- Stack args: 4-byte aligned (not 8-byte)
- No floating-point support

**Multi-Return Support**:

- When returns > 2 (a0-a1), use return area mechanism
- Caller allocates return area in outgoing args area
- Pass return area pointer as hidden first argument in x10 (a0)
- Callee stores excess returns to return area

**Tests**: Unit tests in `backend/tests/abi_tests.rs`:

- Argument passing: 0-8 args (all in regs)
- Argument passing: 9+ args (some on stack)
- Return values: 0-2 returns (all in regs)
- Return values: 3+ returns (multi-return with return area)
- Large call sizes (> 8 words)
- Multi-return with > 8 words

### Phase 2: Lowering Infrastructure

#### 2.1 Lowerer Structure (`backend/lower/mod.rs`)

**Purpose**: Main lowering entry point, similar to cranelift's `lower.rs`.

**Key Components**:

- `Lowerer` struct to hold lowering state
- `LowerCtx` context for lowering (similar to cranelift's `Lower<Inst>`)
- `lower_function()` - Main entry point
- `lower_inst()` - Lower individual IR instructions
- `lower_block()` - Lower basic blocks

**State Management**:

- Current function being lowered
- Frame layout
- ABI information
- Register allocation results
- Spill/reload plan

#### 2.2 Instruction Lowering (`backend/lower/`)

**Modules** (following cranelift structure):

1. **`arithmetic.rs`** - Lower arithmetic instructions

    - `iadd` → `add`
    - `isub` → `sub`
    - `imul` → `mul`
    - `idiv` → `div`
    - `irem` → `rem`

2. **`comparisons.rs`** - Lower comparison instructions

    - `icmp_eq` → `xor` + `beq`/`bne`
    - `icmp_ne` → `xor` + `bne`
    - `icmp_lt` → `slt` + branch
    - `icmp_le` → `slt` + `xori` + branch
    - `icmp_gt` → `slt` (swapped) + branch
    - `icmp_ge` → `slt` (swapped) + `xori` + branch

3. **`branch.rs`** - Lower control flow

    - `jump` → `jal` or direct jump
    - `br` → conditional branch (`beq`, `bne`, `blt`, `bge`, etc.)
    - Block address tracking

4. **`call.rs`** - Lower function calls

    - Argument preparation (registers + stack)
    - Call instruction (`jal` for direct, `jalr` for indirect)
    - Return value handling (registers + return area)
    - Multi-return support

5. **`return_.rs`** - Lower return instructions

    - Return value preparation
    - Multi-return handling (store to return area)
    - Epilogue generation

6. **`iconst.rs`** - Lower integer constants

    - Small constants: `addi` with immediate
    - Large constants: `lui` + `addi` sequence

7. **`helpers.rs`** - Lowering helper functions

    - Register allocation helpers
    - Spill/reload helpers
    - Address mode helpers

### Phase 3: Prologue and Epilogue

#### 3.1 Prologue Generation (`backend/lower/prologue.rs`)

**Sequence** (per `docs/riscv32-abi.md`):

1. Allocate setup area: `addi sp, sp, -8`
2. Save return address: `sw ra, 4(sp)`
3. Save old FP: `sw fp, 0(sp)`
4. Set new FP: `mv fp, sp`
5. Adjust SP for clobbers + fixed frame + outgoing args
6. Save clobbered callee-saved registers

**Implementation**:

- Follow cranelift's `gen_prologue_frame_setup()` (lines 330-372 in `abi.rs`)
- Adapt for RV32: 8-byte setup area, 4-byte stores (`sw` not `sd`), `I32` type

#### 3.2 Epilogue Generation (`backend/lower/epilogue.rs`)

**Sequence**:

1. Restore clobbered callee-saved registers
2. Restore SP: `addi sp, sp, <stack_size>`
3. Restore RA: `lw ra, 4(sp)`
4. Restore FP: `lw fp, 0(sp)`
5. Restore SP: `addi sp, sp, 8`
6. Return: `ret` (alias for `jalr x0, x1, 0`)

**Implementation**:

- Follow cranelift's `gen_epilogue_frame_restore()` (lines 374-405 in `abi.rs`)
- Adapt for RV32: 4-byte loads (`lw` not `ld`), `I32` type

### Phase 4: Call Lowering

#### 4.1 Call Site Preparation (`backend/lower/call.rs`)

**Caller Side**:

1. **Argument Preparation**:

    - First 8 args → x10-x17 (a0-a7)
    - Additional args → stack (in outgoing args area)
    - Multi-return: Allocate return area, pass pointer in x10

2. **Register Preservation**:

    - Spill caller-saved registers that are live across call
    - Use temporary spill slots

3. **Call Instruction**:

    - Direct call: `jal <target>`
    - Indirect call: `jalr x0, <reg>, 0`

4. **Return Value Handling**:

    - Read register returns from x10-x11
    - Read stack returns from return area

**Callee Side** (in prologue):

1. **Incoming Arguments**:

    - First 8 args from x10-x17
    - Stack args accessed via FP (or SP before frame setup)
    - Multi-return: Receive return area pointer in x10

**Implementation**:

- Follow cranelift's call handling patterns
- Support large call sizes (> 8 words) with proper stack layout
- Support multi-return with return area mechanism

### Phase 5: Integration and Testing

#### 5.1 Module Compilation (`backend/mod.rs`)

**Public API**:

- `compile_module()` - Compile entire module
- `compile_function()` - Compile single function
- `compile_module_to_insts()` - Compile to instruction buffer

**Internal Functions**:

- `compute_liveness()` - Liveness analysis
- `allocate_registers()` - Register allocation
- `create_spill_reload_plan()` - Spill/reload planning

#### 5.2 Test Integration

**Reuse Existing Tests**:

- `backend/tests/call_tests.rs` - Function call tests
- `backend/tests/branch_tests.rs` - Control flow tests
- `backend/tests/comparison_tests.rs` - Comparison tests
- `backend/tests/iconst_tests.rs` - Constant tests

**New Unit Tests**:

- `backend/tests/frame_tests.rs` - Frame layout tests
- `backend/tests/abi_tests.rs` - ABI tests
- `backend/tests/prologue_tests.rs` - Prologue generation tests
- `backend/tests/epilogue_tests.rs` - Epilogue generation tests

## Key Implementation Details

### Register Usage (RV32)

- **Argument registers**: x10-x17 (a0-a7) - 8 registers
- **Return registers**: x10-x11 (a0-a1) - 2 registers
- **Callee-saved**: x8 (fp), x9, x18-x27 (s0-s11)
- **Caller-saved**: x5-x7, x10-x17, x28-x31 (t0-t6)

### Stack Layout (from high to low addresses)

1. **Incoming arguments** (caller's frame)
2. **Setup area** (8 bytes: FP + RA)
3. **Clobber area** (callee-saved registers)
4. **Fixed frame storage** (spill slots, etc.)
5. **Outgoing arguments** (for calls made by this function)

### Multi-Return Mechanism

- When function returns > 2 values:

    1. Caller allocates return area in outgoing args area
    2. Caller passes return area pointer as first argument (x10)
    3. Callee receives pointer in x10
    4. Callee stores excess returns (values 3+) to return area
    5. Callee returns first 2 values in x10-x11
    6. Caller reads register returns, then reads stack returns

### Large Call Sizes

- Arguments beyond 8 are passed on the stack
- Stack arguments are 4-byte aligned
- Outgoing args area must be large enough for all calls made by function
- Frame layout must account for maximum outgoing args size

## File Structure

```
crates/lpc-codegen/src/backend/
├── mod.rs                 # Module entry point, public API
├── abi.rs                 # ABI calculations and helpers
├── frame.rs               # Frame layout computation
├── regalloc.rs            # Register allocation (may exist)
├── liveness.rs            # Liveness analysis (may exist)
├── spill_reload.rs        # Spill/reload planning (may exist)
├── lower/
│   ├── mod.rs            # Lowering entry point
│   ├── arithmetic.rs     # Arithmetic instruction lowering
│   ├── comparisons.rs    # Comparison instruction lowering
│   ├── branch.rs         # Control flow lowering
│   ├── call.rs           # Function call lowering
│   ├── return_.rs        # Return instruction lowering
│   ├── iconst.rs         # Integer constant lowering
│   ├── prologue.rs       # Prologue generation
│   ├── epilogue.rs       # Epilogue generation
│   └── helpers.rs        # Lowering helper functions
└── tests/
    ├── mod.rs
    ├── frame_tests.rs     # NEW: Frame layout unit tests
    ├── abi_tests.rs       # NEW: ABI unit tests
    ├── prologue_tests.rs  # NEW: Prologue tests
    ├── epilogue_tests.rs  # NEW: Epilogue tests
    ├── call_tests.rs      # EXISTING: Integration tests
    ├── branch_tests.rs    # EXISTING: Integration tests
    ├── comparison_tests.rs # EXISTING: Integration tests
    └── iconst_tests.rs    # EXISTING: Integration tests
```

## Critical Requirements

1. **ABI Compliance**: Must strictly follow `docs/riscv32-abi.md`
2. **Large Calls**: Must support > 8 word arguments correctly
3. **Multi-Return**: Must support > 2 return values with return area
4. **Frame Layout**: Must match cranelift's structure (adapted for RV32)
5. **Testability**: Separate unit tests for logic, integration tests for full flows
6. **Code Clarity**: Match cranelift's code organization and naming

## Reference Implementation

- **Cranelift RV64**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64`
- **Key files**:
    - `abi.rs` - ABI implementation (lines 87-681)
    - `lower.rs` - Lowering entry point
    - `inst/emit.rs` - Instruction emission

## Success Criteria

1. All existing tests pass (`backend/tests/*.rs`)
2. New unit tests pass for frame layout and ABI
3. Large call sizes (> 8 words) work correctly
4. Multi-return (> 2 values) works correctly
5. Code structure matches cranelift's organization
6. ABI compliance verified against `docs/riscv32-abi.md`