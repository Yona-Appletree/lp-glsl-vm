# Symbol Table and Relocation Resolution

## Current State

1. **Relocations are recorded but not resolved:**

   - `VCodeReloc` records relocations during lowering (in `crates/lpc-codegen/src/backend3/vcode.rs`)
   - `EmitState.external_relocations` records relocations during emission (in `crates/lpc-codegen/src/isa/riscv32/backend3/emit.rs`)
   - `fix_external_relocations()` is a stub that doesn't actually resolve relocations

2. **No symbol table exists:**

   - Function calls use string-based names (`target: String` in `VCodeReloc`)
   - No mechanism to map function names to addresses/offsets
   - No way to resolve relocations to actual function addresses

3. **Function call emission:**

   - Currently emits `JAL` with placeholder offset 0 (line 733 in `emit.rs`)
   - Needs proper address loading and `JALR` for direct calls

## Reference: Cranelift's Approach

Cranelift uses:

- `ExternalName` enum (User, TestCase, LibCall, KnownSymbol) instead of raw strings
- Relocations tracked in `MachBuffer` with `ExternalName` targets
- Symbol resolution happens at higher level - Cranelift just tracks references
- `TextSectionBuilder` interface allows resolving relocations by providing symbol addresses

For our simpler single-module case, we can use a straightforward approach:

- Simple symbol table: function name -> code offset
- Build symbol table as we emit functions sequentially
- Resolve relocations after all functions are emitted

## Implementation Plan

### 1. Create Symbol Type and Symbol Table Module

**File**: `crates/lpc-codegen/src/backend3/symbols.rs`

Create a symbol type enum (similar to Cranelift's `ExternalName`) and symbol table:

```rust
/// Symbol identifier for functions and external references.
///
/// Similar to Cranelift's ExternalName, this enum supports different kinds
/// of symbols that may need different resolution strategies.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Symbol {
    /// A function defined in the current module (local function).
    /// The string is the function name.
    Local(String),

    /// An external function (defined elsewhere, e.g., in JIT context).
    /// The string is the function name or identifier.
    /// For JIT, this might resolve to a function pointer provided by the runtime.
    External(String),

    /// A test case function name (for testing/debugging).
    /// Similar to Cranelift's TestCase variant.
    TestCase(String),
}

impl Symbol {
    /// Create a local symbol from a function name.
    pub fn local(name: impl Into<String>) -> Self {
        Self::Local(name.into())
    }

    /// Create an external symbol from a function name.
    pub fn external(name: impl Into<String>) -> Self {
        Self::External(name.into())
    }

    /// Create a test case symbol from a name.
    pub fn testcase(name: impl Into<String>) -> Self {
        Self::TestCase(name.into())
    }

    /// Get the name/identifier of this symbol.
    pub fn name(&self) -> &str {
        match self {
            Symbol::Local(name) => name,
            Symbol::External(name) => name,
            Symbol::TestCase(name) => name,
        }
    }
}

/// Symbol table for tracking function addresses/offsets.
///
/// Maps symbols to their code offsets (for local functions) or addresses
/// (for external functions resolved at runtime).
pub struct SymbolTable {
    /// Symbol -> code offset mapping (for local functions)
    local_symbols: BTreeMap<Symbol, u32>,

    /// Symbol -> address mapping (for external functions, resolved at runtime)
    /// Initially empty; populated by the JIT runtime or linker.
    external_symbols: BTreeMap<Symbol, u64>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            local_symbols: BTreeMap::new(),
            external_symbols: BTreeMap::new(),
        }
    }

    /// Add a local symbol (function defined in this module).
    pub fn add_local(&mut self, symbol: Symbol, offset: u32) {
        self.local_symbols.insert(symbol, offset);
    }

    /// Add an external symbol address (for JIT/runtime resolution).
    pub fn add_external(&mut self, symbol: Symbol, address: u64) {
        self.external_symbols.insert(symbol, address);
    }

    /// Look up a symbol's address/offset.
    ///
    /// Returns `None` if the symbol is not found.
    /// For local symbols, returns the code offset.
    /// For external symbols, returns the runtime address.
    pub fn lookup(&self, symbol: &Symbol) -> Option<u64> {
        if let Some(&offset) = self.local_symbols.get(symbol) {
            Some(offset as u64)
        } else if let Some(&address) = self.external_symbols.get(symbol) {
            Some(address)
        } else {
            None
        }
    }

    /// Check if a symbol is external (not defined in this module).
    pub fn is_external(&self, symbol: &Symbol) -> bool {
        !self.local_symbols.contains_key(symbol)
    }
}
```

**Note**: For now, we'll primarily use `Local` symbols. `External` symbols will be used when JIT integration needs to call functions provided by the runtime.

**JIT Integration**: When compiling code for JIT execution, external symbols (e.g., runtime-provided functions) will be resolved by the JIT runtime before execution. The runtime can call `symbol_table.add_external(Symbol::external("runtime_func"), address)` to register these addresses. During relocation resolution, external symbols will use their runtime addresses instead of code offsets.

### 2. Update Relocation Types to Use Symbol

**File**: `crates/lpc-codegen/src/backend3/vcode.rs`

Update `VCodeReloc` to use `Symbol` instead of `String`:

```rust
pub struct VCodeReloc {
    pub inst_idx: InsnIndex,
    pub kind: RelocKind,
    pub target: Symbol,  // Changed from String
}
```

**File**: `crates/lpc-codegen/src/backend3/reloc.rs`

Update `record_reloc()` to accept `Symbol`:

```rust
pub fn record_reloc(
    relocations: &mut alloc::vec::Vec<VCodeReloc>,
    inst_idx: InsnIndex,
    kind: RelocKind,
    target: Symbol,  // Changed from String
) {
    relocations.push(VCodeReloc {
        inst_idx,
        kind,
        target,
    });
}
```

### 3. Integrate Symbol Table into Emission

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/emit.rs`

- Add `SymbolTable` parameter to `VCode::emit()` method
- Build symbol table as functions are emitted (track current offset)
- Pass symbol table to `fix_external_relocations()`

**Changes needed:**

- Modify `emit()` signature to accept `&mut SymbolTable`
- Track function start offsets and register them in symbol table using `Symbol::local(name)`
- Update `EmitState::Reloc` to use `Symbol` instead of `String`
- Update `fix_external_relocations()` to use symbol table

### 4. Implement Relocation Resolution

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/emit.rs`

Implement `fix_external_relocations()` to:

1. Look up function address/offset from symbol table using `symbol_table.lookup(&reloc.target)`
2. Handle different symbol types:
   - **Local symbols**: Use code offset (PC-relative addressing)
   - **External symbols**: Use runtime address (absolute addressing, or PC-relative if address is known)
3. For RISC-V function calls:

   - Load function address into a register (AUIPC + ADDI for PC-relative, or LUI + ADDI for absolute)
   - Replace placeholder JAL with JALR
   - Handle PC-relative addressing (AUIPC for high 20 bits, ADDI for low 12 bits)
   - For external symbols with known absolute addresses, use LUI + ADDI + JALR

**RISC-V call sequence:**

```rust
// Load function address (PC-relative)
AUIPC t0, hi20(function_addr - pc)  // High 20 bits
ADDI  t0, t0, lo12(function_addr)  // Low 12 bits
JALR  ra, t0, 0                      // Call function
```

### 5. Update Function Call Emission

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/emit.rs`

Modify `Jal` emission (around line 696-746):

- Instead of emitting placeholder JAL with 0 offset
- Emit AUIPC + ADDI + JALR sequence
- Record relocation for the AUIPC instruction (needs function address)
- Or emit placeholder and patch later in `fix_external_relocations()`

**Approach**: Emit placeholder sequence during emission, patch in `fix_external_relocations()` for simplicity.

### 6. Handle Multi-Function Compilation

**File**: `crates/lpc-codegen/src/lib.rs` or new compilation entry point

For single-module compilation:

- Create a `SymbolTable` before emitting functions
- Emit functions sequentially, tracking offsets
- Register each function's start offset in symbol table
- After all functions emitted, resolve all relocations

**Note**: This assumes functions are emitted in order. For out-of-order emission, we'd need a two-pass approach (collect all functions first, then emit).

### 7. Update Tests

**File**: `crates/lpc-codegen/src/backend3/tests/emission_tests.rs`

Add tests for:

- Symbol table creation and lookup
- Relocation resolution with known function addresses
- Multi-function compilation with cross-function calls

## Design Decisions

1. **Symbol enum for extensibility**: Following Cranelift's pattern, we use a `Symbol` enum to support different symbol kinds:

   - `Local`: Functions defined in the current module (resolved to code offsets)
   - `External`: Functions provided by the runtime/JIT (resolved to runtime addresses)
   - `TestCase`: Test function names (for testing/debugging)

   This design allows us to add more symbol kinds in the future (e.g., `LibCall`, `KnownSymbol`) without breaking changes.

2. **Separate local and external symbol tables**: The `SymbolTable` maintains separate maps for local symbols (code offsets) and external symbols (runtime addresses). This separation makes it clear which symbols need runtime resolution vs. compile-time resolution.

3. **Sequential emission**: Assume functions are emitted in order. This simplifies symbol table building for local symbols.

4. **Post-emission resolution**: Resolve all relocations after all functions are emitted. This is simpler than incremental resolution. External symbols may be resolved later by the JIT runtime.

5. **RISC-V call sequence**: Use AUIPC + ADDI + JALR for function calls. This is standard RISC-V pattern for PC-relative calls. For external symbols, the runtime address is used directly.

## Future Enhancements

- **External symbol resolution**: When JIT integration needs to call runtime-provided functions, external symbols will be resolved by the JIT runtime before code execution. The symbol table's `add_external()` method allows the runtime to register these addresses.

- **Additional symbol kinds**: Can add more variants to `Symbol` enum as needed:

  - `LibCall`: Well-known library functions (similar to Cranelift)
  - `KnownSymbol`: Platform-specific symbols
  - `User`: User-defined external symbol references (with namespace/index)

- **Multi-module linking**: Would need more sophisticated symbol resolution across modules.

- **Position-independent code (PIC) support**: For code that can be loaded at arbitrary addresses.

- **GOT/PLT support**: For dynamic linking scenarios.

## Files to Modify

1. `crates/lpc-codegen/src/backend3/symbols.rs` (new) - Symbol enum and SymbolTable
2. `crates/lpc-codegen/src/backend3/vcode.rs` (modify) - Update VCodeReloc to use Symbol
3. `crates/lpc-codegen/src/backend3/reloc.rs` (modify) - Update record_reloc to use Symbol
4. `crates/lpc-codegen/src/isa/riscv32/backend3/emit.rs` (modify) - Integrate symbol table and resolve relocations
5. `crates/lpc-codegen/src/backend3/mod.rs` (modify) - Add symbols module
6. `crates/lpc-codegen/src/backend3/tests/emission_tests.rs` (modify) - Add tests for symbol table and relocation resolution
7. `crates/lpc-codegen/src/backend3/vcode_builder.rs` (modify) - Update to use Symbol when recording relocations
