# Backend3 Phase 4: Control Flow

**Goal**: Branches and calls

**Timeline**: Week 4

**Deliverable**: Can compile functions with branches and calls

## Tasks

### 1. Branch lowering

**Components**:

- Lower Jump and Br instructions
- Handle block parameters (phi moves in edge blocks)
- Two-dest branch representation
- Record branch relocations

**See**: Main plan for branch lowering details (`17-backend3.md`)

### 2. Branch resolution (ISA-agnostic)

**File**: `backend3/branch.rs`

**Components**:

- Resolve two-dest branches
- Convert to single-dest branches during emission
- Basic branch simplification (fallthrough optimization)

**See**: Main plan for branch resolution details (`17-backend3.md`)

### 3. Call lowering

**Components**:

- Lower Call instructions
- Argument preparation (registers + stack)
- Return value handling
- Record function call relocations

**See**: Main plan for call handling (`17-backend3.md`)

### 4. Multi-return support

**Components**:

- Return area mechanism
- Handle >2 return values
- Return area pointer passing

**See**: Main plan for multi-return details (`17-backend3.md`)

### 5. Relocation fixup (ISA-agnostic)

**File**: `backend3/reloc.rs`

**Components**:

- Relocation handling
- Fix function call addresses
- Resolve branch targets

**See**: Main plan for relocation handling (`17-backend3.md`)

## Testing

- Unit tests for branch lowering
- Unit tests for branch resolution
- Unit tests for call lowering
- Unit tests for multi-return
- Unit tests for relocation fixup
- Integration test: Compile function with branches
- Integration test: Compile function with calls
- Integration test: Compile function with multi-return

## Success Criteria

- ✅ Can lower branches (Jump, Br)
- ✅ Can resolve two-dest branches to single-dest
- ✅ Can lower function calls
- ✅ Can handle multi-return (>2 values)
- ✅ Can fix up relocations
- ✅ Can compile functions with branches and calls end-to-end
