# Backend3 Phase 2: Regalloc2 Integration

**Goal**: Register allocation working

**Timeline**: Week 2

**Deliverable**: Can allocate registers for simple functions

## Cranelift References

**Primary Reference**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/machinst/`

- **VCode regalloc2 Integration**: `vcode.rs` - Implementation of `regalloc2::Function` trait for VCode
  - See `impl<...> Function for VCode<I>` around line 200-400
  - Operand collection and ranges handling
  - Block structure (succs, preds, params)
  - Branch arguments handling
- **ABI Machine Spec**: `abi.rs` - Generic ABI machine spec trait and implementation
- **Compilation Pipeline**: `compile.rs` - Regalloc2 integration in compilation pipeline (line 57-80)

**RISC-V Specific References**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/`

- **ABI Implementation**: `abi.rs` - RISC-V 64-bit ABI machine spec
  - `impl ABIMachineSpec for Riscv64MachineDeps` - Register classes, callee-saved/caller-saved
  - Frame layout computation
  - Multi-return handling

## Tasks

### 1. Implement regalloc2::Function trait (ISA-agnostic)

**File**: `backend3/regalloc.rs`

**Purpose**: Implement `regalloc2::Function` trait for VCode to enable register allocation.

**Complete Implementation**:

```rust
use regalloc2::{
    Function as RegallocFunction, 
    InstRange, 
    Operand, 
    OperandKind,
    PRegSet,
    RegClass,
    VReg,
};

/// Implement regalloc2::Function trait for VCode
impl<I: MachInst> RegallocFunction for VCode<I> {
    type Inst = InsnIndex;
    type Block = BlockIndex;
    type VReg = VReg;
    type PReg = RealReg;

    fn num_insts(&self) -> usize {
        self.insts.len()
    }

    fn num_blocks(&self) -> usize {
        self.block_ranges.len()
    }

    fn entry_block(&self) -> BlockIndex {
        self.entry
    }

    fn block_insns(&self, block: BlockIndex) -> InstRange {
        let range = self.block_ranges.get(block.index());
        InstRange::new(
            InsnIndex::new(range.start), 
            InsnIndex::new(range.end)
        )
    }

    fn block_succs(&self, block: BlockIndex) -> &[BlockIndex] {
        let range = self.block_succ_range.get(block.index());
        &self.block_succs[range]
    }

    fn block_preds(&self, block: BlockIndex) -> &[BlockIndex] {
        let range = self.block_pred_range.get(block.index());
        &self.block_preds[range]
    }

    fn block_params(&self, block: BlockIndex) -> &[VReg] {
        // Entry block params are handled by Args instruction, not block params
        if block == self.entry {
            return &[];
        }
        let range = self.block_params_range.get(block.index());
        &self.block_params[range]
    }

    fn branch_blockparams(
        &self, 
        block: BlockIndex, 
        _insn: InsnIndex, 
        succ_idx: usize
    ) -> &[VReg] {
        // Return the VRegs passed to a specific successor block
        let succ_range = self.branch_block_arg_succ_range.get(block.index());
        debug_assert!(succ_idx < succ_range.len());
        let branch_block_args = self.branch_block_arg_range.get(
            succ_range.start + succ_idx
        );
        &self.branch_block_args[branch_block_args]
    }

    fn is_ret(&self, insn: InsnIndex) -> bool {
        match self.insts[insn.index()].is_term() {
            MachTerminator::Ret | MachTerminator::RetCall => true,
            MachTerminator::Branch => false,
            MachTerminator::None => false, // Could be trap, but not ret
        }
    }

    fn is_branch(&self, insn: InsnIndex) -> bool {
        match self.insts[insn.index()].is_term() {
            MachTerminator::Branch => true,
            _ => false,
        }
    }

    fn inst_operands(&self, insn: InsnIndex) -> &[Operand] {
        let range = self.operand_ranges.get(insn.index());
        &self.operands[range]
    }

    fn inst_clobbers(&self, insn: InsnIndex) -> PRegSet {
        // Return explicitly clobbered registers for this instruction
        // (e.g., from function calls)
        self.clobbers.get(&insn).cloned().unwrap_or_default()
    }

    fn num_vregs(&self) -> usize {
        self.vreg_types.len()
    }

    fn debug_value_labels(&self) -> &[(VReg, InsnIndex, InsnIndex, u32)] {
        // For debug info (optional, can return empty slice)
        &[]
    }

    fn spillslot_size(&self, regclass: RegClass) -> usize {
        // RISC-V 32: all GPRs are 4 bytes
        self.abi.get_spillslot_size(regclass) as usize
    }

    fn allow_multiple_vreg_defs(&self) -> bool {
        // Allow multiple defs of the same VReg (needed for some backends)
        true
    }
}
```

**Operand Collection During Lowering**:

Operands are collected during lowering by calling `get_operands()` on each `MachInst`:

```rust
impl Lower {
    fn lower_inst(&mut self, inst: InstEntity) {
        let inst_data = self.func.dfg.inst_data(inst).unwrap();
        
        // Create machine instruction
        let mach_inst = match inst_data.opcode {
            Opcode::Iadd => {
                let rs1 = self.value_to_vreg[&inst_data.args[0]];
                let rs2 = self.value_to_vreg[&inst_data.args[1]];
                let rd = self.value_to_vreg[&inst_data.results[0]];
                MachInst::Add { rd, rs1, rs2 }
            }
            // ... other opcodes ...
        };
        
        // Collect operands for regalloc2
        let inst_idx = self.vcode.insts.len();
        mach_inst.get_operands(&mut |reg: &mut Reg, constraint, kind, _pos| {
            self.vcode.operands.push(Operand {
                vreg: reg.to_vreg(),
                constraint,
                kind, // Use, Def, or Modify
            });
        });
        
        // Record operand range for this instruction
        let operand_start = self.vcode.operands.len() - /* operand count */;
        let operand_end = self.vcode.operands.len();
        self.vcode.operand_ranges.set(inst_idx, operand_start..operand_end);
        
        // Push instruction
        self.vcode.insts.push(mach_inst);
    }
}
```

**ABI Machine Spec Configuration**:

The ABI machine spec is provided separately via `VCode::abi.machine_env()`:

```rust
impl VCode<MachInst> {
    fn run_regalloc(&self) -> regalloc2::Output {
        let machine_env = self.abi.machine_env();
        regalloc2::run(self, machine_env, &regalloc2::RegallocOptions::default())
            .expect("register allocation")
    }
}
```

### 2. ABI machine spec (RISC-V 32-specific)

**File**: `isa/riscv32/backend3/abi.rs`

**Components**:
- Riscv32ABI implementation
- Register classes
- Callee-saved vs caller-saved
- ABIMachineSpec trait implementation

**ABI Machine Spec Implementation**:

```rust
// File: crates/lpc-codegen/src/isa/riscv32/backend3/abi.rs

use regalloc2::{ABIMachineSpec, ...};
use super::inst::MachInst;

/// RISC-V 32-bit ABI machine specification for regalloc2
pub struct Riscv32ABI;

impl ABIMachineSpec for Riscv32ABI {
    type I = MachInst;

    fn callee_saved_gprs() -> &'static [RealReg] {
        // s0-s11 (x8-x9, x18-x27)
    }

    fn caller_saved_gprs() -> &'static [RealReg] {
        // a0-a7, t0-t6 (x5-x7, x10-x17, x28-x31)
    }

    fn fixed_stack_slots() -> &'static [StackSlot] {
        // None for now
    }

    // ... implement ABI methods ...
}
```

**Key Features**:
- Defines register classes
- Specifies callee-saved vs caller-saved
- Configures stack slots
- Handles multi-return (return area)
- Provides frame layout computation helpers
- Generates prologue/epilogue sequences

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

