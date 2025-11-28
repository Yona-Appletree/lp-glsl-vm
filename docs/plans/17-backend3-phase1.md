# Backend3 Phase 1: Foundation

**Goal**: Basic structure and lowering

**Timeline**: Week 1

**Deliverable**: Can lower simple arithmetic functions to VCode

## Tasks

### 1. Create VCode structure (ISA-agnostic)

**Files**: 
- `backend3/vcode.rs`: Core VCode type (generic over MachInst)
- `backend3/vcode_builder.rs`: Builder for constructing VCode

**Components**:
- Basic block and instruction tracking
- Block metadata (cold, indirect targets)
- Relocation tracking
- Operand arrays for regalloc2
- Block structure (ranges, successors, predecessors, parameters)

**See**: Main plan for VCode structure details (`17-backend3.md`)

### 2. Block lowering order (ISA-agnostic)

**File**: `backend3/blockorder.rs`

**Components**:
- Critical edge detection
- Reverse postorder computation
- Basic implementation (no cold block optimization yet)

**See**: Main plan for BlockLoweringOrder details (`17-backend3.md`)

### 3. Create machine instruction type (RISC-V 32-specific)

**File**: `isa/riscv32/backend3/inst.rs`

**Components**:
- MachInst enum with VReg operands
- Implement basic instructions (add, addi, lw, sw)
- Implement MachInst trait for regalloc2 (operand visitor)

**See**: Main plan for MachInst details (`17-backend3.md`)

### 4. Basic lowering (ISA-agnostic)

**File**: `backend3/lower.rs`

**Components**:
- Lower simple instructions (iconst, iadd, isub)
- Use block lowering order
- Create virtual registers for values
- Build VCode structure
- Handle edge blocks (phi moves)
- Uses MachInst trait (implemented by RISC-V 32 MachInst)

**See**: Main plan for Lowering details (`17-backend3.md`)

### 5. Constant handling (ISA-agnostic)

**File**: `backend3/constants.rs`

**Components**:
- Constant materialization
- Inline constants (12-bit immediates)
- Large constants (lui + addi)

**See**: Main plan for Constant Handling details (`17-backend3.md`)

## Testing

- Unit tests for VCode structure
- Unit tests for block ordering
- Unit tests for lowering simple instructions
- Unit tests for constant materialization
- Integration test: Lower simple arithmetic function to VCode

## Success Criteria

- ✅ VCode structure can represent simple functions
- ✅ Block ordering computes correct order
- ✅ Can lower iconst, iadd, isub instructions
- ✅ Can materialize constants (inline and large)
- ✅ Can create VCode from simple IR function

