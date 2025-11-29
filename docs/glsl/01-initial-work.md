# GLSL Frontend: Initial Work

## Overview

This document outlines the initial implementation plan for a GLSL frontend compiler that translates GLSL source code to LPIR (Low-level Program Intermediate Representation). This is the first step toward building a complete GLSL compiler frontend that can be expanded to support the full GLSL specification.

## Goals

1. **Parse GLSL**: Use `glsl-parser` to parse GLSL source code into an AST
2. **Type Checking**: Implement basic type checking for a small subset of GLSL
3. **Code Generation**: Generate LPIR from the type-checked AST
4. **Test Infrastructure**: Build utilities to compile and test GLSL programs end-to-end

## Initial Scope

For the initial implementation, we will support a minimal but useful subset of GLSL:

### Supported Types

- **`int`**: 32-bit signed integers (maps to LPIR `I32`)
- **`bool`**: Boolean values (maps to LPIR `U32`, where 0 = false, 1 = true)

**Not supported initially:**

- `float`, `vec2`, `vec3`, `vec4`, etc.
- `struct` types
- Arrays
- Samplers and textures

### Supported Functions

- Function definitions with parameters and return values
- Function calls (including recursion)
- **Parameter qualifiers**: `in`, `out`, `inout`
  - `in` (default): Pass by value
  - `out`: Pass by reference (caller allocates, callee writes)
  - `inout`: Pass by reference (caller allocates, callee reads and writes)

**Implementation approach for `out`/`inout`:**

- Caller allocates space on the stack for the parameter value
- Caller passes the address (as `I32`) to the callee
- Callee uses `Load` to read (`inout` only) and `Store` to write
- This follows the standard approach used by reference compilers (glslang, DirectXShaderCompiler)

### Supported Control Flow

- **`if`/`else`**: Conditional branching
- **`for` loops**: With initialization, condition, and increment
- **`while` loops**: With condition

**Not supported initially:**

- `switch`/`case`
- `do`/`while`
- `break`/`continue`
- `discard` (fragment shader specific)

### Supported Expressions

- Integer literals (`42`, `-10`)
- Boolean literals (`true`, `false`)
- Variable references
- Binary operators: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `<=`, `>`, `>=`, `&&`, `||`
- Unary operators: `-`, `!`
- Function calls
- Assignment (`=`)
- Parenthesized expressions

**Not supported initially:**

- Ternary operator (`?:`)
- Array indexing
- Struct field access
- Vector operations

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    GLSL Source Code                         │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Parser (glsl-parser)                    │
│  - Parse GLSL to AST (TranslationUnit)                     │
│  - Handle syntax errors                                    │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Type Checker                            │
│  - Build symbol table (functions, variables)              │
│  - Check types match (arguments, returns, assignments)     │
│  - Resolve function calls                                  │
│  - Validate control flow                                   │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Code Generator                          │
│  - Convert AST to LPIR using FunctionBuilder               │
│  - Handle SSA construction                                 │
│  - Generate control flow blocks                            │
│  - Handle out/inout parameters                             │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    LPIR Module                             │
│  - Functions with signatures                               │
│  - Ready for backend compilation                           │
└─────────────────────────────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Project Setup

**Crate**: `crates/lpc-glsl`

**Files to create:**

- `Cargo.toml`: Dependencies (glsl-parser, lpc-lpir)
- `src/lib.rs`: Public API
- `src/parser.rs`: GLSL parsing wrapper
- `src/typecheck.rs`: Type checking module
- `src/codegen.rs`: LPIR code generation
- `src/error.rs`: Error types
- `src/symbols.rs`: Symbol table for type checking

**Dependencies:**

```toml
[dependencies]
glsl = { path = "../../../glsl-parser/glsl", default-features = false }
lpc-lpir = { path = "../lpc-lpir" }
```

### Phase 2: Basic Parsing Infrastructure

**Goal**: Parse GLSL and extract function definitions

**Tasks:**

1. Wrap `glsl-parser` to parse GLSL source
2. Extract `FunctionDefinition` nodes from `TranslationUnit`
3. Build basic error reporting
4. Write unit tests for parsing simple functions

**Example unit test:**

```rust
#[test]
fn test_parse_simple_function() {
    let glsl = r#"
        int add(int x, int y) {
            return x + y;
        }
    "#;
    // Parse and verify function is extracted
    let result = parse_glsl(glsl);
    assert!(result.is_ok());
    let functions = result.unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].prototype.name.as_str(), "add");
}
```

### Phase 3: Type System and Symbol Table

**Goal**: Build type checking infrastructure

**Tasks:**

1. Define GLSL type representation (`int`, `bool`)
2. Map GLSL types to LPIR types (`int` → `I32`, `bool` → `U32`)
3. Build symbol table:
   - Function signatures (name, parameters, return type)
   - Variable declarations (name, type, scope)
4. Handle scoping (function parameters, local variables)
5. Write tests for symbol table operations

**Key structures:**

```rust
pub enum GlslType {
    Int,
    Bool,
}

pub struct FunctionSignature {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: Option<GlslType>,
}

pub struct Parameter {
    pub qualifier: ParameterQualifier, // in, out, inout
    pub ty: GlslType,
    pub name: String,
}
```

### Phase 4: Expression Type Checking

**Goal**: Type check expressions

**Tasks:**

1. Type check literals (int, bool)
2. Type check variable references (lookup in symbol table)
3. Type check binary operators (type promotion, result types)
4. Type check unary operators
5. Type check function calls (argument count/type matching)
6. Write unit tests for expression type checking
7. Write integration tests using `GlslTest` for expression compilation

**Example:**

```rust
// int x = 10 + 20;  // OK: int + int = int
// bool b = x;       // Error: cannot assign int to bool
// int y = x + true; // Error: cannot add int and bool
```

### Phase 5: Statement Type Checking

**Goal**: Type check statements and control flow

**Tasks:**

1. Type check variable declarations
2. Type check assignments (LHS and RHS types must match)
3. Type check `if`/`else` (condition must be bool)
4. Type check `for` loops (init, condition, increment)
5. Type check `while` loops (condition must be bool)
6. Type check `return` statements (must match function return type)
7. Write unit tests for statement type checking
8. Write integration tests using `GlslTest` for statement compilation

### Phase 6: Basic Code Generation

**Goal**: Generate LPIR for simple functions

**Tasks:**

1. Create `CodeGen` struct with `FunctionBuilder`
2. Generate LPIR for:
   - Function signatures
   - Integer/boolean constants
   - Variable declarations (SSA values)
   - Arithmetic operations (`+`, `-`, `*`, `/`, `%`)
   - Comparison operations (`==`, `!=`, `<`, etc.)
   - Logical operations (`&&`, `||`, `!`)
   - Return statements
3. Write unit tests for code generation components
4. Write integration tests using `GlslTest` to verify generated LPIR

**Example:**

```glsl
int add(int x, int y) {
    return x + y;
}
```

Should generate LPIR like:

```rust
function %add(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = iadd v0, v1
    return v2
}
```

### Phase 7: Control Flow Code Generation

**Goal**: Generate LPIR for control flow

**Tasks:**

1. Generate LPIR for `if`/`else`:
   - Create blocks for true/false branches
   - Use `Br` instruction with condition
   - Merge blocks with phi nodes if needed
2. Generate LPIR for `for` loops:
   - Create blocks for init, condition, body, increment, exit
   - Use `Jump` and `Br` to connect blocks
3. Generate LPIR for `while` loops:
   - Create blocks for condition, body, exit
   - Use `Jump` and `Br` to connect blocks
4. Write unit tests for control flow code generation
5. Write integration tests using `GlslTest` to verify control flow LPIR

### Phase 8: Function Calls and Recursion

**Goal**: Generate LPIR for function calls

**Tasks:**

1. Generate `Call` instructions for function calls
2. Handle argument passing (value parameters)
3. Handle return value extraction
4. Write unit tests for function call code generation
5. Write integration tests using `GlslTest` for function calls and recursion

### Phase 9: Out/Inout Parameters

**Goal**: Support `out` and `inout` parameters

**Tasks:**

1. **Caller side**:
   - Allocate stack space for `out`/`inout` parameters
   - For `inout`: Store current value to stack
   - Pass address (as `I32`) to callee
   - After call: Load result from stack (for `out`/`inout`)
2. **Callee side**:
   - Receive address parameter (as `I32`)
   - For `inout`: Load initial value using `Load`
   - For `out`: Initialize to default if needed
   - Store result using `Store` before return
3. Update type checker to track parameter qualifiers
4. Write unit tests for out/inout parameter handling
5. Write integration tests using `GlslTest` to verify out/inout LPIR generation

**Example:**

```glsl
void swap(inout int x, inout int y) {
    int temp = x;
    x = y;
    y = temp;
}
```

### Phase 10: Integration Test Infrastructure

**Goal**: Build integration test helpers for verifying GLSL → LPIR compilation

**Tasks:**

1. Create `GlslTest` helper struct that:
   - Takes GLSL source code
   - Parses, type checks, and generates LPIR
   - Provides `assert_lpir()` method to verify generated LPIR matches expected string
2. Support testing multiple functions in a single GLSL source
3. Handle normalization of LPIR strings for comparison (whitespace, etc.)
4. Write comprehensive integration tests using the helper

**Test helper API:**

```rust
pub struct GlslTest {
    glsl_source: String,
    // ... internal state
}

impl GlslTest {
    pub fn new(glsl: &str) -> Self {
        // Parse GLSL, type check, generate LPIR
    }

    pub fn assert_lpir(&self, expected_lpir: &str) {
        // Compare generated LPIR (normalized) against expected
        // Format: function name -> LPIR string
    }
}
```

**Example usage:**

```rust
#[test]
fn test_simple_add() {
    let glsl = r#"
        int add(int x, int y) {
            return x + y;
        }
    "#;

    GlslTest::new(glsl)
        .assert_lpir("add", r#"
            function %add(i32, i32) -> i32 {
            block0(v0: i32, v1: i32):
                v2 = iadd v0, v1
                return v2
            }
        "#);
}
```

## References

### Parsing

- **glsl-parser**: `/Users/yona/dev/photomancer/glsl-parser`
  - Provides `TranslationUnit`, `FunctionDefinition`, `Statement`, `Expr` AST types
  - Handles GLSL syntax parsing
  - No type checking or code generation

### Type Checking Inspiration

- **glslang**: `/Users/yona/dev/photomancer/glslang`

  - Full GLSL type checker implementation
  - Symbol table management
  - Type promotion and conversion rules
  - Key files:
    - `glslang/MachineIndependent/ParseHelper.cpp`: Type checking logic
    - `glslang/MachineIndependent/intermediate.h`: Type system definitions

- **DirectXShaderCompiler**: `/Users/yona/dev/photomancer/DirectXShaderCompiler`
  - HLSL/GLSL type checking
  - Parameter qualifier handling (`in`, `out`, `inout`)
  - Key files:
    - `lib/HLSL/`: HLSL type checking (similar concepts to GLSL)

### Code Generation Inspiration

- **glslang**: SPIR-V code generation

  - `SPIRV/GlslangToSpv.cpp`: AST to SPIR-V translation
  - Control flow generation
  - SSA construction

- **DirectXShaderCompiler**: DXIL code generation
  - `lib/HLSL/`: HLSL to DXIL translation
  - Function call handling
  - Parameter passing conventions

### LPIR Documentation

- **LPIR README**: `crates/lpc-lpir/README.md`
- **FunctionBuilder API**: `crates/lpc-lpir/src/builder/`
- **LPIR Types**: `crates/lpc-lpir/src/types.rs`
- **LPIR Opcodes**: `crates/lpc-lpir/src/dfg/opcode.rs`

## Future Extensions

Once the initial implementation is complete, we can extend it to support:

1. **More Types**:

   - `float` and floating-point operations
   - Vector types (`vec2`, `vec3`, `vec4`)
   - Struct types
   - Arrays

2. **More Control Flow**:

   - `switch`/`case`
   - `break`/`continue`
   - `discard` (fragment shader)

3. **More Expressions**:

   - Ternary operator (`?:`)
   - Array indexing
   - Struct field access
   - Vector swizzling

4. **Shader-Specific Features**:

   - Uniforms and attributes
   - Built-in variables (`gl_Position`, etc.)
   - Samplers and textures

5. **Optimizations**:
   - Constant folding
   - Dead code elimination
   - Inlining

## Testing Strategy

### Unit Tests

Each module and phase should have comprehensive unit tests:

- **Parser tests**: Verify GLSL parsing extracts correct AST nodes
- **Type checker tests**: Verify type checking logic (type matching, error detection)
- **Symbol table tests**: Verify symbol resolution and scoping
- **Code generator tests**: Verify LPIR generation for individual constructs

**Example unit test structure:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function_definition() {
        // Test parsing extracts function correctly
    }

    #[test]
    fn test_type_check_binary_operator() {
        // Test type checking for +, -, *, etc.
    }

    #[test]
    fn test_generate_add_instruction() {
        // Test code generation for addition
    }
}
```

### Integration Tests

Integration tests use the `GlslTest` helper to verify the full compilation pipeline (GLSL → LPIR):

**File**: `tests/integration.rs`

**Test categories:**

1. **Simple functions**: Basic arithmetic, comparisons
2. **Control flow**: `if`/`else`, `for`, `while` loops
3. **Function calls**: Direct calls, recursion
4. **Out/inout parameters**: Parameter passing semantics
5. **Complex programs**: Multiple functions, nested control flow

**Example integration test:**

```rust
#[test]
fn test_add_function() {
    let glsl = r#"
        int add(int x, int y) {
            return x + y;
        }
    "#;

    GlslTest::new(glsl)
        .assert_lpir("add", r#"
            function %add(i32, i32) -> i32 {
            block0(v0: i32, v1: i32):
                v2 = iadd v0, v1
                return v2
            }
        "#);
}

#[test]
fn test_recursive_factorial() {
    let glsl = r#"
        int factorial(int n) {
            if (n <= 1) {
                return 1;
            } else {
                return n * factorial(n - 1);
            }
        }
    "#;

    GlslTest::new(glsl)
        .assert_lpir("factorial", r#"
            function %factorial(i32) -> i32 {
            block0(v0: i32):
                v1 = iconst 1
                v2 = icmp_le v0, v1
                br v2, block1, block2
            block1:
                v3 = iconst 1
                return v3
            block2:
                // ... recursive call handling ...
            }
        "#);
}
```

**Note**: End-to-end testing (GLSL → LPIR → RISC-V → emulator) will be added in a future phase once the backend is more complete.

## Notes

- **Bool representation**: GLSL `bool` maps to LPIR `U32` (0 = false, 1 = true)
- **Out/Inout parameters**: Implemented using stack-allocated memory and pointers (addresses as `I32`)
- **SSA construction**: Use `FunctionBuilder` to maintain SSA form automatically
- **Error handling**: Use `Result` types with descriptive error messages
- **Source locations**: Track source locations for better error messages (use LPIR's `RelSourceLoc`)
