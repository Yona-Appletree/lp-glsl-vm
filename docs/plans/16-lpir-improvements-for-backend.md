# LPIR Improvements for Backend3

## Overview

This document outlines the improvements needed in LPIR to support the new Cranelift-style backend (`backend3`). The new backend follows Cranelift's architecture: **Lowering → VCode (virtual registers) → Register Allocation → Emission**.

## Current State

LPIR currently has:

- ✅ SSA form with values
- ✅ DFG (Data Flow Graph) with instruction data
- ✅ Layout separation (where instructions are)
- ✅ Block parameters (phi-like values)
- ✅ Multi-return signatures (supports multiple return types)
- ❌ Multi-return implementation (panics on >2 returns)
- ❌ Operand classification (use/def/modify)
- ❌ Value-to-instruction mapping helpers
- ❌ Instruction operand constraints metadata

## Required Improvements

### 1. Multi-Return Support

**Status**: Partially implemented (signature supports it, backend panics)

**Current Issues**:

- `Return` instruction supports multiple values in `args`
- Backend panics at `return_.rs:21` and `call.rs:111` when handling >2 returns
- No return area mechanism support

**Required Changes**:

#### 1.1 Return Instruction Validation

**File**: `crates/lpc-lpir/src/verifier/format.rs`

Add validation that return instruction values match function signature:

```rust
// Verify return instruction matches function signature
if let Opcode::Return = inst_data.opcode {
    let return_count = inst_data.args.len();
    let expected_count = func.signature.returns.len();
    if return_count != expected_count {
        return Err(VerifierError::ReturnCountMismatch {
            expected: expected_count,
            actual: return_count,
        });
    }
    // Verify types match
    for (i, ret_value) in inst_data.args.iter().enumerate() {
        let ret_ty = func.dfg.value_type(*ret_value);
        let expected_ty = func.signature.returns.get(i);
        // ... type checking ...
    }
}
```

#### 1.2 Call Instruction Multi-Return Support

**File**: `crates/lpc-lpir/src/verifier/format.rs`

Add validation that call instruction results match callee signature:

```rust
// Verify call instruction results match callee signature
if let Opcode::Call { callee } = &inst_data.opcode {
    // Need access to module to get callee signature
    // This may require passing Module to verifier
    let callee_func = module.get_function(callee)?;
    let result_count = inst_data.results.len();
    let expected_count = callee_func.signature.returns.len();
    if result_count != expected_count {
        return Err(VerifierError::CallResultCountMismatch {
            callee: callee.clone(),
            expected: expected_count,
            actual: result_count,
        });
    }
}
```

**Note**: This requires passing `Module` to the verifier, or verifying at module level.

#### 1.3 Helper Methods for Multi-Return

**File**: `crates/lpc-lpir/src/dfg/mod.rs` or `crates/lpc-lpir/src/function.rs`

Add convenience methods:

```rust
impl Function {
    /// Get the number of return values expected by this function
    pub fn return_count(&self) -> usize {
        self.signature.returns.len()
    }

    /// Check if this function uses multi-return (more than 2 returns)
    pub fn uses_multi_return(&self) -> bool {
        self.signature.returns.len() > 2
    }

    /// Get return value types
    pub fn return_types(&self) -> &[Type] {
        &self.signature.returns
    }
}
```

### 2. Operand Classification

**Status**: Not implemented

**Purpose**: Register allocators (like regalloc2) need to know:

- **Use**: Read-only operand (input)
- **Def**: Write-only operand (output)
- **Modify**: Read-write operand (input and output)

**Current State**: LPIR only distinguishes `args` (inputs) and `results` (outputs).

**Required Changes**:

#### 2.1 Add Operand Kind Enum

**File**: `crates/lpc-lpir/src/dfg/operand.rs` (new file)

```rust
/// Operand kind for register allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandKind {
    /// Use: read-only operand (input)
    Use,
    /// Def: write-only operand (output)
    Def,
    /// Modify: read-write operand (input and output)
    Modify,
}
```

#### 2.2 Add Operand Metadata to InstData

**File**: `crates/lpc-lpir/src/dfg/inst_data.rs`

Add optional operand kind information:

```rust
pub struct InstData {
    // ... existing fields ...

    /// Optional operand kinds (for register allocation)
    /// If None, defaults to: args = Use, results = Def
    /// If Some, provides explicit operand classification
    pub operand_kinds: Option<Vec<OperandKind>>,
}
```

**Alternative Approach**: Don't modify `InstData`, instead add a helper trait:

```rust
/// Trait for instructions that can classify operands
pub trait OperandClassifier {
    /// Classify operands for register allocation
    /// Returns (use_operands, def_operands, modify_operands)
    fn classify_operands(&self) -> (Vec<usize>, Vec<usize>, Vec<usize>);
}
```

**Recommendation**: Start with helper methods, add metadata later if needed.

#### 2.3 Helper Methods for Operand Classification

**File**: `crates/lpc-lpir/src/dfg/mod.rs`

Add methods to classify operands:

```rust
impl DFG {
    /// Get operand kinds for an instruction
    /// Returns a vector of (value_index, kind) tuples
    pub fn operand_kinds(&self, inst: InstEntity) -> Vec<(Value, OperandKind)> {
        let inst_data = self.inst_data(inst)?;
        let mut result = Vec::new();

        // All args are uses
        for arg in &inst_data.args {
            result.push((*arg, OperandKind::Use));
        }

        // All results are defs
        for result_value in &inst_data.results {
            result.push((*result_value, OperandKind::Def));
        }

        result
    }

    /// Check if an instruction modifies a register (read-write)
    /// For now, no instructions modify registers (all are use or def)
    pub fn has_modify_operands(&self, inst: InstEntity) -> bool {
        // Future: check for instructions like add-with-update, etc.
        false
    }
}
```

### 3. Value-to-Instruction Mapping

**Status**: Partially implemented

**Purpose**: Backend needs to find:

- Which instruction defines a value
- Which instructions use a value
- Block parameter sources (phi-like)

**Current State**:

- `DFG::inst_results()` gives results for an instruction
- `DFG::inst_args()` gives args for an instruction
- No reverse mapping (value → defining instruction)

**Required Changes**:

#### 3.1 Add Value Definition Tracking

**File**: `crates/lpc-lpir/src/function.rs` or `crates/lpc-lpir/src/dfg/mod.rs`

Add helper methods:

```rust
impl Function {
    /// Find the instruction that defines a value
    /// Returns None for block parameters and function parameters
    pub fn value_def(&self, value: Value) -> Option<InstEntity> {
        // Iterate through all instructions
        for block in self.blocks() {
            for inst in self.block_insts(block) {
                if let Some(inst_data) = self.dfg.inst_data(inst) {
                    if inst_data.results.contains(&value) {
                        return Some(inst);
                    }
                }
            }
        }
        None
    }

    /// Find all instructions that use a value
    pub fn value_uses(&self, value: Value) -> Vec<InstEntity> {
        let mut uses = Vec::new();
        for block in self.blocks() {
            for inst in self.block_insts(block) {
                if let Some(inst_data) = self.dfg.inst_data(inst) {
                    if inst_data.args.contains(&value) {
                        uses.push(inst);
                    }
                    // Also check block_args
                    if let Some(block_args) = &inst_data.block_args {
                        for (_, args) in &block_args.targets {
                            if args.contains(&value) {
                                uses.push(inst);
                            }
                        }
                    }
                }
            }
        }
        uses
    }

    /// Check if a value is a block parameter
    pub fn is_block_param(&self, value: Value) -> Option<(Block, usize)> {
        for (block_idx, block) in self.blocks.iter().enumerate() {
            if let Some(block_data) = self.block_data(block) {
                for (param_idx, param_value) in block_data.params.iter().enumerate() {
                    if *param_value == value {
                        return Some((block, param_idx));
                    }
                }
            }
        }
        None
    }

    /// Check if a value is a function parameter
    pub fn is_function_param(&self, value: Value) -> Option<usize> {
        // Function parameters are block 0 parameters
        if let Some((block, param_idx)) = self.is_block_param(value) {
            if block == self.entry_block().unwrap() {
                return Some(param_idx);
            }
        }
        None
    }
}
```

**Performance Note**: These methods do linear scans. For large functions, consider building reverse maps, but start simple.

### 4. Block Parameter Source Tracking

**Status**: Partially implemented

**Purpose**: Backend needs to know which values flow into block parameters (phi sources).

**Current State**:

- Block parameters exist
- Jump/Branch instructions pass values to blocks
- No helper to find all sources of a block parameter

**Required Changes**:

#### 4.1 Add Block Parameter Source Tracking

**File**: `crates/lpc-lpir/src/function.rs`

Add method to find block parameter sources:

```rust
impl Function {
    /// Find all sources (predecessor blocks + values) for a block parameter
    /// Returns Vec<(predecessor_block, value_passed)>
    pub fn block_param_sources(&self, block: Block, param_idx: usize) -> Vec<(Block, Value)> {
        let mut sources = Vec::new();

        // Get the parameter value
        let block_data = self.block_data(block)?;
        let param_value = block_data.params.get(param_idx)?;

        // Find all predecessors
        let cfg = ControlFlowGraph::new(self);
        for pred_block in cfg.predecessors(block) {
            // Find the instruction that branches to this block
            for inst in self.block_insts(pred_block) {
                if let Some(inst_data) = self.dfg.inst_data(inst) {
                    if let Some(block_args) = &inst_data.block_args {
                        for (target, args) in &block_args.targets {
                            if *target == block {
                                // Found a branch/jump to this block
                                if let Some(arg_value) = args.get(param_idx) {
                                    sources.push((pred_block, *arg_value));
                                }
                            }
                        }
                    }
                }
            }
        }

        sources
    }
}
```

### 5. Instruction Operand Constraints

**Status**: Not implemented

**Purpose**: Some instructions have constraints (e.g., call args must be in specific registers).

**Current State**: No constraint metadata.

**Required Changes**:

#### 5.1 Add Operand Constraint Types

**File**: `crates/lpc-lpir/src/dfg/operand.rs` (extend)

```rust
/// Operand constraint for register allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandConstraint {
    /// No constraint (any register)
    Any,
    /// Must be in a specific register class
    RegClass(RegClass),
    /// Must be in a specific register (for ABI)
    FixedReg(Reg),
    /// Must be on the stack (for large values)
    Stack,
}
```

**Note**: This may be backend-specific. Consider keeping it in backend3, not LPIR.

**Recommendation**: Start without this, add later if needed for ABI constraints.

### 6. Instruction Metadata for Lowering

**Status**: Partially implemented

**Purpose**: Backend needs metadata about instructions for lowering:

- Is this a call? (affects register allocation)
- Is this a terminator? (affects block structure)
- Does this access memory? (affects scheduling)

**Current State**: Opcode enum has this information, but no helper methods.

**Required Changes**:

#### 6.1 Add Instruction Query Methods

**File**: `crates/lpc-lpir/src/dfg/opcode.rs`

Add helper methods:

```rust
impl Opcode {
    /// Is this a call instruction?
    pub fn is_call(&self) -> bool {
        matches!(self, Opcode::Call { .. })
    }

    /// Is this a terminator (branch, jump, return)?
    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            Opcode::Jump | Opcode::Br | Opcode::Return | Opcode::Halt | Opcode::Trap { .. }
        )
    }

    /// Does this instruction access memory?
    pub fn is_memory_access(&self) -> bool {
        matches!(self, Opcode::Load | Opcode::Store)
    }

    /// Does this instruction have side effects?
    pub fn has_side_effects(&self) -> bool {
        matches!(
            self,
            Opcode::Store
                | Opcode::Call { .. }
                | Opcode::Syscall
                | Opcode::Return
                | Opcode::Trap { .. }
                | Opcode::Trapz { .. }
                | Opcode::Trapnz { .. }
        )
    }
}
```

### 7. Type Information Improvements

**Status**: Implemented, but may need enhancements

**Purpose**: Backend needs type information for:

- Register class selection
- Spill slot sizing
- ABI argument/return handling

**Current State**:

- `DFG::value_type()` exists
- Types are tracked per value

**Required Changes**: None immediately, but verify:

- All values have types (especially block parameters)
- Function parameters have types
- Return values have types

### 8. Module-Level Verification

**Status**: Partially implemented

**Purpose**: Verify call/return compatibility across functions.

**Current State**: Verification is per-function.

**Required Changes**:

#### 8.1 Add Module-Level Verification

**File**: `crates/lpc-lpir/src/verifier/mod.rs`

Add module verification:

```rust
/// Verify a module (cross-function checks)
pub fn verify_module(module: &Module) -> Result<(), VerifierError> {
    // Verify each function
    for (name, func) in &module.functions {
        verify(func)?;
    }

    // Verify call instructions match callee signatures
    for (name, func) in &module.functions {
        for block in func.blocks() {
            for inst in func.block_insts(block) {
                if let Some(inst_data) = func.dfg.inst_data(inst) {
                    if let Opcode::Call { callee } = &inst_data.opcode {
                        let callee_func = module
                            .get_function(callee)
                            .ok_or_else(|| VerifierError::UnknownFunction(callee.clone()))?;

                        // Verify argument count
                        if inst_data.args.len() != callee_func.signature.params.len() {
                            return Err(VerifierError::CallArgCountMismatch {
                                callee: callee.clone(),
                                expected: callee_func.signature.params.len(),
                                actual: inst_data.args.len(),
                            });
                        }

                        // Verify result count
                        if inst_data.results.len() != callee_func.signature.returns.len() {
                            return Err(VerifierError::CallResultCountMismatch {
                                callee: callee.clone(),
                                expected: callee_func.signature.returns.len(),
                                actual: inst_data.results.len(),
                            });
                        }

                        // Verify types (optional, can be strict or lenient)
                        // ...
                    }
                }
            }
        }
    }

    Ok(())
}
```

## Implementation Priority

### Phase 1: Critical (Required for Backend3)

1. **Multi-return support** (#1)

   - Remove panics in backend
   - Add validation
   - Add helper methods

2. **Value-to-instruction mapping** (#3)

   - `value_def()`
   - `value_uses()`
   - `is_block_param()`

3. **Block parameter source tracking** (#4)
   - `block_param_sources()`

### Phase 2: Important (Improves Backend3 Quality)

4. **Operand classification** (#2)

   - Add `OperandKind` enum
   - Add helper methods

5. **Instruction query methods** (#6)

   - `is_call()`, `is_terminator()`, etc.

6. **Module-level verification** (#8)
   - Cross-function checks

### Phase 3: Optional (Future Enhancements)

7. **Operand constraints** (#5)

   - May be backend-specific
   - Add if needed for ABI

8. **Type information improvements** (#7)
   - Verify completeness
   - Add helpers if needed

## Testing Requirements

For each improvement, add tests:

1. **Multi-return**:

   - Test function with 3+ returns
   - Test call with 3+ results
   - Test verification errors

2. **Value mapping**:

   - Test `value_def()` for various instructions
   - Test `value_uses()` with multiple uses
   - Test block parameter detection

3. **Block parameter sources**:

   - Test with multiple predecessors
   - Test with jump and branch

4. **Operand classification**:
   - Test use/def classification
   - Test modify (when added)

## Migration Notes

- These changes are **additive** - existing code continues to work
- New methods are helpers, don't change existing APIs
- Verification changes may catch existing bugs (good!)

## Related Documents

- `docs/plans/14-lpir-additional-instructions.md` - Instruction additions
- `docs/plans/10.5-call-handling.md` - Call/return handling
- `docs/riscv32-abi.md` - ABI requirements
