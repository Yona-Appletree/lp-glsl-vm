# LPIR Enhancement: CLIF-Aligned Architecture

## Overview

Enhance LPIR to align with CLIF/standard compiler architecture while maintaining its embedded-friendly, no_std nature. This refactoring establishes a solid foundation for optimizations and future frontends while keeping the codebase modular and well-tested.

## Goals

1. **Foundation**: Implement entity system and linked-list layout (like CLIF)
2. **Modularity**: Break code into focused, testable modules
3. **Testability**: Comprehensive tests for each component
4. **Learning**: Understand proper compiler IR architecture
5. **Extensibility**: Enable optimizations and multiple frontends/ISAs
6. **Maintainability**: Clear separation of concerns, easy to explore

## Key Architectural Changes

### 1. Entity System

- `Block`, `Inst`, `Value` become entity references (like CLIF's `EntityRef`)
- Type-safe entity IDs prevent mixing blocks/instructions/values
- Foundation for efficient entity maps

### 2. Linked-List Layout

- Separate `Layout` struct manages block/instruction ordering
- Doubly-linked lists enable O(1) insertion/deletion
- Enables efficient optimizations (spill/reload insertion, block splitting)

### 3. PrimaryMap Storage

- Dense entity-to-data maps (like CLIF's `PrimaryMap`)
- O(1) lookups, cache-friendly
- Type-safe: `PrimaryMap<Block, BlockData>` vs `PrimaryMap<Inst, InstData>`

### 4. Data Flow Graph Structure

- Separate instruction data from layout
- Uniform instruction structure (opcode + operands)
- Easier to analyze and transform

### 5. Enhanced Verification

- Integrated verifier catches IR violations early
- Comprehensive checks: SSA, dominance, CFG integrity

## File Structure

```
crates/lpc-lpir/src/
├── entity.rs              # EntityRef trait and base types
├── entity_map.rs          # PrimaryMap implementation
├── layout/
│   ├── mod.rs            # Layout public API
│   ├── block_node.rs     # BlockNode (linked list node)
│   ├── inst_node.rs      # InstNode (linked list node)
│   ├── sequence.rs       # Sequence numbers for ordering
│   └── packed_option.rs  # PackedOption for space efficiency
├── dfg/
│   ├── mod.rs            # Data Flow Graph (instruction data)
│   ├── opcode.rs         # Opcode enum
│   └── inst_data.rs      # InstData structure
├── function.rs           # Function (refactored)
├── block.rs              # BlockData (refactored)
├── inst.rs               # Inst enum (kept for compatibility, wraps DFG)
├── value.rs              # Value (enhanced with EntityRef)
├── verifier/
│   ├── mod.rs            # Verifier public API
│   ├── ssa.rs            # SSA validation
│   ├── cfg.rs            # CFG validation
│   ├── dominance.rs      # Dominance validation
│   └── types.rs          # Type checking
└── ... (existing files)
```

## Implementation Phases

### Phase 1: Entity System Foundation

**Goal**: Establish entity references as the foundation for everything else.

#### 1.1 Entity Trait (`entity.rs`)

**File**: `crates/lpc-lpir/src/entity.rs`

**Implementation**:

```rust
//! Entity reference system for type-safe entity IDs.

use core::fmt;

/// Base trait for entity references (like CLIF's EntityRef)
pub trait EntityRef: Copy + Clone + PartialEq + Eq + core::hash::Hash + fmt::Debug {
    /// Get the index of this entity
    fn index(self) -> usize;

    /// Create an entity from an index
    fn from_index(index: usize) -> Self;

    /// Get the next available index (for entity creation)
    fn next_index(self) -> Self {
        Self::from_index(self.index() + 1)
    }
}
```

**Tests**: `entity/tests.rs`

- `test_entity_ref_trait()` - Verify trait methods work
- `test_entity_ordering()` - Verify entities can be ordered
- `test_entity_hashing()` - Verify entities work in hash maps

#### 1.2 Block Entity (`entity.rs`)

**Implementation**:

```rust
/// Block entity reference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Block(u32);

impl EntityRef for Block {
    fn index(self) -> usize { self.0 as usize }
    fn from_index(index: usize) -> Self { Block(index as u32) }
}

impl Block {
    pub fn new(index: u32) -> Self { Block(index) }
}
```

**Tests**:

- `test_block_creation()` - Create blocks from indices
- `test_block_equality()` - Block equality and hashing
- `test_block_ordering()` - Block ordering

#### 1.3 Inst Entity (`entity.rs`)

**Implementation**: Similar to Block

**Tests**: Similar to Block tests

#### 1.4 Value Enhancement (`value.rs`)

**Changes**: Make `Value` implement `EntityRef`

**Tests**: Update existing tests, add `EntityRef` trait tests

**Migration**: Update all code using `Value::new()` to use `EntityRef::from_index()`

---

### Phase 2: PrimaryMap Storage

**Goal**: Implement efficient entity-to-data maps.

#### 2.1 PrimaryMap Implementation (`entity_map.rs`)

**File**: `crates/lpc-lpir/src/entity_map.rs`

**Implementation**:

```rust
//! Dense entity-to-data maps (like CLIF's PrimaryMap).

use alloc::vec::Vec;
use crate::entity::EntityRef;

/// Dense map from entity to data
///
/// This is essentially a Vec with entity-based indexing.
/// Provides O(1) lookups and is cache-friendly.
#[derive(Debug, Clone)]
pub struct PrimaryMap<K: EntityRef, V> {
    data: Vec<V>,
    _phantom: core::marker::PhantomData<K>,
}

impl<K: EntityRef, V> PrimaryMap<K, V> {
    pub fn new() -> Self { /* ... */ }

    pub fn with_capacity(capacity: usize) -> Self { /* ... */ }

    /// Push a value and return its entity key
    pub fn push(&mut self, value: V) -> K { /* ... */ }

    /// Get a value by entity key
    pub fn get(&self, key: K) -> Option<&V> { /* ... */ }

    /// Get a mutable value by entity key
    pub fn get_mut(&mut self, key: K) -> Option<&mut V> { /* ... */ }

    /// Get length
    pub fn len(&self) -> usize { /* ... */ }

    /// Check if empty
    pub fn is_empty(&self) -> bool { /* ... */ }

    /// Iterate over entries
    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> { /* ... */ }

    /// Iterate over values
    pub fn values(&self) -> impl Iterator<Item = &V> { /* ... */ }
}
```

**Tests**: `entity_map/tests.rs`

- `test_primary_map_basic()` - Basic push/get operations
- `test_primary_map_capacity()` - Capacity management
- `test_primary_map_iteration()` - Iterator functionality
- `test_primary_map_type_safety()` - Verify type safety (can't mix Block/Inst maps)
- `test_primary_map_growth()` - Verify map grows correctly
- `test_primary_map_empty()` - Empty map behavior

---

### Phase 3: Linked-List Layout System

**Goal**: Implement CLIF-style linked-list layout for efficient insertion/deletion.

#### 3.1 Sequence Numbers (`layout/sequence.rs`)

**File**: `crates/lpc-lpir/src/layout/sequence.rs`

**Purpose**: BASIC-style line numbers for fast program order comparison

**Implementation**:

```rust
//! Sequence numbers for program order comparison.

/// Sequence number type (BASIC-style: 10, 20, 30...)
pub type SequenceNumber = u32;

/// Initial stride for sequence numbers
pub const MAJOR_STRIDE: SequenceNumber = 10;

/// Minor stride for renumbering
pub const MINOR_STRIDE: SequenceNumber = 2;

/// Limit for local renumbering before full block renumber
pub const LOCAL_LIMIT: SequenceNumber = 100 * MINOR_STRIDE;

/// Compute midpoint between two sequence numbers
pub fn midpoint(a: SequenceNumber, b: SequenceNumber) -> Option<SequenceNumber> {
    debug_assert!(a < b);
    // Avoid overflow, return None if no room
    let m = a + (b - a) / 2;
    if m > a { Some(m) } else { None }
}
```

**Tests**: `layout/sequence/tests.rs`

- `test_midpoint_basic()` - Basic midpoint calculation
- `test_midpoint_edge_cases()` - Edge cases (no room, overflow)
- `test_sequence_ordering()` - Verify sequence numbers maintain order

#### 3.2 PackedOption (`layout/packed_option.rs`)

**File**: `crates/lpc-lpir/src/layout/packed_option.rs`

**Purpose**: Space-efficient Option for entity references (like CLIF's PackedOption)

**Implementation**: Simple wrapper around `Option<T>` for now (can optimize later)

```rust
//! Packed Option for entity references.

use crate::entity::EntityRef;

/// Space-efficient Option for entity references
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PackedOption<T: EntityRef> {
    index: u32,
    _phantom: core::marker::PhantomData<T>,
}

impl<T: EntityRef> PackedOption<T> {
    pub fn none() -> Self { /* ... */ }
    pub fn some(entity: T) -> Self { /* ... */ }
    pub fn expand(self) -> Option<T> { /* ... */ }
    pub fn is_some(&self) -> bool { /* ... */ }
    pub fn is_none(&self) -> bool { /* ... */ }
}
```

**Tests**: `layout/packed_option/tests.rs`

- `test_packed_option_some()` - Some value
- `test_packed_option_none()` - None value
- `test_packed_option_roundtrip()` - Roundtrip conversion

#### 3.3 BlockNode (`layout/block_node.rs`)

**File**: `crates/lpc-lpir/src/layout/block_node.rs`

**Implementation**:

```rust
//! Linked list node for blocks.

use crate::{Block, Inst};
use crate::layout::packed_option::PackedOption;

/// A node in the block linked list
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockNode {
    /// Previous block in layout order
    pub prev: PackedOption<Block>,
    /// Next block in layout order
    pub next: PackedOption<Block>,
    /// First instruction in this block
    pub first_inst: PackedOption<Inst>,
    /// Last instruction in this block
    pub last_inst: PackedOption<Inst>,
    /// Is this block marked as "cold"?
    pub cold: bool,
}
```

**Tests**: `layout/block_node/tests.rs`

- `test_block_node_creation()` - Create empty node
- `test_block_node_links()` - Set/get prev/next links
- `test_block_node_instructions()` - Set/get first/last inst

#### 3.4 InstNode (`layout/inst_node.rs`)

**File**: `crates/lpc-lpir/src/layout/inst_node.rs`

**Implementation**:

```rust
//! Linked list node for instructions.

use crate::{Block, Inst};
use crate::layout::{packed_option::PackedOption, sequence::SequenceNumber};

/// A node in the instruction linked list
#[derive(Clone, Debug, Default)]
pub struct InstNode {
    /// Block containing this instruction
    pub block: PackedOption<Block>,
    /// Previous instruction in the block
    pub prev: PackedOption<Inst>,
    /// Next instruction in the block
    pub next: PackedOption<Inst>,
    /// Sequence number for program order comparison
    pub seq: SequenceNumber,
}
```

**Tests**: `layout/inst_node/tests.rs`

- `test_inst_node_creation()` - Create node
- `test_inst_node_links()` - Set/get prev/next links
- `test_inst_node_block()` - Set/get block
- `test_inst_node_sequence()` - Sequence number assignment

#### 3.5 Layout Implementation (`layout/mod.rs`)

**File**: `crates/lpc-lpir/src/layout/mod.rs`

**Implementation**:

```rust
//! Function layout (block and instruction ordering).

use crate::entity::EntityRef;
use crate::entity_map::PrimaryMap;
use crate::{Block, Inst};
use crate::layout::{
    block_node::BlockNode,
    inst_node::InstNode,
    sequence::{SequenceNumber, MAJOR_STRIDE, MINOR_STRIDE, LOCAL_LIMIT, midpoint},
};

/// Layout manages the ordering of blocks and instructions
///
/// This is separate from the actual instruction data (in DFG).
/// Layout only tracks WHERE instructions are, not WHAT they are.
#[derive(Debug, Clone)]
pub struct Layout {
    /// Linked list nodes for blocks
    blocks: PrimaryMap<Block, BlockNode>,
    /// Linked list nodes for instructions
    insts: PrimaryMap<Inst, InstNode>,
    /// First block in layout order
    first_block: Option<Block>,
    /// Last block in layout order
    last_block: Option<Block>,
}

impl Layout {
    // Block operations
    pub fn new() -> Self { /* ... */ }
    pub fn append_block(&mut self, block: Block) { /* ... */ }
    pub fn insert_block(&mut self, block: Block, before: Block) { /* ... */ }
    pub fn insert_block_after(&mut self, block: Block, after: Block) { /* ... */ }
    pub fn remove_block(&mut self, block: Block) { /* ... */ }
    pub fn is_block_inserted(&self, block: Block) -> bool { /* ... */ }
    pub fn blocks(&self) -> Blocks<'_> { /* ... */ }
    pub fn entry_block(&self) -> Option<Block> { /* ... */ }

    // Instruction operations
    pub fn append_inst(&mut self, inst: Inst, block: Block) { /* ... */ }
    pub fn insert_inst(&mut self, inst: Inst, before: Inst) { /* ... */ }
    pub fn remove_inst(&mut self, inst: Inst) { /* ... */ }
    pub fn inst_block(&self, inst: Inst) -> Option<Block> { /* ... */ }
    pub fn block_insts(&self, block: Block) -> Insts<'_> { /* ... */ }
    pub fn first_inst(&self, block: Block) -> Option<Inst> { /* ... */ }
    pub fn last_inst(&self, block: Block) -> Option<Inst> { /* ... */ }
    pub fn next_inst(&self, inst: Inst) -> Option<Inst> { /* ... */ }
    pub fn prev_inst(&self, inst: Inst) -> Option<Inst> { /* ... */ }

    // Block splitting
    pub fn split_block(&mut self, new_block: Block, before: Inst) { /* ... */ }

    // Program order comparison
    pub fn pp_cmp(&self, a: impl Into<ProgramPoint>, b: impl Into<ProgramPoint>) -> Ordering { /* ... */ }

    // Sequence number management (private)
    fn assign_inst_seq(&mut self, inst: Inst) { /* ... */ }
    fn renumber_insts(&mut self, inst: Inst, seq: SequenceNumber, limit: SequenceNumber) { /* ... */ }
    fn full_block_renumber(&mut self, block: Block) { /* ... */ }
}

/// Iterator over blocks in layout order
pub struct Blocks<'f> {
    layout: &'f Layout,
    next: Option<Block>,
}

/// Iterator over instructions in a block
pub struct Insts<'f> {
    layout: &'f Layout,
    head: Option<Inst>,
    tail: Option<Inst>,
}

/// Program point (block or instruction)
pub enum ProgramPoint {
    Block(Block),
    Inst(Inst),
}
```

**Tests**: `layout/tests.rs`

**Block Tests**:

- `test_layout_new()` - Empty layout
- `test_layout_append_block()` - Append blocks
- `test_layout_insert_block()` - Insert block before another
- `test_layout_insert_block_after()` - Insert block after another
- `test_layout_remove_block()` - Remove block (must be empty)
- `test_layout_blocks_iterator()` - Iterate blocks in order
- `test_layout_entry_block()` - Get entry block
- `test_layout_block_links()` - Verify prev/next links maintained

**Instruction Tests**:

- `test_layout_append_inst()` - Append instructions
- `test_layout_insert_inst()` - Insert instruction before another
- `test_layout_remove_inst()` - Remove instruction
- `test_layout_block_insts_iterator()` - Iterate instructions in block
- `test_layout_inst_block()` - Get block containing instruction
- `test_layout_inst_links()` - Verify prev/next links
- `test_layout_first_last_inst()` - Get first/last instruction

**Block Splitting Tests**:

- `test_layout_split_block_empty()` - Split empty block
- `test_layout_split_block_middle()` - Split at middle instruction
- `test_layout_split_block_beginning()` - Split at first instruction
- `test_layout_split_block_end()` - Split at last instruction
- `test_layout_split_block_instructions_moved()` - Verify instructions moved
- `test_layout_split_block_links_maintained()` - Verify links maintained

**Sequence Number Tests**:

- `test_layout_sequence_numbers()` - Sequence numbers assigned correctly
- `test_layout_sequence_renumbering()` - Renumbering when space runs out
- `test_layout_sequence_midpoint()` - Midpoint calculation for insertion
- `test_layout_pp_cmp()` - Program point comparison

**Edge Case Tests**:

- `test_layout_double_insert_block()` - Can't insert block twice
- `test_layout_double_insert_inst()` - Can't insert inst twice
- `test_layout_remove_nonempty_block()` - Can't remove non-empty block
- `test_layout_remove_uninserted_block()` - Can't remove uninserted block
- `test_layout_insert_inst_uninserted_block()` - Can't insert inst in uninserted block

---

### Phase 4: Data Flow Graph (DFG)

**Goal**: Separate instruction data from layout.

#### 4.1 Opcode Enum (`dfg/opcode.rs`)

**File**: `crates/lpc-lpir/src/dfg/opcode.rs`

**Implementation**:

```rust
//! Instruction opcodes.

use alloc::string::String;

/// Instruction opcode
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Opcode {
    // Arithmetic
    Iadd,
    Isub,
    Imul,
    Idiv,
    Irem,

    // Comparisons
    IcmpEq,
    IcmpNe,
    IcmpLt,
    IcmpLe,
    IcmpGt,
    IcmpGe,

    // Constants
    Iconst,
    Fconst,

    // Control flow
    Jump,
    Br,
    Return,
    Call { callee: String },
    Syscall,
    Halt,

    // Memory
    Load,
    Store,
}
```

**Tests**: `dfg/opcode/tests.rs`

- `test_opcode_equality()` - Opcode equality
- `test_opcode_call_with_name()` - Call opcode with function name

#### 4.2 InstData Structure (`dfg/inst_data.rs`)

**File**: `crates/lpc-lpir/src/dfg/inst_data.rs`

**Implementation**:

```rust
//! Instruction data structure.

use crate::{Value, Block, Type};
use crate::dfg::opcode::Opcode;

/// Instruction data (opcode + operands)
#[derive(Debug, Clone)]
pub struct InstData {
    pub opcode: Opcode,
    /// Input values (arguments)
    pub args: Vec<Value>,
    /// Output values (results, usually 0 or 1)
    pub results: Vec<Value>,
    /// Block arguments for branches/jumps
    pub block_args: Option<BlockArgs>,
    /// Type information (for loads/stores)
    pub ty: Option<Type>,
    /// Immediate values (for constants, syscalls)
    pub imm: Option<Immediate>,
}

/// Block arguments for control flow instructions
#[derive(Debug, Clone)]
pub struct BlockArgs {
    /// For Jump: single target with args
    /// For Br: two targets with args each
    pub targets: Vec<(Block, Vec<Value>)>,
}

/// Immediate values
#[derive(Debug, Clone)]
pub enum Immediate {
    I64(i64),
    F32Bits(u32),
    I32(i32),
    String(String),
}
```

**Tests**: `dfg/inst_data/tests.rs`

- `test_inst_data_arithmetic()` - Arithmetic instruction data
- `test_inst_data_branch()` - Branch instruction with block args
- `test_inst_data_call()` - Call instruction with function name
- `test_inst_data_load_store()` - Load/store with type info

#### 4.3 DFG Module (`dfg/mod.rs`)

**File**: `crates/lpc-lpir/src/dfg/mod.rs`

**Implementation**:

```rust
//! Data Flow Graph (instruction data).

use crate::entity_map::PrimaryMap;
use crate::{Inst, Value};
use crate::dfg::{opcode::Opcode, inst_data::InstData};

/// Data Flow Graph - stores instruction data
#[derive(Debug, Clone)]
pub struct DFG {
    /// Instruction data
    pub insts: PrimaryMap<Inst, InstData>,
    /// Value types (for type checking)
    pub value_types: PrimaryMap<Value, Type>,
}

impl DFG {
    pub fn new() -> Self { /* ... */ }

    pub fn create_inst(&mut self, data: InstData) -> Inst { /* ... */ }

    pub fn inst_data(&self, inst: Inst) -> Option<&InstData> { /* ... */ }

    pub fn inst_data_mut(&mut self, inst: Inst) -> Option<&mut InstData> { /* ... */ }

    pub fn inst_args(&self, inst: Inst) -> &[Value] { /* ... */ }

    pub fn inst_results(&self, inst: Inst) -> &[Value] { /* ... */ }
}
```

**Tests**: `dfg/tests.rs`

- `test_dfg_new()` - Create empty DFG
- `test_dfg_create_inst()` - Create instruction
- `test_dfg_inst_data()` - Get instruction data
- `test_dfg_inst_args_results()` - Get args/results

---

### Phase 5: Function Refactoring

**Goal**: Integrate Layout and DFG into Function.

#### 5.1 BlockData Structure (`block.rs`)

**File**: `crates/lpc-lpir/src/block.rs`

**Refactored Implementation**:

```rust
//! Block data (parameters, metadata).

use crate::Value;

/// Block data (what a block is, separate from layout)
#[derive(Debug, Clone)]
pub struct BlockData {
    /// Block parameters (for phi nodes)
    pub params: Vec<Value>,
}
```

**Tests**: `block/tests.rs`

- `test_block_data_new()` - Create empty block
- `test_block_data_with_params()` - Create block with parameters

#### 5.2 Function Refactoring (`function.rs`)

**File**: `crates/lpc-lpir/src/function.rs`

**Refactored Implementation**:

```rust
//! Functions.

use alloc::string::String;
use crate::signature::Signature;
use crate::entity_map::PrimaryMap;
use crate::{Block, Inst};
use crate::block::BlockData;
use crate::layout::Layout;
use crate::dfg::DFG;

/// A function in the IR
#[derive(Debug, Clone)]
pub struct Function {
    /// Function signature
    pub signature: Signature,
    /// Function name
    pub name: String,
    /// Block data (what blocks are)
    pub blocks: PrimaryMap<Block, BlockData>,
    /// Layout (where blocks/instructions are)
    pub layout: Layout,
    /// Data Flow Graph (what instructions are)
    pub dfg: DFG,
}

impl Function {
    pub fn new(signature: Signature, name: String) -> Self { /* ... */ }

    pub fn create_block(&mut self) -> Block { /* ... */ }

    pub fn create_inst(&mut self, data: InstData) -> Inst { /* ... */ }

    pub fn append_block(&mut self, block: Block) { /* ... */ }

    pub fn append_inst(&mut self, inst: Inst, block: Block) { /* ... */ }

    pub fn entry_block(&self) -> Option<Block> { /* ... */ }
}
```

**Tests**: `function/tests.rs`

- `test_function_new()` - Create empty function
- `test_function_create_block()` - Create and add block
- `test_function_create_inst()` - Create and add instruction
- `test_function_entry_block()` - Get entry block
- `test_function_block_insts()` - Get instructions in block

---

### Phase 6: Enhanced Verifier

**Goal**: Integrate comprehensive verification.

#### 6.1 Verifier Structure (`verifier/mod.rs`)

**File**: `crates/lpc-lpir/src/verifier/mod.rs`

**Implementation**:

```rust
//! IR verifier.

use crate::Function;
use alloc::vec::Vec;
use alloc::string::String;

/// Verifier error
#[derive(Debug, Clone)]
pub struct VerifierError {
    pub message: String,
    pub location: Option<String>,
}

/// Verify a function is well-formed
pub fn verify(function: &Function) -> Result<(), Vec<VerifierError>> {
    let mut errors = Vec::new();

    // Run all checks
    verify_ssa(function, &mut errors);
    verify_cfg(function, &mut errors);
    verify_dominance(function, &mut errors);
    verify_types(function, &mut errors);
    verify_terminators(function, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
```

**Tests**: `verifier/tests.rs`

- `test_verify_valid_function()` - Valid function passes
- `test_verify_invalid_block_index()` - Invalid block index fails
- `test_verify_missing_terminator()` - Missing terminator fails
- `test_verify_ssa_violation()` - SSA violation fails

#### 6.2 SSA Verification (`verifier/ssa.rs`)

**File**: `crates/lpc-lpir/src/verifier/ssa.rs`

**Tests**: `verifier/ssa/tests.rs`

#### 6.3 CFG Verification (`verifier/cfg.rs`)

**File**: `crates/lpc-lpir/src/verifier/cfg.rs`

**Tests**: `verifier/cfg/tests.rs`

#### 6.4 Dominance Verification (`verifier/dominance.rs`)

**File**: `crates/lpc-lpir/src/verifier/dominance.rs`

**Tests**: `verifier/dominance/tests.rs`

#### 6.5 Type Verification (`verifier/types.rs`)

**File**: `crates/lpc-lpir/src/verifier/types.rs`

**Tests**: `verifier/types/tests.rs`

---

### Phase 7: Builder Updates

**Goal**: Update builders to use new structure.

#### 7.1 FunctionBuilder Updates (`builder/function_builder.rs`)

**Changes**:

- Use `Function::create_block()` instead of `Function::add_block()`
- Use `Function::create_inst()` and `Function::append_inst()`
- Return `Block`/`Inst` entities instead of indices

**Tests**: Update existing builder tests

#### 7.2 BlockBuilder Updates (`builder/block_builder.rs`)

**Changes**:

- Work with `Inst` entities
- Use DFG for instruction creation

**Tests**: Update existing builder tests

---

### Phase 8: Parser Updates

**Goal**: Update parser to use new structure.

#### 8.1 Parser Refactoring (`parser/`)

**Changes**:

- Parse into DFG structure
- Create entities during parsing
- Build Layout as instructions are parsed

**Tests**: Update parser tests, ensure all existing tests pass

---

### Phase 9: Migration and Compatibility

**Goal**: Ensure smooth migration from old to new structure.

#### 9.1 Compatibility Layer

**Temporary compatibility**:

- Keep old `Inst` enum for transition period
- Provide conversion functions
- Gradually migrate code

#### 9.2 Update All Tests

**Tasks**:

- Update all existing tests to use new structure
- Ensure test coverage maintained
- Add new tests for new features

---

## Testing Strategy

### Unit Tests

Each module should have comprehensive unit tests:

1. **Happy path tests** - Normal usage
2. **Edge case tests** - Empty collections, single elements, etc.
3. **Error case tests** - Invalid operations, error conditions
4. **Property tests** - Invariants that should always hold

### Integration Tests

Test the full pipeline:

- Parse → Build → Verify → Lower
- Ensure end-to-end functionality works

### Test Organization

```
crates/lpc-lpir/src/
├── entity.rs
├── entity/tests.rs
├── entity_map.rs
├── entity_map/tests.rs
├── layout/
│   ├── mod.rs
│   ├── tests.rs
│   ├── sequence.rs
│   └── sequence/tests.rs
└── ...
```

## Implementation Order

1. **Phase 1**: Entity system (foundation)
2. **Phase 2**: PrimaryMap (storage)
3. **Phase 3**: Layout system (ordering)
4. **Phase 4**: DFG (instruction data)
5. **Phase 5**: Function refactoring (integration)
6. **Phase 6**: Verifier (validation)
7. **Phase 7**: Builder updates (API)
8. **Phase 8**: Parser updates (input)
9. **Phase 9**: Migration (compatibility)

## Success Criteria

1. ✅ All existing tests pass
2. ✅ New tests cover all new functionality
3. ✅ Code is modular and easy to explore
4. ✅ Performance is acceptable (no regressions)
5. ✅ Documentation is clear
6. ✅ Can insert/remove instructions efficiently
7. ✅ Can split blocks efficiently
8. ✅ Verifier catches IR violations

## Notes

- **no_std**: All code must remain no_std compatible
- **Alloc**: Can use `alloc` crate (Vec, String, BTreeMap, etc.)
- **Testing**: Use `#[cfg(test)]` for test modules
- **Documentation**: Add doc comments to all public APIs
- **Error Handling**: Use `Result` types, avoid panics in library code

## Future Enhancements

After this refactoring:

- Add optimizations (dead code elimination, constant folding)
- Add more frontends (GLSL parser, etc.)
- Add more ISAs (if needed)
- Add more sophisticated register allocation
- Add more optimizations leveraging linked-list layout

