# GLSL Frontend Architecture Plan

## Overview

This document outlines the planned architectural improvements for the GLSL frontend (`lpc-glsl`), inspired by Clang's codegen architecture and addressing current limitations in SSA construction, value representation, and code organization.

## Current Problems

### 1. SSA Construction Issues

**Problem**: Manual phi node creation leads to dominance violations and incorrect SSA form.

**Symptoms**:
- Variables modified in nested control flow don't get proper phi nodes
- Values from non-dominating blocks are used in phi nodes
- Test failures in `test_nested_control_flow` due to dominance violations

**Root Cause**: No abstraction for SSA construction - we manually track variables and create phi nodes, but don't properly handle dominance.

### 2. Value Representation

**Problem**: No distinction between lvalues and rvalues, or different value kinds.

**Current State**: Everything is just a `Value` (SSA value).

**Issues**:
- Can't properly represent aggregates (structs, arrays)
- Assignment semantics unclear
- Out/inout parameters handled ad-hoc

### 3. Code Organization

**Problem**: Single 1800+ line `codegen.rs` file with mixed concerns.

**Issues**:
- Hard to maintain and test
- Expression, statement, and declaration codegen all mixed together
- No clear separation of concerns

### 4. Loop Handling

**Problem**: No proper loop context tracking.

**Issues**:
- Can't properly handle break/continue (not yet implemented)
- Loop variable phi nodes created incorrectly
- No tracking of which variables are modified in loops

### 5. Scope Management

**Problem**: Manual scope tracking with `scope_stack`.

**Issues**:
- Error-prone manual push/pop
- No RAII guarantees
- Hard to extend for future features (cleanups, exception handling)

## New Architecture

### Module Structure

```
crates/lpc-glsl/src/
  codegen/
    mod.rs              // CodeGen struct, main entry point
    context.rs          // CodeGenContext trait
    value.rs            // GlslValue, GlslLValue abstractions
    ssa.rs              // SSABuilder for proper SSA construction
    loop.rs             // LoopInfo, LoopStack
    scope.rs            // Scope, ScopeStack
    expr.rs             // Expression codegen
    stmt.rs             // Statement codegen (if/for/while/return)
    decl.rs             // Declaration codegen
    builder.rs          // CodeGenBuilder wrapper
```

### Core Abstractions

#### 1. Value Representation (`value.rs`)

```rust
/// Represents an rvalue - the result of evaluating an expression
pub enum GlslRValue {
    /// Simple scalar value (int, float, bool)
    Scalar(Value),
    /// Address of an aggregate (for structs/arrays - future)
    Aggregate(Value),
    // Future: Complex, Vector, etc.
}

/// Represents an lvalue - something that can be assigned to
pub struct GlslLValue {
    /// Address of the value
    address: Value,
    /// Type of the value
    ty: GlslType,
    /// Alignment requirement
    alignment: u32,
    /// Whether this is a reference parameter
    is_reference: bool,
}

/// A value that can be either an lvalue or rvalue
pub enum GlslValue {
    LValue(GlslLValue),
    RValue(GlslRValue),
}

impl GlslRValue {
    /// Convert to SSA value (for scalar) or get address (for aggregate)
    pub fn to_value(&self) -> Value;
    
    /// Load from memory if aggregate
    pub fn load(self, builder: &mut CodeGenBuilder) -> GlslRValue;
}

impl GlslLValue {
    /// Store an rvalue into this lvalue
    pub fn store(self, value: GlslRValue, builder: &mut CodeGenBuilder);
    
    /// Load the value from this lvalue
    pub fn load(self, builder: &mut CodeGenBuilder) -> GlslRValue;
}
```

**Benefits**:
- Clear distinction between assignments and uses
- Proper handling of aggregates when we add struct support
- Cleaner semantics for out/inout parameters

#### 2. SSA Construction (`ssa.rs`)

```rust
/// Builder for SSA form that handles phi node insertion automatically
pub struct SSABuilder {
    /// Map from variable name to map of block -> value
    /// Tracks the definition of each variable in each block
    defs: BTreeMap<String, BTreeMap<Block, Value>>,
    
    /// Set of variables that need phi nodes (modified in multiple blocks)
    needs_phi: BTreeSet<String>,
    
    /// Dominance tree for the function
    domtree: DominatorTree,
}

impl SSABuilder {
    /// Get the SSA value for a variable at a given block
    /// Automatically inserts phi nodes if needed
    pub fn get_value(
        &mut self,
        var: &str,
        block: Block,
        builder: &mut FunctionBuilder,
    ) -> Value {
        // Check if variable is defined in this block
        if let Some(value) = self.defs.get(var).and_then(|m| m.get(&block)) {
            return *value;
        }
        
        // Check if we need a phi node
        if self.needs_phi.contains(var) {
            // Get all predecessors
            let preds = builder.function().block_preds(block);
            
            // Collect values from all predecessors
            let mut incoming: Vec<(Block, Value)> = Vec::new();
            for pred in preds {
                if let Some(value) = self.get_value(var, *pred, builder) {
                    incoming.push((*pred, value));
                }
            }
            
            // Create phi node if we have multiple incoming values
            if incoming.len() > 1 {
                let phi = builder.new_value();
                // Create phi instruction
                // ...
                return phi;
            }
        }
        
        // Single definition - use it directly
        // ...
    }
    
    /// Record a definition of a variable in a block
    pub fn record_def(&mut self, var: &str, block: Block, value: Value) {
        self.defs
            .entry(var.to_string())
            .or_insert_with(BTreeMap::new)
            .insert(block, value);
        
        // Mark as needing phi if defined in multiple blocks
        if let Some(blocks) = self.defs.get(var) {
            if blocks.len() > 1 {
                self.needs_phi.insert(var.to_string());
            }
        }
    }
    
    /// Finalize phi nodes for all variables that need them
    pub fn finalize_phi_nodes(&mut self, builder: &mut FunctionBuilder) {
        // Insert phi nodes at merge points
        // ...
    }
}
```

**Benefits**:
- Automatic phi node insertion
- Proper dominance handling
- Fixes current dominance violations

#### 3. Loop Tracking (`loop.rs`)

```rust
/// Information about a loop
pub struct LoopInfo {
    /// Loop header block (where condition is checked)
    header: Block,
    
    /// Loop exit block (where we go when condition is false)
    exit: Block,
    
    /// Continue block (where we go on continue) - optional
    continue_block: Option<Block>,
    
    /// Variables modified in this loop (need phi nodes)
    modified_vars: BTreeSet<String>,
    
    /// Variables used in loop condition
    cond_vars: BTreeSet<String>,
}

/// Stack of nested loops
pub struct LoopStack {
    loops: Vec<LoopInfo>,
}

impl LoopStack {
    pub fn push(&mut self, info: LoopInfo);
    pub fn pop(&mut self) -> Option<LoopInfo>;
    pub fn current(&self) -> Option<&LoopInfo>;
    pub fn find_break_target(&self) -> Option<Block>;
    pub fn find_continue_target(&self) -> Option<Block>;
}
```

**Benefits**:
- Proper break/continue handling
- Better identification of loop variables needing phi nodes
- Cleaner loop codegen

#### 4. Scope Management (`scope.rs`)

```rust
/// A lexical scope
pub struct Scope {
    /// Variables declared in this scope
    variables: BTreeSet<String>,
    
    /// Cleanup actions (future: for destructors, exception handling)
    cleanups: Vec<CleanupAction>,
}

/// Stack of scopes
pub struct ScopeStack {
    scopes: Vec<Scope>,
}

impl ScopeStack {
    pub fn push(&mut self) {
        self.scopes.push(Scope::new());
    }
    
    pub fn pop(&mut self) -> Option<Scope> {
        self.scopes.pop()
    }
    
    pub fn declare(&mut self, name: String) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.variables.insert(name);
        }
    }
    
    pub fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.variables.contains(name))
    }
}

/// RAII guard for scope entry/exit
pub struct ScopeGuard<'a> {
    stack: &'a mut ScopeStack,
}

impl<'a> ScopeGuard<'a> {
    pub fn new(stack: &'a mut ScopeStack) -> Self {
        stack.push();
        Self { stack }
    }
}

impl<'a> Drop for ScopeGuard<'a> {
    fn drop(&mut self) {
        self.stack.pop();
    }
}
```

**Benefits**:
- RAII guarantees for scope management
- Less error-prone than manual push/pop
- Extensible for future features

#### 5. CodeGen Context (`context.rs`)

```rust
/// Trait for accessing CodeGen internals
/// Allows codegen modules to access shared state without tight coupling
pub trait CodeGenContext {
    fn current_block(&self) -> GlslResult<Block>;
    fn set_current_block(&mut self, block: Block);
    
    fn builder_mut(&mut self) -> &mut FunctionBuilder;
    
    fn ssa_builder_mut(&mut self) -> &mut SSABuilder;
    
    fn loop_stack_mut(&mut self) -> &mut LoopStack;
    
    fn scope_stack_mut(&mut self) -> &mut ScopeStack;
    
    fn variables(&self) -> &BTreeMap<String, GlslValue>;
    fn variables_mut(&mut self) -> &mut BTreeMap<String, GlslValue>;
    
    fn symbols(&self) -> &SymbolTable;
}
```

**Benefits**:
- Decouples codegen modules from CodeGen internals
- Easier to test individual modules
- Clear interface for shared state

#### 6. CodeGen Builder (`builder.rs`)

```rust
/// Wrapper around BlockBuilder that adds metadata and debug info
pub struct CodeGenBuilder<'a> {
    block_builder: BlockBuilder<'a>,
    context: &'a mut CodeGen,
}

impl<'a> CodeGenBuilder<'a> {
    /// Create a constant
    pub fn iconst(&mut self, value: Value, imm: i64) {
        // Add debug info, metadata, etc.
        self.block_builder.iconst(value, imm);
    }
    
    /// Add two values
    pub fn iadd(&mut self, result: Value, lhs: Value, rhs: Value) {
        // Add debug info, metadata, etc.
        self.block_builder.iadd(result, lhs, rhs);
    }
    
    // ... other operations
}
```

**Benefits**:
- Centralized place for adding metadata
- Future: debug info, profiling, etc.
- Cleaner API

### CodeGen Structure (`mod.rs`)

```rust
pub struct CodeGen {
    /// Function builder
    builder: FunctionBuilder,
    
    /// Current block being built
    current_block: Option<Block>,
    
    /// SSA builder for proper SSA construction
    ssa_builder: SSABuilder,
    
    /// Loop stack for tracking nested loops
    loop_stack: LoopStack,
    
    /// Scope stack for variable scoping
    scope_stack: ScopeStack,
    
    /// Variable name to value mapping (for current scope)
    variables: BTreeMap<String, GlslValue>,
    
    /// Symbol table for function lookups
    symbols: SymbolTable,
    
    /// Out/inout parameter tracking
    out_inout_params: BTreeMap<String, (Value, GlslType)>,
}

impl CodeGen {
    pub fn new(name: String, signature: Signature) -> Self {
        let builder = FunctionBuilder::new(signature, name);
        Self {
            builder,
            current_block: None,
            ssa_builder: SSABuilder::new(),
            loop_stack: LoopStack::new(),
            scope_stack: ScopeStack::new(),
            variables: BTreeMap::new(),
            symbols: SymbolTable::new(),
            out_inout_params: BTreeMap::new(),
        }
    }
    
    /// Get a builder for the current block
    pub fn builder(&mut self) -> CodeGenBuilder {
        let block = self.current_block.expect("No current block");
        let block_builder = self.builder.block_builder(block);
        CodeGenBuilder {
            block_builder,
            context: self,
        }
    }
}

impl CodeGenContext for CodeGen {
    // Implement trait methods
}
```

## Implementation Plan

### Phase 1: Foundation (Value Representation)

1. **Create `value.rs`**
   - Implement `GlslRValue`, `GlslLValue`, `GlslValue`
   - Add conversion methods
   - Update existing code to use new types gradually

2. **Update assignment handling**
   - Use `GlslLValue::store()` for assignments
   - Use `GlslLValue::load()` for variable reads

**Estimated effort**: 2-3 days

### Phase 2: SSA Construction

1. **Create `ssa.rs`**
   - Implement `SSABuilder` with dominance-aware phi insertion
   - Integrate with `lpc-lpir` dominance analysis

2. **Update variable handling**
   - Replace manual `variables` map with `SSABuilder`
   - Update all variable reads/writes to use SSA builder

3. **Test with nested control flow**
   - Fix `test_nested_control_flow`
   - Verify dominance violations are resolved

**Estimated effort**: 3-4 days

### Phase 3: Module Separation

1. **Create module structure**
   - `expr.rs` - Move expression codegen
   - `stmt.rs` - Move statement codegen
   - `decl.rs` - Move declaration codegen
   - `context.rs` - Create CodeGenContext trait
   - `builder.rs` - Create CodeGenBuilder wrapper

2. **Refactor CodeGen**
   - Split into modules
   - Use CodeGenContext trait
   - Update all call sites

**Estimated effort**: 2-3 days

### Phase 4: Loop and Scope Improvements

1. **Create `loop.rs`**
   - Implement `LoopInfo` and `LoopStack`
   - Update loop codegen to use loop stack

2. **Create `scope.rs`**
   - Implement `Scope` and `ScopeStack` with RAII
   - Replace manual scope tracking

3. **Add break/continue support**
   - Use loop stack to find targets
   - Generate proper jumps

**Estimated effort**: 2-3 days

### Phase 5: Cleanup and Testing

1. **Remove old code**
   - Clean up manual phi node creation
   - Remove manual variable tracking
   - Remove manual scope tracking

2. **Add comprehensive tests**
   - Test nested control flow
   - Test loops with break/continue
   - Test variable scoping

3. **Documentation**
   - Update code comments
   - Add module-level docs
   - Update architecture docs

**Estimated effort**: 2-3 days

## Migration Strategy

### Incremental Migration

1. **Add new abstractions alongside old code**
   - Don't break existing functionality
   - Add new types and modules incrementally

2. **Update one feature at a time**
   - Start with value representation
   - Then SSA construction
   - Then module separation
   - Finally loop/scope improvements

3. **Keep tests passing**
   - Run tests after each change
   - Fix regressions immediately
   - Add new tests for new features

### Backward Compatibility

- Keep `CodeGen::new()` API the same
- Keep `generate_function()` API the same
- Internal refactoring doesn't affect external API

## Benefits of New Architecture

1. **Correctness**
   - Proper SSA form with automatic phi insertion
   - No more dominance violations
   - Correct handling of nested control flow

2. **Maintainability**
   - Clear separation of concerns
   - Smaller, focused modules
   - Easier to test and debug

3. **Extensibility**
   - Easy to add new value types (structs, arrays)
   - Easy to add new control flow (switch, break/continue)
   - Easy to add new features (debug info, profiling)

4. **Performance**
   - Better SSA construction (fewer redundant phi nodes)
   - More opportunities for optimization

## Future Enhancements

Once the new architecture is in place:

1. **Struct Support**
   - Use `GlslLValue` for struct fields
   - Aggregate value representation

2. **Array Support**
   - Array indexing with bounds checking
   - Array value representation

3. **Debug Information**
   - Source location tracking
   - Variable name preservation
   - Debug metadata generation

4. **Optimizations**
   - Dead code elimination
   - Constant folding
   - Loop optimizations

## References

- Clang CodeGen: `/Users/yona/dev/photomancer/DirectXShaderCompiler/tools/clang/lib/CodeGen/`
- LLVM SSAUpdater: `llvm/Transforms/Utils/SSAUpdater.h`
- Clang CGValue: `tools/clang/lib/CodeGen/CGValue.h`
- Clang CodeGenFunction: `tools/clang/lib/CodeGen/CodeGenFunction.h`

