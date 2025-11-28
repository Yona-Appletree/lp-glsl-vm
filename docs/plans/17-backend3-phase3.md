# Backend3 Phase 3: Emission

**Goal**: Generate machine code

**Timeline**: Week 3

**Deliverable**: Can compile simple functions end-to-end

## Tasks

### 1. Emission state tracking (ISA-agnostic)

**Components**:

- Track SP offsets for stack-relative addressing
- Track label offsets for branch targets
- Track pending fixups
- Track external relocations

**See**: Main plan for EmitState details (`17-backend3.md`)

### 2. Frame layout computation (ISA-agnostic)

**Components**:

- Compute frame size from regalloc spills
- Compute ABI requirements
- Determine clobbered callee-saved registers

**See**: Main plan for frame layout computation (`17-backend3.md`)

### 3. Prologue/epilogue generation (ISA-agnostic, uses ISA-specific ABI)

**Components**:

- Generate prologue: setup area → SP adjustment → callee-saved saves
- Generate epilogue: callee-saved restores → SP restore → return
- Uses ABI trait for frame layout details

**See**: Main plan for prologue/epilogue details (`17-backend3.md`)

### 4. Emission implementation (ISA-agnostic)

**File**: `backend3/emit.rs`

**Components**:

- Apply allocations and emit code
- Handle edits (moves, spills, reloads)
- Iterate blocks and instructions
- Record relocations
- Streaming label-based emission

**See**: Main plan for emission details (`17-backend3.md`)

### 5. Instruction conversion (RISC-V 32-specific)

**Components**:

- Convert VReg operands to physical registers in MachInst
- Handle stack slots (compute offsets)
- Implement MachInst methods for emission

### 6. InstBuffer enhancements

**Components**:

- `cur_offset()`: Get current code offset
- `emit_branch_with_label()`: Emit branch with label target
- `patch_branch()`: Patch branch instruction with computed offset

**See**: Main plan for InstBuffer enhancements (`17-backend3.md`)

### 7. End-to-end test

**Components**:

- Lower → Regalloc → Emit → Execute
- Verify correct machine code generation

## Testing

- Unit tests for emission state tracking
- Unit tests for frame layout computation
- Unit tests for prologue/epilogue generation
- Unit tests for edit emission (moves, spills, reloads)
- Integration test: End-to-end compilation of simple function

## Success Criteria

- ✅ Can emit prologue/epilogue
- ✅ Can emit instructions with allocated registers
- ✅ Can emit edits (moves, spills, reloads)
- ✅ Can compile simple function end-to-end
- ✅ Generated code executes correctly
