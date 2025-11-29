# GLSL Value Type Migration Plan

## Overview

Migrate from direct `Value` usage to `GlslValue`/`GlslLValue`/`GlslRValue` abstractions to support future struct/array types and provide cleaner semantics for assignments and variable access.

## Current State

- Variables stored as `BTreeMap<String, Value>`
- Expression codegen returns `Value` directly
- Assignments directly update `Value` in map
- No distinction between lvalues and rvalues
- `GlslValue` types exist but unused (`codegen/value.rs`)

## Target State

- Variables stored as `BTreeMap<String, GlslValue>`
- Variable reads return `GlslLValue` (wraps address for scalars, actual address for aggregates)
- Expression results return `GlslRValue`
- Assignments use `GlslLValue::store()`
- Clear separation between lvalue/rvalue semantics

## Why This Matters

### Current Limitations

1. **No aggregate support**: Can't represent structs/arrays (need addresses)
2. **Unclear semantics**: Assignment vs. use distinction not enforced
3. **Out/inout parameters**: Currently handled ad-hoc with manual load/store

### Benefits After Migration

1. **Struct support**: `GlslLValue` can represent struct addresses
2. **Array support**: `GlslRValue::Aggregate` can represent array addresses
3. **Cleaner code**: Type system enforces lvalue/rvalue distinction
4. **Future-proof**: Ready for struct field access, array indexing, etc.

## Migration Strategy

### Phase 1: Scalar LValue Support

**Goal**: For scalar types (int, bool, float), wrap `Value` in `GlslLValue` even though they're already SSA values.

**Changes**:

1. **Update `CodeGenContext` trait** (`function/codegen.rs`)
   - Change `variables()` return type to `&BTreeMap<String, GlslValue>`
   - Change `variables_mut()` return type to `&mut BTreeMap<String, GlslValue>`
   - Update `clone_variables()` and `restore_variables()` signatures

2. **Update `CodeGen` struct** (`function/codegen.rs`)
   - Change `variables: BTreeMap<String, Value>` to `variables: BTreeMap<String, GlslValue>`
   - Update initialization to use `GlslValue::rvalue(GlslRValue::Scalar(value))` for parameters

3. **Create helper methods** (`function/codegen.rs`)
   ```rust
   /// Get variable as lvalue (for assignments)
   fn get_variable_lvalue(&self, name: &str) -> GlslResult<GlslLValue>;
   
   /// Get variable as rvalue (for reads)
   fn get_variable_rvalue(&mut self, name: &str) -> GlslResult<GlslRValue>;
   ```

4. **Update variable declaration** (`decl/codegen.rs`)
   - Store as `GlslValue::rvalue(GlslRValue::Scalar(value))` instead of `Value`
   - For scalars, we still store as RValue (since they're SSA values)

5. **Update variable reads** (`expr/codegen.rs`)
   - `Expr::Variable` should return `GlslRValue` instead of `Value`
   - For scalars: get from variables map, extract `GlslRValue::Scalar(value)`
   - Use lazy SSA construction to get the value

6. **Update assignments** (`expr/codegen.rs`)
   - `Expr::Assignment` should:
     - Get lvalue from variable (create if needed)
     - Evaluate rhs as rvalue
     - Call `GlslLValue::store()` (for scalars, this is a no-op, just update map)
     - Record in SSABuilder

**Files to Modify**:
- `crates/lpc-glsl/src/function/codegen.rs`
- `crates/lpc-glsl/src/expr/codegen.rs`
- `crates/lpc-glsl/src/decl/codegen.rs`

**Challenges**:
- Need to handle transition period where some code expects `Value`
- For scalars, `GlslLValue` wraps a `Value` (not an address) - need to clarify semantics
- Control flow codegen may need updates to handle `GlslValue` in variable maps

### Phase 2: Expression Return Types

**Goal**: All expression codegen returns `GlslRValue` instead of `Value`.

**Changes**:

1. **Update `generate_expr` signature** (`expr/codegen.rs`)
   - Change return type from `GlslResult<Value>` to `GlslResult<GlslRValue>`
   - Update all call sites

2. **Update literal codegen** (`expr/codegen.rs`)
   - `Expr::IntConst` → `GlslRValue::Scalar(value)`
   - `Expr::BoolConst` → `GlslRValue::Scalar(value)`
   - `Expr::FloatConst` → `GlslRValue::Scalar(value)`

3. **Update operator codegen** (`expr/codegen.rs`)
   - `generate_unary_op` → returns `GlslRValue`
   - `generate_binary_op` → returns `GlslRValue`
   - Extract `Value` from operands using `.to_value()`

4. **Update function call codegen** (`expr/codegen.rs`)
   - `generate_function_call` → returns `GlslRValue::Scalar(return_value)`

5. **Update control flow codegen** (`control/codegen.rs`)
   - Extract `Value` from `GlslRValue` when needed for phi nodes
   - Use `.to_value()` to convert

**Files to Modify**:
- `crates/lpc-glsl/src/expr/codegen.rs`
- `crates/lpc-glsl/src/control/codegen.rs`
- All call sites of `generate_expr`

**Challenges**:
- Many places expect `Value` - need systematic conversion
- Phi nodes need `Value`, so need `.to_value()` calls

### Phase 3: Scalar LValue Implementation

**Goal**: For scalars, `GlslLValue` should wrap the SSA `Value` directly (not an address).

**Changes**:

1. **Update `GlslLValue` semantics** (`codegen/value.rs`)
   - For scalars: `address` field contains the SSA `Value` (not a memory address)
   - `is_reference` field distinguishes SSA values from memory addresses
   - `store()` for scalars: just update the value (no actual store instruction)
   - `load()` for scalars: return the value directly (no load instruction)

2. **Update variable storage** (`function/codegen.rs`)
   - For scalar variables: store as `GlslValue::lvalue(GlslLValue::new(value, ty, ...))`
   - This allows assignments to use `store()` method

3. **Update variable reads** (`expr/codegen.rs`)
   - For scalars: get `GlslLValue`, call `.to_rvalue()` to get `GlslRValue`
   - This ensures we go through the proper abstraction

**Files to Modify**:
- `crates/lpc-glsl/src/codegen/value.rs`
- `crates/lpc-glsl/src/function/codegen.rs`
- `crates/lpc-glsl/src/expr/codegen.rs`

**Challenges**:
- Need to clarify that for scalars, `GlslLValue.address` is the SSA value, not a memory address
- This is a bit of a semantic hack, but necessary for uniform API

### Phase 4: Out/Inout Parameter Support

**Goal**: Use `GlslLValue` for out/inout parameters (these are actual addresses).

**Changes**:

1. **Update parameter handling** (`function/codegen.rs`)
   - For `out`/`inout`: create `GlslLValue` with actual address
   - Store in variables map as `GlslValue::lvalue(...)`
   - Set `is_reference: true`

2. **Update parameter reads** (`expr/codegen.rs`)
   - For `inout`: call `.load()` to get initial value
   - For `out`: use default value initially

3. **Update return handling** (`function/codegen.rs`)
   - Before return: iterate `out_inout_params`, get `GlslLValue` from variables
   - Call `.store()` with current value
   - This will generate actual `store` instructions

**Files to Modify**:
- `crates/lpc-glsl/src/function/codegen.rs`
- `crates/lpc-glsl/src/expr/codegen.rs`

**Benefits**:
- Cleaner code: out/inout handled uniformly with other assignments
- Less ad-hoc logic

### Phase 5: Testing and Validation

**Goal**: Ensure all tests pass and add tests for value type handling.

**Changes**:

1. **Fix existing tests**
   - Update tests that expect `Value` to handle `GlslRValue`
   - Extract `.to_value()` where needed

2. **Add new tests** (`tests/value_tests.rs`)
   - Test scalar variable assignment/read
   - Test out/inout parameter handling
   - Test expression result types
   - Test lvalue/rvalue conversion

3. **Verify SSA correctness**
   - Ensure lazy SSA construction still works
   - Verify phi nodes are created correctly
   - Check dominance violations

**Files to Modify**:
- `crates/lpc-glsl/tests/complex_tests.rs`
- `crates/lpc-glsl/tests/expression_tests.rs`
- `crates/lpc-glsl/tests/variable_tests.rs`
- New: `crates/lpc-glsl/tests/value_tests.rs`

## Implementation Order

1. **Phase 1** (Scalar LValue Support) - Foundation
2. **Phase 2** (Expression Return Types) - Core migration
3. **Phase 3** (Scalar LValue Implementation) - Complete scalar support
4. **Phase 4** (Out/Inout Support) - Use new types for parameters
5. **Phase 5** (Testing) - Validate everything works

## Key Design Decisions

### Scalar LValue Semantics

For scalar types, `GlslLValue.address` contains the SSA `Value`, not a memory address. This is a semantic convenience to allow uniform API:

- `GlslLValue::store()` for scalars: updates the value in the map (no store instruction)
- `GlslLValue::load()` for scalars: returns the value directly (no load instruction)
- `is_reference: false` indicates this is an SSA value, not a memory address

For aggregates (future):
- `GlslLValue.address` will contain actual memory address
- `GlslLValue::store()` will generate `store` instruction
- `GlslLValue::load()` will generate `load` instruction
- `is_reference: true` indicates this is a memory address

### Variable Storage Strategy

**Scalar variables**: Store as `GlslValue::lvalue(GlslLValue::new(value, ...))`
- Allows using `.store()` for assignments
- Allows using `.to_rvalue()` for reads

**Out/inout parameters**: Store as `GlslValue::lvalue(GlslLValue::new(address, ..., is_reference: true))`
- Actual memory addresses
- `.store()` generates real store instructions

## Future Extensions

Once this migration is complete, adding struct/array support becomes straightforward:

1. **Struct types**: `GlslLValue` with struct address
2. **Array types**: `GlslRValue::Aggregate` for array addresses
3. **Field access**: `GlslLValue` for struct fields
4. **Array indexing**: `GlslRValue::Aggregate` for array elements

## Success Criteria

1. All existing tests pass
2. Variables stored as `GlslValue` in map
3. Expression codegen returns `GlslRValue`
4. Assignments use `GlslLValue::store()`
5. Out/inout parameters use `GlslLValue` with addresses
6. No performance regression (scalars should be zero-cost abstraction)
7. Code ready for struct/array support

## Reference

- Original architecture plan: `docs/glsl/03-architecture.md`
- Value type definitions: `crates/lpc-glsl/src/codegen/value.rs`
- Clang CGValue system (inspiration)

