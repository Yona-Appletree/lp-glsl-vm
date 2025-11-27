# LPIR - Low-level Program Intermediate Representation

LPIR is a SSA-based intermediate representation designed for code generation targeting RISC-V. It provides a text format similar to Cranelift's CLIF for representing functions with basic blocks, instructions, and explicit control flow.

## Features

- **SSA Form**: Single Static Assignment with explicit value scoping
- **Basic Blocks**: Control flow via basic blocks with parameters (phi nodes)
- **Explicit Parameter Passing**: Values must be explicitly passed to blocks via jump/branch arguments
- **Text Format**: Human-readable IR syntax with parsing support
- **Type System**: Supports i32, i64, f32, f64 types

## Validation

All parsed functions are automatically validated for:

- **Block Indices**: Jump/branch targets must reference valid blocks
- **Parameter Counts**: Jump/branch arguments must match target block parameter counts
- **Return Values**: Return instructions must match function signature return count
- **SSA Properties**: Values cannot be defined multiple times in the same block
- **Terminating Instructions**: All blocks must end with return/jump/branch/halt
- **Entry Block**: Entry block parameters must match function signature
- **Value Scoping**: Values can only be used within their defining block or passed as parameters

## Limitations

- **No Type Checking**: Parameter and return value types are not validated (only counts)
- **No Dominance Validation**: No verification of proper SSA dominance relationships
- **No Dead Code Detection**: Unreachable blocks are not detected
- **No Call Validation**: Function calls are not validated against module definitions
- **Simple Type System**: No aggregate types, pointers, or complex types
- **No Optimization Passes**: IR is not optimized (intended for lowering to target code)

## Example

### Simple Function

```rust
use lpc_lpir::parse_function;

let ir = r#"
function %add(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}
"#;

let func = parse_function(ir)?;
```

### Recursive Fibonacci with Branching

This example demonstrates branching, multiple blocks, block parameters, and recursive calls. Note that values must be explicitly passed to blocks:

```rust
use lpc_lpir::parse_module;

let ir = r#"
module {
entry: %fib

function %fib(i32) -> i32 {
block0(v0: i32):
    v1 = iconst 1
    v2 = icmp_le v0, v1
    brif v2, block1(v0), block2(v0, v1)

block1(v3: i32):
    return v3

block2(v4: i32, v5: i32):
    v6 = iconst 2
    v7 = isub v4, v5
    v8 = isub v4, v6
    call %fib(v7) -> v9
    call %fib(v8) -> v10
    v11 = iadd v9, v10
    return v11
}
}"#;

let module = parse_module(ir)?;
```

Key features demonstrated:

- **Branching**: `brif v2, block1(v0), block2(v0, v1)` conditionally branches and passes values to target blocks
- **Multiple blocks**: Three blocks with different control flow paths
- **Block parameters**:
  - `block1(v3: i32)` receives `v0` from the branch
  - `block2(v4: i32, v5: i32)` receives `v0` and `v1` from the branch
- **Value scoping**: Values defined in `block0` (`v0`, `v1`) must be passed as parameters to `block1` and `block2` since they're used there
- **Recursive calls**: `call %fib(v7) -> v9` calls the same function recursively with computed values
