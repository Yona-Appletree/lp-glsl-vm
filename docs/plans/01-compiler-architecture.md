# Compiler Architecture Design

## Overview

Design a clean, modular, no_std-compatible compiler architecture for generating RISC-V 32-bit code. Inspired by modern compiler designs (LLVM, Cranelift) but built from scratch to be lightweight and strictly no_std compatible.

## Design Principles

1. **no_std First**: All components must work without std, using only `core` and `alloc`
2. **Modular**: Clear separation of concerns with well-defined interfaces
3. **Extensible**: Easy to add new optimizations, targets, or features
4. **Simple**: Start simple, add complexity only when needed
5. **Well-Tested**: Comprehensive tests at each layer

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────┐
│                    Frontend / IR Builder                    │
│  - Build IR from high-level code                           │
│  - SSA construction                                        │
│  - Variable management                                      │
└───────────────────────────┬───────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    IR (Intermediate Representation)         │
│  - SSA form                                                │
│  - Basic blocks                                            │
│  - Instructions                                            │
│  - Types                                                   │
└───────────────────────────┬───────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Optimizer (Optional)                     │
│  - Dead code elimination                                   │
│  - Constant folding                                         │
│  - Simple optimizations                                     │
└───────────────────────────┬───────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Backend                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │ Instruction │  │   Register   │  │    Code      │     │
│  │  Selection  │→ │  Allocation  │→ │   Emission    │     │
│  └──────────────┘  └──────────────┘  └──────────────┘     │
└───────────────────────────┬───────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Target (RISC-V 32-bit)                  │
│  - Instruction encoding                                     │
│  - Calling conventions                                      │
│  - ELF generation                                           │
└─────────────────────────────────────────────────────────────┘
```

## Component Design

### 1. IR (Intermediate Representation)

**Crate**: `r5-ir` (RISC-V 5 IR)

**Core Types**:
- `Function`: A function with basic blocks, instructions, signature
- `Block`: A basic block (single entry, single exit)
- `Inst`: An instruction (opcode + operands)
- `Value`: An SSA value (result of an instruction)
- `Type`: Type system (i32, i64, f32, f64, etc.)

**Key Properties**:
- SSA form (Static Single Assignment)
- Basic blocks with explicit control flow
- Type-safe value representation
- Immutable IR (transformations create new IR)

**Example IR Structure**:
```rust
Function {
    signature: Signature { params: [i32, i32], returns: [i32] },
    blocks: [
        Block {
            params: [Value(0), Value(1)],  // function parameters
            insts: [
                Inst::Iadd { result: Value(2), args: [Value(0), Value(1)] },
                Inst::Return { args: [Value(2)] },
            ],
        }
    ],
}
```

**Instruction Set** (Initial):
- Arithmetic: `iadd`, `isub`, `imul`, `idiv`, `irem`
- Comparison: `icmp_eq`, `icmp_ne`, `icmp_lt`, `icmp_le`, `icmp_gt`, `icmp_ge`
- Control flow: `jump`, `br` (conditional branch), `return`
- Memory: `load`, `store`
- Constants: `iconst`, `fconst`

### 2. Frontend / IR Builder

**Crate**: `r5-builder`

**Purpose**: Build IR from high-level code or API calls

**Key Components**:
- `FunctionBuilder`: Build a function's IR
- `BlockBuilder`: Build instructions within a block
- `SSABuilder`: Handle SSA construction for variables

**API Design**:
```rust
let mut func_builder = FunctionBuilder::new();
let block = func_builder.create_block();
let mut block_builder = func_builder.block_builder(block);

// Declare variables
let a = func_builder.declare_var(Type::I32);
let b = func_builder.declare_var(Type::I32);

// Build instructions
let a_val = block_builder.use_var(a);
let b_val = block_builder.use_var(b);
let sum = block_builder.iadd(a_val, b_val);
block_builder.def_var(sum_var, sum);
block_builder.return_(&[sum]);

let func = func_builder.finish();
```

### 3. Optimizer

**Crate**: `r5-opt`

**Purpose**: Transform IR to better IR (optimizations)

**Design**: Pass-based architecture
- Each pass is a function: `fn(Function) -> Function`
- Passes can be composed: `pass1.and_then(pass2).and_then(pass3)`

**Initial Passes**:
- `DeadCodeElimination`: Remove unused instructions
- `ConstantFolding`: Evaluate constant expressions
- `SimplifyCFG`: Simplify control flow

**Future Passes**:
- `InstructionCombining`: Combine simple instructions
- `LoopOptimizations`: Loop-specific optimizations
- `RegisterCoalescing`: Coalesce related values

### 4. Backend

**Crate**: `r5-backend`

**Purpose**: Target-agnostic instruction selection, register allocation, and code emission

**Key Components**:

#### Instruction Selection / Lowering
- Map IR instructions to target-specific instructions
- Handle instruction patterns (e.g., `iadd` → `add` for RISC-V)
- Handle complex operations (e.g., division → function call)

#### Register Allocation
- Simple linear scan allocator (start simple)
- Map SSA values to physical registers or stack slots
- Handle register pressure and spilling

#### Code Emission
- Generate machine code from allocated instructions
- Handle relocations and fixups
- Generate metadata (debug info, etc.)

**Interface**:
```rust
trait Backend {
    type Target;
    
    fn lower(&self, func: &Function) -> TargetFunction;
    fn allocate_registers(&self, func: &TargetFunction) -> AllocatedFunction;
    fn emit(&self, func: &AllocatedFunction) -> Vec<u8>;
}
```

### 5. Target (RISC-V 32-bit)

**Crate**: `r5-target-riscv32`

**Purpose**: RISC-V 32-bit specific implementation

**Key Components**:

#### Instruction Encoding
- Use `riscv32-encoder` crate for encoding instructions
- Map abstract instructions to RISC-V opcodes
- Handle instruction formats (R, I, S, U, J, B)

#### Calling Convention
- RISC-V calling convention (ABI)
- Register usage (a0-a7 for args, a0-a1 for returns)
- Stack frame layout
- Prologue/epilogue generation

#### ELF Generation
- Generate ELF32 files
- Section headers (.text, .data, etc.)
- Relocations
- Symbol table

### 6. Instruction Encoder

**Crate**: `riscv32-encoder`

**Purpose**: Low-level RISC-V 32-bit instruction encoding

**Features**:
- Encode all RISC-V instruction formats (R, I, S, U, J, B)
- Register definitions
- Instruction builders
- Validation

**Note**: This crate was previously created and tested, but deleted during rollback. We'll recreate it.

## Implementation Plan

### Phase 1: Core IR (Foundation)

1. **Create `r5-ir` crate**
   - Define `Type` enum (i32, i64, f32, f64)
   - Define `Value` (SSA value identifier)
   - Define `Inst` enum (all instruction types)
   - Define `Block` (basic block with instructions)
   - Define `Function` (function with blocks and signature)
   - Define `Signature` (function signature: params + returns)
   - Write comprehensive tests

2. **Create `riscv32-encoder` crate**
   - Recreate instruction encoder
   - Support all instruction formats
   - Register definitions
   - Tests for encoding correctness

### Phase 2: IR Builder

3. **Create `r5-builder` crate**
   - `FunctionBuilder` for building functions
   - `BlockBuilder` for building blocks
   - SSA construction helpers
   - Variable management
   - Tests

### Phase 3: Backend (Target-Agnostic)

4. **Create `r5-backend` crate**
   - Define backend traits/interfaces
   - Instruction selection interface
   - Register allocation interface
   - Code emission interface

### Phase 4: RISC-V Target

5. **Create `r5-target-riscv32` crate**
   - Implement backend traits for RISC-V
   - Instruction lowering (IR → RISC-V)
   - Simple register allocator (linear scan)
   - Code emission using `riscv32-encoder`
   - ELF generation
   - Tests

### Phase 5: Optimizer (Optional, Later)

6. **Create `r5-opt` crate**
   - Dead code elimination pass
   - Constant folding pass
   - Pass composition utilities
   - Tests

### Phase 6: Integration

7. **Update `embive-program`**
   - Use new compiler architecture in `jit_add_experiment`
   - Build IR for add function
   - Compile to RISC-V
   - Generate ELF
   - Transpile and execute

## Crate Structure

```
crates/
├── r5-ir/                    # Core IR types
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── types.rs          # Type system
│       ├── value.rs          # SSA values
│       ├── inst.rs           # Instructions
│       ├── block.rs          # Basic blocks
│       ├── function.rs       # Functions
│       └── signature.rs      # Function signatures
│
├── r5-builder/               # IR builder
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── function_builder.rs
│       ├── block_builder.rs
│       └── ssa.rs            # SSA construction
│
├── r5-backend/               # Backend (target-agnostic)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── traits.rs         # Backend traits
│       ├── selection.rs      # Instruction selection interface
│       ├── regalloc.rs       # Register allocation interface
│       └── emission.rs       # Code emission interface
│
├── r5-target-riscv32/        # RISC-V 32-bit target
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── lower.rs          # Instruction lowering
│       ├── regalloc.rs       # Register allocator
│       ├── emit.rs           # Code emission
│       ├── calling_conv.rs  # Calling convention
│       └── elf.rs           # ELF generation
│
├── r5-opt/                   # Optimizer (optional, later)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── dce.rs            # Dead code elimination
│       ├── const_fold.rs     # Constant folding
│       └── simplify_cfg.rs   # CFG simplification
│
└── riscv32-encoder/          # RISC-V instruction encoder
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── regs.rs           # Register definitions
        └── encode.rs         # Instruction encoding
```

## Dependencies

### `r5-ir`
- `no_std` compatible
- Uses `alloc` for collections

### `r5-builder`
- Depends on: `r5-ir`
- `no_std` compatible

### `r5-backend`
- Depends on: `r5-ir`
- `no_std` compatible

### `r5-target-riscv32`
- Depends on: `r5-ir`, `r5-backend`, `riscv32-encoder`
- `no_std` compatible

### `r5-opt`
- Depends on: `r5-ir`
- `no_std` compatible

### `riscv32-encoder`
- Pure `no_std` (no `alloc` needed)
- No dependencies

## Testing Strategy

1. **Unit Tests**: Each crate has comprehensive unit tests
2. **Integration Tests**: Test full compilation pipeline
3. **End-to-End Tests**: Test in `embive-program` with real execution
4. **Property Tests**: Test instruction encoding correctness

## Future Considerations

- **More Targets**: Could add other targets (ARM, x86, etc.)
- **More Optimizations**: Add more optimization passes as needed
- **Debug Info**: Add debug information generation
- **Error Messages**: Improve error messages and diagnostics
- **Performance**: Profile and optimize hot paths

## Notes

- Start simple, iterate based on needs
- Focus on correctness first, performance second
- Keep each crate focused and testable
- Document public APIs thoroughly
- Follow Rust best practices (clippy, rustfmt)

