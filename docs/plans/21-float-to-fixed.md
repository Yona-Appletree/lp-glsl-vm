# Float to Fixed16x16 Translation at LPIR Level

## Overview

This plan implements floating point emulation for GLSL on RISC-V32 IMAC (no floating point extension) by translating float operations to 16.16 fixed-point arithmetic at the LPIR level. The GLSL frontend will generate standard float operations in LPIR, and a transformation pass will convert them to fixed-point operations before backend lowering.

**Target**: RISC-V32 IMAC (no floating point extension)  
**Fixed Point Format**: 16.16 (16 integer bits, 16 fractional bits)  
**Translation Level**: LPIR (after GLSL frontend, before backend3 lowering)

## Goals

1. **Enable GLSL float support**: Allow GLSL frontend to generate float types and operations
2. **Centralized translation**: Convert all float operations to fixed16x16 at LPIR level
3. **Transparent to backend**: Backend3 treats fixed-point values as I32
4. **Maintain correctness**: Preserve SSA form, dominance, and IR invariants

## Critical Missing Operations

**IMPORTANT**: This plan requires adding **MULH** (multiply high) to LPIR and backend3 for efficient fixed-point multiply.

- **MULH**: ❌ Not in LPIR, ❌ Not in backend3
  - **Required for**: Efficient fixed-point multiply `(a * b) >> 16`
  - **RISC-V**: `MULH rd, rs1, rs2` (M extension)
  - **Priority**: **HIGH** - Without this, fixed-point multiply requires library calls or 20+ instructions
  - **See**: Phase 4 for implementation details

**Optional operations** (can add later if needed):
- Shift immediate operations (SLLI/SRLI/SRAI) - LPIR has register shifts, backend has immediates
- MULHU (unsigned multiply high) - For unsigned fixed-point operations

## Current State

### LPIR Float Support

LPIR currently supports:
- ✅ `F32` type in type system
- ✅ `Fconst` opcode (floating point constants)
- ✅ `Fcmp` opcode (floating point comparisons with `FloatCC`)
- ❌ `Fadd`, `Fsub`, `Fmul`, `Fdiv` opcodes (not yet implemented)

### GLSL Frontend

GLSL frontend currently:
- ❌ Rejects float types (`Expr::FloatConst` returns error)
- ❌ No float type in `GlslType` enum
- ✅ Can be extended to support float types

### Backend3

Backend3 currently:
- ✅ Supports I32 operations (add, sub, mul, div, comparisons)
- ✅ No floating point lowering (as expected for RISC-V32 IMAC)
- ✅ Will treat fixed-point values as I32 after translation

## Architecture

### Compilation Pipeline

**Current Pipeline:**
```
GLSL Frontend → LPIR (i32/u32 only) → Backend3 Lowering → VCode → Register Allocation → Emission
```

**Proposed Pipeline:**
```
GLSL Frontend → LPIR (with F32) → [Float→Fixed Pass] → LPIR (I32 fixed-point) → Backend3 Lowering → ...
```

### Design Decisions

#### 1. Type Representation

**Decision**: Represent fixed16x16 as `I32` type in LPIR after translation.

**Rationale**:
- Minimal changes to existing infrastructure
- Backend3 already handles I32 operations efficiently
- Fixed-point semantics are implicit (documented, not encoded in types)
- Can add explicit `Fixed16x16` type later if needed for better type safety

**Note**: We could add explicit `Fixed16x16` type to LPIR later for better type safety, but starting with I32 representation.

#### 2. Translation Pass Location

**Decision**: Create new LPIR transformation module (`crates/lpc-lpir/src/transform/`)

**Rationale**:
- Keeps GLSL frontend simple (generates standard float operations)
- Centralized translation point
- Leverages LPIR's SSA, dominance, and verification infrastructure
- Can be applied to any LPIR function (not just GLSL-generated)

#### 3. Fixed-Point Format

**Format**: 16.16 signed fixed-point
- **Range**: -32768.0 to +32767.9999847412109375
- **Precision**: 1/65536 (approximately 0.00001526)
- **Representation**: `fixed_value = float_value * 65536` (rounded to nearest integer)

**Limitations**:
- No NaN, no infinity (overflow/underflow handled via clamping or wrapping)
- No signed zero distinction
- Limited range compared to IEEE 754 float32

## Fixed-Point Arithmetic

### Conversion Formulas

**Float to Fixed16x16:**
```rust
fn float_to_fixed16x16(f: f32) -> i32 {
    // Clamp to representable range
    let clamped = f.clamp(-32768.0, 32767.9999847412109375);
    // Convert to fixed-point (round to nearest)
    (clamped * 65536.0).round() as i32
}
```

**Fixed16x16 to Float (for constants/debugging):**
```rust
fn fixed16x16_to_float(fixed: i32) -> f32 {
    fixed as f32 / 65536.0
}
```

### Arithmetic Operations

**Addition/Subtraction:**
```rust
// Fixed-point addition/subtraction is direct integer addition/subtraction
fixed_result = fixed_a + fixed_b  // for addition
fixed_result = fixed_a - fixed_b    // for subtraction
```

**Multiplication:**
```rust
// Fixed-point multiplication requires shift to maintain precision
// (a * b) >> 16, but need to handle overflow
// Use 64-bit intermediate to avoid overflow
let temp: i64 = (fixed_a as i64) * (fixed_b as i64);
fixed_result = (temp >> 16) as i32;
```

**Division:**
```rust
// Fixed-point division requires shift before division
// (a << 16) / b, but need to handle overflow
// Use 64-bit intermediate
let temp: i64 = ((fixed_a as i64) << 16) / (fixed_b as i64);
fixed_result = temp as i32;
```

**Comparison:**
```rust
// Fixed-point comparisons are direct integer comparisons
// (except for equality - may need epsilon for floating-point-like behavior)
// For now, use direct integer comparison
result = (fixed_a < fixed_b) ? 1 : 0  // for less-than
```

## Implementation Plan

### Phase 1: Add Missing Float Operations to LPIR

**Goal**: Add `Fadd`, `Fsub`, `Fmul`, `Fdiv` opcodes to LPIR so GLSL frontend can generate them.

#### 1.1 Add Float Arithmetic Opcodes

**File**: `crates/lpc-lpir/src/dfg/opcode.rs`

Add new opcodes:
```rust
pub enum Opcode {
    // ... existing opcodes ...
    
    // Floating point arithmetic
    /// Floating point add: result = arg1 + arg2
    Fadd,
    /// Floating point subtract: result = arg1 - arg2
    Fsub,
    /// Floating point multiply: result = arg1 * arg2
    Fmul,
    /// Floating point divide: result = arg1 / arg2
    Fdiv,
}
```

#### 1.2 Update DFG Type Inference

**File**: `crates/lpc-lpir/src/dfg/mod.rs`

Add type inference for new opcodes:
```rust
impl DFG {
    pub fn infer_result_type(&self, opcode: &Opcode) -> Option<Type> {
        match opcode {
            // ... existing cases ...
            Opcode::Fadd | Opcode::Fsub | Opcode::Fmul | Opcode::Fdiv => Some(Type::F32),
        }
    }
}
```

#### 1.3 Add Builder Methods

**File**: `crates/lpc-lpir/src/builder/block_builder.rs`

Add builder methods:
```rust
impl BlockBuilder {
    pub fn fadd(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.inst(InstData::binary(Opcode::Fadd, result, arg1, arg2));
    }
    
    pub fn fsub(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.inst(InstData::binary(Opcode::Fsub, result, arg1, arg2));
    }
    
    pub fn fmul(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.inst(InstData::binary(Opcode::Fmul, result, arg1, arg2));
    }
    
    pub fn fdiv(&mut self, result: Value, arg1: Value, arg2: Value) {
        self.inst(InstData::binary(Opcode::Fdiv, result, arg1, arg2));
    }
}
```

#### 1.4 Add Parser Support

**File**: `crates/lpc-lpir/src/parser/instructions.rs`

Add parsing for float arithmetic:
```rust
pub(crate) fn parse_fadd(input: &str) -> IResult<&str, InstData> {
    // Parse: "v0 = fadd v1, v2"
}

pub(crate) fn parse_fsub(input: &str) -> IResult<&str, InstData> {
    // Parse: "v0 = fsub v1, v2"
}

pub(crate) fn parse_fmul(input: &str) -> IResult<&str, InstData> {
    // Parse: "v0 = fmul v1, v2"
}

pub(crate) fn parse_fdiv(input: &str) -> IResult<&str, InstData> {
    // Parse: "v0 = fdiv v1, v2"
}
```

#### 1.5 Add Verifier Support

**File**: `crates/lpc-lpir/src/verifier/format.rs`

Add validation for float arithmetic:
- Verify 2 arguments, 1 result
- Verify arguments are F32 type
- Verify result is F32 type

**File**: `crates/lpc-lpir/src/verifier/types.rs`

Add type checking for float arithmetic operations.

#### 1.6 Add Tests

**File**: `crates/lpc-lpir/tests/float_arithmetic_tests.rs`

Add tests for:
- Parsing float arithmetic operations
- Type checking float arithmetic
- Builder API for float arithmetic

**Estimated Effort**: 1-2 days

### Phase 2: Create Float-to-Fixed Transformation Module

**Goal**: Create transformation infrastructure for converting float operations to fixed-point.

#### 2.1 Create Transform Module Structure

**File**: `crates/lpc-lpir/src/transform/mod.rs`

```rust
//! LPIR transformations and optimization passes.

pub mod fixed_point;

pub use fixed_point::convert_floats_to_fixed16x16;
```

**File**: `crates/lpc-lpir/src/lib.rs`

Add transform module:
```rust
mod transform;
pub use transform::convert_floats_to_fixed16x16;
```

#### 2.2 Create Fixed-Point Utilities

**File**: `crates/lpc-lpir/src/transform/fixed_point.rs`

Create helper functions for fixed-point conversion:
```rust
/// Convert a float32 value to fixed16x16 representation.
pub fn float_to_fixed16x16(f: f32) -> i32 {
    // Clamp to representable range
    let clamped = f.clamp(-32768.0, 32767.9999847412109375);
    // Convert to fixed-point (round to nearest)
    (clamped * 65536.0).round() as i32
}

/// Convert fixed16x16 back to float32 (for debugging/constants).
pub fn fixed16x16_to_float(fixed: i32) -> f32 {
    fixed as f32 / 65536.0
}
```

**Estimated Effort**: 0.5 days

### Phase 3: Implement Float-to-Fixed Transformation

**Goal**: Implement the main transformation pass that converts float operations to fixed-point.

#### 3.1 Function Signature Conversion

**File**: `crates/lpc-lpir/src/transform/fixed_point.rs`

```rust
/// Convert function signature: F32 params/returns → I32
fn convert_signature(sig: &mut Signature) {
    for param_ty in &mut sig.params {
        if *param_ty == Type::F32 {
            *param_ty = Type::I32;
        }
    }
    for ret_ty in &mut sig.returns {
        if *ret_ty == Type::F32 {
            *ret_ty = Type::I32;
        }
    }
}
```

#### 3.2 Value Type Conversion

Track which values are converted from F32 to I32:
```rust
struct ConversionContext {
    /// Map from original Value to converted Value (if different)
    /// For most values, we update in-place, but may need new values for constants
    converted_values: BTreeMap<Value, Value>,
    
    /// Builder for creating new instructions
    builder: FunctionBuilder,
}
```

#### 3.3 Instruction Conversion

Convert each float operation:

**Fconst → Iconst:**
```rust
fn convert_fconst(ctx: &mut ConversionContext, inst: InstEntity, func: &Function) {
    // Get original f32 constant value
    let f32_value = extract_f32_constant(func, inst);
    
    // Convert to fixed-point
    let fixed_value = float_to_fixed16x16(f32_value);
    
    // Replace instruction with iconst
    // Update value type from F32 to I32
}
```

**Fadd/Fsub → Iadd/Isub:**
```rust
fn convert_fadd(ctx: &mut ConversionContext, inst: InstEntity, func: &Function) {
    // Fixed-point addition is direct integer addition
    // Replace Fadd with Iadd
    // Ensure operands are I32 (should already be converted)
}
```

**Fmul → Fixed-point multiplication:**
```rust
fn convert_fmul(ctx: &mut ConversionContext, inst: InstEntity, func: &Function) {
    // Implement: (a * b) >> 16 using MULH
    // See Phase 5 for full implementation details
}
```

**Fdiv → Fixed-point division:**
```rust
fn convert_fdiv(ctx: &mut ConversionContext, inst: InstEntity, func: &Function) {
    // Implement: (a << 16) / b
    // Use library function call for now (see Phase 5)
}
```

**Fcmp → Icmp:**
```rust
fn convert_fcmp(ctx: &mut ConversionContext, inst: InstEntity, func: &Function, cond: FloatCC) {
    // Convert FloatCC to IntCC
    // Most comparisons map directly:
    //   FloatCC::Equal → IntCC::Equal
    //   FloatCC::LessThan → IntCC::SignedLessThan
    //   FloatCC::GreaterThan → IntCC::SignedGreaterThan
    //   etc.
    // 
    // Special cases:
    //   FloatCC::Unordered → always false (no NaN in fixed-point)
    //   FloatCC::Ordered → always true
    //   FloatCC::UnorderedOrEqual → IntCC::Equal (no NaN)
    //   etc.
}
```

**Load/Store with F32 type:**
```rust
fn convert_load(ctx: &mut ConversionContext, inst: InstEntity, func: &Function) {
    // Change Load type from F32 to I32
    // Update value type
}

fn convert_store(ctx: &mut ConversionContext, inst: InstEntity, func: &Function) {
    // Change Store type from F32 to I32
}
```

#### 3.4 Main Transformation Function

```rust
/// Convert all float operations in a function to fixed16x16.
///
/// This pass:
/// 1. Converts function signature (F32 → I32)
/// 2. Converts all F32 values to I32 (fixed-point representation)
/// 3. Converts all float operations to fixed-point operations
/// 4. Updates all value types
/// 5. Verifies the function is still valid
pub fn convert_floats_to_fixed16x16(func: &mut Function) -> Result<(), TransformError> {
    // 1. Convert signature
    convert_signature(&mut func.signature);
    
    // 2. Create conversion context
    let mut ctx = ConversionContext::new(func);
    
    // 3. Walk all blocks and instructions
    for block in func.layout.blocks() {
        // Collect instructions first (to avoid borrow issues)
        let insts: Vec<InstEntity> = func.block_insts(block).collect();
        
        for inst in insts {
            let inst_data = func.dfg.inst_data(inst).clone();
            
            match inst_data.opcode {
                Opcode::Fconst => convert_fconst(&mut ctx, inst, func)?,
                Opcode::Fadd => convert_fadd(&mut ctx, inst, func)?,
                Opcode::Fsub => convert_fsub(&mut ctx, inst, func)?,
                Opcode::Fmul => convert_fmul(&mut ctx, inst, func)?,
                Opcode::Fdiv => convert_fdiv(&mut ctx, inst, func)?,
                Opcode::Fcmp { cond } => convert_fcmp(&mut ctx, inst, func, cond)?,
                Opcode::Load if func.dfg.value_type(inst_data.args[0]) == Some(Type::F32) => {
                    convert_load(&mut ctx, inst, func)?;
                }
                Opcode::Store if func.dfg.value_type(inst_data.args[1]) == Some(Type::F32) => {
                    convert_store(&mut ctx, inst, func)?;
                }
                _ => {
                    // Update operand types if they reference F32 values
                    update_operand_types(&mut ctx, inst, func)?;
                }
            }
        }
    }
    
    // 4. Update all value types from F32 to I32
    update_all_value_types(func);
    
    // 5. Verify function is still valid
    verify(func)?;
    
    Ok(())
}
```

**Estimated Effort**: 3-4 days

### Phase 4: Add Missing RISC-V32 Operations to LPIR

**Goal**: Add RISC-V32 M extension operations needed for efficient fixed-point arithmetic.

#### 4.1 Missing Operations Analysis

For efficient fixed-point arithmetic, we need operations that RISC-V32 supports but LPIR doesn't currently have:

**Critical Missing Operations:**

1. **MULH** (Multiply High, signed × signed)
   - **RISC-V Instruction**: `MULH rd, rs1, rs2` (M extension)
   - **Purpose**: Get high 32 bits of 64-bit product
   - **Usage**: Fixed-point multiply `(a * b) >> 16` requires high bits
   - **Status**: ❌ Not in LPIR, ❌ Not in backend

2. **MULHU** (Multiply High, unsigned × unsigned) - Optional
   - **RISC-V Instruction**: `MULHU rd, rs1, rs2` (M extension)
   - **Purpose**: Get high 32 bits for unsigned multiply
   - **Usage**: May be needed for unsigned fixed-point operations
   - **Status**: ❌ Not in LPIR, ❌ Not in backend

3. **Shift Immediate Operations** - Optional but helpful
   - **RISC-V Instructions**: `SLLI`, `SRLI`, `SRAI` (shift by immediate)
   - **Purpose**: Shift by constant amount (e.g., 16 for fixed-point)
   - **Usage**: More efficient than shift by register for constant shifts
   - **Status**: ❌ Not in LPIR (has Ishl/Ishr/Iashr with register shift), ✅ Backend has SLLI/SRLI/SRAI

**Why MULH is Critical:**

For fixed-point multiply `result = (a * b) >> 16`:
- `MUL` gives us the low 32 bits: `lo = a * b` (low 32 bits)
- `MULH` gives us the high 32 bits: `hi = (a * b) >> 32` (high 32 bits)
- We need: `result = (hi << 16) | (lo >> 16)` or just `hi` if we shift the product correctly

#### 4.2 Add MULH to LPIR

**File**: `crates/lpc-lpir/src/dfg/opcode.rs`

Add new opcode:
```rust
pub enum Opcode {
    // ... existing opcodes ...
    
    /// Integer multiply high (signed): result = high 32 bits of (arg1 * arg2)
    /// This is used for extended precision arithmetic and fixed-point operations.
    /// Maps to RISC-V MULH instruction (M extension).
    Imulh,
}
```

**File**: `crates/lpc-lpir/src/dfg/mod.rs`

Add type inference:
```rust
Opcode::Imulh => Some(Type::I32), // Returns high 32 bits as I32
```

**File**: `crates/lpc-lpir/src/builder/block_builder.rs`

Add builder method:
```rust
pub fn imulh(&mut self, result: Value, arg1: Value, arg2: Value) {
    self.inst(InstData::binary(Opcode::Imulh, result, arg1, arg2));
}
```

**File**: `crates/lpc-lpir/src/parser/instructions.rs`

Add parser:
```rust
pub(crate) fn parse_imulh(input: &str) -> IResult<&str, InstData> {
    // Parse: "v0 = imulh v1, v2"
    let (input, _) = terminated(tag("imulh"), blank)(input)?;
    let (input, result) = parse_value(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = blank(input)?;
    let (input, arg1) = parse_value(input)?;
    let (input, _) = char(',')(input)?;
    let (input, _) = blank(input)?;
    let (input, arg2) = parse_value(input)?;
    Ok((input, InstData::binary(Opcode::Imulh, result, arg1, arg2)))
}
```

**File**: `crates/lpc-lpir/src/verifier/format.rs`

Add validation:
- Verify 2 arguments, 1 result
- Verify all are I32 type
- Verify no immediate

#### 4.3 Add MULH to Backend3

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/inst.rs`

Add machine instruction:
```rust
pub enum Riscv32MachInst {
    // ... existing instructions ...
    
    /// MULH: rd = high 32 bits of (rs1 * rs2) (signed, RISC-V M extension)
    Mulh {
        rd: Writable<Reg>,
        rs1: Reg,
        rs2: Reg,
    },
}
```

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/lower.rs`

Add lowering:
```rust
Opcode::Imulh => {
    let args = inst_data.args;
    let result = inst_data.results[0];
    let arg1_vreg = self.value_to_vreg[&args[0]];
    let arg2_vreg = self.value_to_vreg[&args[1]];
    let result_vreg = self.value_to_vreg[&result];
    
    let mulh_inst = Riscv32MachInst::Mulh {
        rd: Writable::new(Reg::from_virtual_reg(result_vreg)),
        rs1: Reg::from_virtual_reg(arg1_vreg),
        rs2: Reg::from_virtual_reg(arg2_vreg),
    };
    
    self.vcode.push(mulh_inst, rel_srcloc);
}
```

**File**: `crates/lpc-codegen/src/isa/riscv32/encode.rs`

Add encoding:
```rust
/// MULH: rd = high 32 bits of (rs1 * rs2) (signed, M extension)
pub fn mulh(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x1, 0x01)
}
```

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/emit.rs`

Add emission:
```rust
Riscv32MachInst::Mulh { rd, rs1, rs2 } => {
    let encoded = encode::mulh(rd.to_gpr(), rs1.to_gpr(), rs2.to_gpr());
    self.buffer.push_u32(encoded);
}
```

**File**: `crates/lpc-codegen/src/isa/riscv32/backend3/vcode_format.rs`

Add formatting:
```rust
Riscv32MachInst::Mulh { rd, rs1, rs2 } => {
    write!(f, "mulh {}, {}, {}", rd, rs1, rs2)
}
```

**Estimated Effort**: 1-2 days

### Phase 5: Handle 64-bit Intermediate Values

**Goal**: Implement proper 64-bit arithmetic for fixed-point multiply/divide using MULH.

#### 5.1 Implement Fixed-Point Multiply with MULH

Now that we have MULH, fixed-point multiply becomes straightforward:

```rust
fn convert_fmul(ctx: &mut ConversionContext, inst: InstEntity, func: &Function) {
    // Get operands
    let arg1 = func.dfg.inst_data(inst).args[0];
    let arg2 = func.dfg.inst_data(inst).args[1];
    let result = func.dfg.inst_data(inst).results[0];
    
    // For fixed-point multiply: result = (a * b) >> 16
    // 
    // Algorithm:
    // 1. Compute low 32 bits: lo = MUL(a, b)
    // 2. Compute high 32 bits: hi = MULH(a, b)
    // 3. Combine: result = (hi << 16) | (lo >> 16)
    //    Actually simpler: result = (hi << 16) + (lo >> 16)
    //    But we can optimize: if we only need the result shifted right by 16,
    //    we can use: result = (hi << 16) | (lo >> 16)
    //
    // More efficient approach:
    // 1. hi = MULH(a, b)  // High 32 bits of product
    // 2. lo = MUL(a, b)   // Low 32 bits of product  
    // 3. result = (hi << 16) | (lo >> 16)
    //    Or: result = (hi << 16) + (lo >> 16)  // Addition works too
    
    let block = ctx.current_block();
    let mut builder = ctx.builder_mut().block_builder(block);
    
    // Allocate temporaries
    let hi = ctx.builder_mut().new_value();
    let lo = ctx.builder_mut().new_value();
    let hi_shifted = ctx.builder_mut().new_value();
    let lo_shifted = ctx.builder_mut().new_value();
    
    // Compute high and low parts
    builder.imulh(hi, arg1, arg2);  // hi = high 32 bits of (a * b)
    builder.imul(lo, arg1, arg2);   // lo = low 32 bits of (a * b)
    
    // Shift: hi << 16, lo >> 16
    let shift_16 = ctx.builder_mut().new_value();
    builder.iconst(shift_16, 16);
    builder.ishl(hi_shifted, hi, shift_16);  // hi_shifted = hi << 16
    builder.ishr(lo_shifted, lo, shift_16);   // lo_shifted = lo >> 16
    
    // Combine: result = hi_shifted | lo_shifted
    builder.ior(result, hi_shifted, lo_shifted);
    
    // Update types
    func.dfg.set_value_type(hi, Type::I32);
    func.dfg.set_value_type(lo, Type::I32);
    func.dfg.set_value_type(hi_shifted, Type::I32);
    func.dfg.set_value_type(lo_shifted, Type::I32);
    func.dfg.set_value_type(shift_16, Type::I32);
    func.dfg.set_value_type(result, Type::I32);
}
```

**Note**: We can optimize further by using immediate shift operations if we add them, but register shifts work fine.

#### 5.2 Implement Fixed-Point Divide

For fixed-point divide `result = (a << 16) / b`:

```rust
fn convert_fdiv(ctx: &mut ConversionContext, inst: InstEntity, func: &Function) {
    // Get operands
    let arg1 = func.dfg.inst_data(inst).args[0];  // a
    let arg2 = func.dfg.inst_data(inst).args[1];   // b
    let result = func.dfg.inst_data(inst).results[0];
    
    // For fixed-point divide: result = (a << 16) / b
    // 
    // We need to divide a 64-bit number (a << 16) by a 32-bit number (b)
    // RISC-V doesn't have native 64-bit divide, so we use extended precision division
    //
    // Algorithm:
    // 1. Shift a left by 16: dividend_hi = a, dividend_lo = 0
    // 2. Use extended precision division algorithm
    //
    // Simplified approach (if a fits in 16 bits):
    //   result = (a << 16) / b
    //   But we need to handle the full 32-bit range of a
    //
    // Use library function call for divide
    // Call helper function: fixed_div(a, b) -> result
    // This will be implemented as a runtime library function
}
```

**Estimated Effort**: 2-3 days

### Phase 6: Update GLSL Frontend to Support Float

**Goal**: Enable GLSL frontend to generate float types and operations.

#### 6.1 Add Float Type to GLSL Type System

**File**: `crates/lpc-glsl/src/types.rs`

```rust
pub enum GlslType {
    Int,
    Bool,
    Float,  // Add this
}

impl GlslType {
    pub fn to_lpir(self) -> LpirType {
        match self {
            GlslType::Int => LpirType::I32,
            GlslType::Bool => LpirType::U32,
            GlslType::Float => LpirType::F32,  // Add this
        }
    }
    
    pub fn from_glsl_type_specifier(spec: &glsl::syntax::TypeSpecifierNonArray) -> Option<Self> {
        match spec {
            glsl::syntax::TypeSpecifierNonArray::Int => Some(GlslType::Int),
            glsl::syntax::TypeSpecifierNonArray::Bool => Some(GlslType::Bool),
            glsl::syntax::TypeSpecifierNonArray::Float => Some(GlslType::Float),  // Add this
            _ => None,
        }
    }
}
```

#### 6.2 Update Expression Codegen

**File**: `crates/lpc-glsl/src/expr/codegen.rs`

```rust
pub fn generate_expr(ctx: &mut dyn CodeGenContext, expr: &Expr) -> GlslResult<Value> {
    match expr {
        // ... existing cases ...
        Expr::FloatConst(f) => {
            let block = ctx.current_block()?;
            let value = ctx.builder_mut().new_value();
            let mut block_builder = ctx.builder_mut().block_builder(block);
            block_builder.fconst(value, *f);
            Ok(value)
        }
        // ... rest of cases ...
    }
}
```

#### 6.3 Add Float Arithmetic Operations

**File**: `crates/lpc-glsl/src/expr/codegen.rs`

Add codegen for float arithmetic expressions:
```rust
// In generate_expr, handle binary operations with float operands
Expr::Binary { op, left, right } => {
    let left_val = generate_expr(ctx, left)?;
    let right_val = generate_expr(ctx, right)?;
    
    // Check types
    let left_ty = ctx.builder().function().dfg.value_type(left_val);
    let right_ty = ctx.builder().function().dfg.value_type(right_val);
    
    if left_ty == Some(Type::F32) && right_ty == Some(Type::F32) {
        // Generate float operation
        let result = ctx.builder_mut().new_value();
        let block = ctx.current_block()?;
        let mut block_builder = ctx.builder_mut().block_builder(block);
        
        match op {
            glsl::syntax::BinaryOperator::Add => block_builder.fadd(result, left_val, right_val),
            glsl::syntax::BinaryOperator::Sub => block_builder.fsub(result, left_val, right_val),
            glsl::syntax::BinaryOperator::Mul => block_builder.fmul(result, left_val, right_val),
            glsl::syntax::BinaryOperator::Div => block_builder.fdiv(result, left_val, right_val),
            // ... comparisons ...
            _ => return Err(GlslError::codegen("Unsupported float operation")),
        }
        
        Ok(result)
    } else {
        // Handle integer/mixed operations
        // ...
    }
}
```

#### 6.4 Update Type Checker

**File**: `crates/lpc-glsl/src/typecheck.rs`

Update type checking to allow float types and operations.

**Estimated Effort**: 2-3 days

### Phase 7: Integration and Testing

**Goal**: Integrate transformation pass into compilation pipeline and add comprehensive tests.

#### 7.1 Integration Point

**File**: `crates/lpc-glsl/src/lib.rs` or compilation entry point

Add transformation pass after GLSL codegen:
```rust
pub fn compile_glsl_to_lpir(source: &str) -> Result<Function, GlslError> {
    // 1. Parse GLSL
    let ast = parse_glsl(source)?;
    
    // 2. Type check
    let type_checked = typecheck(ast)?;
    
    // 3. Generate LPIR
    let mut func = codegen(type_checked)?;
    
    // 4. Convert floats to fixed-point
    lpc_lpir::convert_floats_to_fixed16x16(&mut func)
        .map_err(|e| GlslError::transform(e))?;
    
    Ok(func)
}
```

#### 7.2 Add Tests

**File**: `crates/lpc-lpir/tests/transform_tests.rs`

Add tests for:
- Fconst conversion
- Fadd/Fsub conversion
- Fmul conversion (with various values)
- Fdiv conversion (with various values, including edge cases)
- Fcmp conversion (all condition codes)
- Function signature conversion
- Load/Store with F32 type conversion
- Complex functions with multiple float operations

**File**: `crates/lpc-glsl/tests/float_tests.rs`

Add end-to-end tests:
- Simple float arithmetic
- Float comparisons
- Float in function parameters/returns
- Float variables and assignments
- Nested float expressions

#### 7.3 Verify Backend Compatibility

Ensure backend3 can handle the transformed code:
- All operations are I32 (no F32 types remain)
- Fixed-point multiply/divide sequences are valid
- Register allocation works correctly
- Code generation produces correct RISC-V instructions

**Estimated Effort**: 2-3 days

## Missing RISC-V32 Operations Summary

### Critical: MULH (Multiply High)

**Status**: ❌ Not in LPIR, ❌ Not in backend3

**RISC-V Instruction**: `MULH rd, rs1, rs2` (M extension, funct7=0x01, funct3=0x1)

**Purpose**: Get high 32 bits of 64-bit signed multiply product

**Why Needed**: 
- Fixed-point multiply `(a * b) >> 16` requires the high 32 bits of the 64-bit product
- MULH makes fixed-point multiply efficient (4-5 instructions)

**Implementation Priority**: **HIGH** - Required for efficient fixed-point multiply

### Optional: Shift Immediate Operations

**Status**: ❌ Not in LPIR (has register shifts), ✅ Backend has SLLI/SRLI/SRAI

**RISC-V Instructions**: `SLLI`, `SRLI`, `SRAI` (shift by immediate 0-31)

**Purpose**: Shift by constant amount (e.g., 16 for fixed-point operations)

**Why Helpful**:
- More efficient than shift by register for constant shifts
- Reduces register pressure (no need for constant in register)
- Common in fixed-point operations (shifting by 16)

**Implementation Priority**: **MEDIUM** - Nice to have, but register shifts work

### Optional: MULHU (Multiply High Unsigned)

**Status**: ❌ Not in LPIR, ❌ Not in backend3

**RISC-V Instruction**: `MULHU rd, rs1, rs2` (M extension)

**Purpose**: Get high 32 bits for unsigned multiply

**Why Helpful**: May be needed for unsigned fixed-point operations

**Implementation Priority**: **LOW** - Can add later if needed for unsigned operations

## Fixed-Point Operation Details

### Multiplication Implementation

For `fixed_result = (fixed_a * fixed_b) >> 16`:

```rust
// Efficient fixed-point multiply using MULH
hi = imulh fixed_a, fixed_b      // High 32 bits of product
lo = imul fixed_a, fixed_b       // Low 32 bits of product
hi_shifted = ishl hi, 16         // hi << 16
lo_shifted = ishr lo, 16         // lo >> 16
result = ior hi_shifted, lo_shifted  // Combine: (hi << 16) | (lo >> 16)
```

This uses MULH (added in Phase 4) and is implemented in Phase 5.

### Division Implementation

For `fixed_result = (fixed_a << 16) / fixed_b`:

```rust
// Call helper function: fixed_div(a, b) -> result
// This will be implemented as a runtime library function
```

We use a library function call for fixed-point division (simpler than inline extended precision division).

## Error Handling

### Overflow/Underflow

**Approach**: Clamp values during conversion, but allow overflow in arithmetic (wrapping behavior).

**Rationale**:
- Fixed-point arithmetic naturally wraps (it's integer arithmetic)
- Clamping only at conversion points (constants, inputs)
- Matches behavior of many embedded systems

### Division by Zero

**Approach**: Let backend handle (may trap or have undefined behavior).

**Future**: Could add explicit checks before division.

## Testing Strategy

### Unit Tests

1. **Fixed-point conversion utilities**
   - Test `float_to_fixed16x16` with various values
   - Test edge cases (0.0, -0.0, max, min, out of range)
   - Test rounding behavior

2. **Instruction conversion**
   - Test each opcode conversion individually
   - Test with various operand values
   - Test edge cases (overflow, underflow, division by zero)

3. **Function transformation**
   - Test simple functions
   - Test functions with multiple float operations
   - Test functions with float parameters/returns
   - Test nested expressions

### Integration Tests

1. **End-to-end GLSL compilation**
   - Compile GLSL with float operations
   - Verify transformation produces valid LPIR
   - Verify backend can lower the code
   - Verify generated code executes correctly

2. **Precision tests**
   - Compare fixed-point results with expected float results
   - Test accumulation of errors
   - Test range limits

## Future Enhancements

1. **Explicit Fixed16x16 Type**: Add `Fixed16x16` type to LPIR for better type safety
2. **Optimizations**: Inline fixed-point multiply/divide instead of function calls
3. **Vector Support**: Extend to `vec2`, `vec3`, `vec4` with fixed-point components
4. **Better Overflow Handling**: Add explicit overflow checks and clamping
5. **I64 Support**: Add I64 type to LPIR for cleaner fixed-point operations
6. **Constant Folding**: Fold fixed-point operations at compile time when possible

## Files to Create/Modify

### New Files

- `crates/lpc-lpir/src/transform/mod.rs`
- `crates/lpc-lpir/src/transform/fixed_point.rs`
- `crates/lpc-lpir/tests/transform_tests.rs`
- `crates/lpc-glsl/tests/float_tests.rs`

### Modified Files

- `crates/lpc-lpir/src/lib.rs` (add transform module)
- `crates/lpc-lpir/src/dfg/opcode.rs` (add Fadd, Fsub, Fmul, Fdiv)
- `crates/lpc-lpir/src/dfg/mod.rs` (type inference for new opcodes)
- `crates/lpc-lpir/src/builder/block_builder.rs` (builder methods)
- `crates/lpc-lpir/src/parser/instructions.rs` (parsing)
- `crates/lpc-lpir/src/verifier/format.rs` (validation)
- `crates/lpc-lpir/src/verifier/types.rs` (type checking)
- `crates/lpc-glsl/src/types.rs` (add Float type)
- `crates/lpc-glsl/src/expr/codegen.rs` (float expression codegen)
- `crates/lpc-glsl/src/typecheck.rs` (float type checking)
- `crates/lpc-glsl/src/lib.rs` (integration point)

## Success Criteria

Phase 1 complete when:
- ✅ LPIR has Fadd, Fsub, Fmul, Fdiv opcodes
- ✅ All new opcodes parse, validate, and type-check correctly
- ✅ Builder API supports float arithmetic

Phase 2 complete when:
- ✅ Transform module structure exists
- ✅ Fixed-point conversion utilities work correctly

Phase 3 complete when:
- ✅ Fconst converts to Iconst correctly
- ✅ Fadd/Fsub convert to Iadd/Isub correctly
- ✅ Fmul converts to fixed-point multiply sequence
- ✅ Fdiv converts to fixed-point divide sequence
- ✅ Fcmp converts to Icmp with correct condition codes
- ✅ Function signatures convert correctly
- ✅ Load/Store with F32 convert correctly

Phase 4 complete when:
- ✅ MULH opcode added to LPIR
- ✅ MULH lowering implemented in backend3
- ✅ MULH encoding and emission implemented
- ✅ Tests pass for MULH

Phase 5 complete when:
- ✅ Fixed-point multiply uses MULH efficiently
- ✅ Fixed-point divide implementation (library or inline)
- ✅ No precision loss beyond expected fixed-point limitations

Phase 6 complete when:
- ✅ GLSL frontend accepts float types
- ✅ GLSL frontend generates float operations
- ✅ Float expressions compile correctly

Phase 7 complete when:
- ✅ Transformation pass integrated into compilation pipeline
- ✅ End-to-end tests pass
- ✅ Backend3 can compile transformed code
- ✅ Generated code executes correctly on target hardware

## References

- Fixed-point arithmetic: https://en.wikipedia.org/wiki/Fixed-point_arithmetic
- Q16.16 format: Common fixed-point format with 16 integer and 16 fractional bits
- RISC-V 32-bit IMAC: Base integer instruction set without floating-point extension

