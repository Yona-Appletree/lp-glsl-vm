# Backend3 Phase 2: Regalloc2 Integration

**Goal**: Register allocation working

**Timeline**: Week 2

**Deliverable**: Can allocate registers for simple functions

## Tasks

### 1. Implement regalloc2::Function trait (ISA-agnostic)

**File**: `backend3/regalloc.rs`

**Components**:
- Block structure methods
- Operand methods
- Provide VCode to regalloc2

**Key Methods**:
- `blocks()`: Return all blocks
- `block_insts()`: Return instruction indices for block
- `block_succs()`: Return successors
- `block_params()`: Return block parameter VRegs
- `inst_operands()`: Return operands for instruction

**See**: Main plan for regalloc2 integration details (`17-backend3.md`)

### 2. ABI machine spec (RISC-V 32-specific)

**File**: `isa/riscv32/backend3/abi.rs`

**Components**:
- Riscv32ABI implementation
- Register classes
- Callee-saved vs caller-saved
- ABIMachineSpec trait implementation

**See**: Main plan for ABI details (`17-backend3.md`)

### 3. Test regalloc2

**Components**:
- Run regalloc2 on simple VCode
- Verify allocations and edits
- Test with register pressure (force spilling)

## Testing

- Unit tests for regalloc2::Function trait implementation
- Unit tests for ABI machine spec
- Integration test: Run regalloc2 on simple VCode, verify allocations

## Success Criteria

- ✅ Can run regalloc2 on VCode
- ✅ Gets allocations for all VRegs
- ✅ Gets edits (moves, spills, reloads)
- ✅ Handles register pressure (spilling works)

