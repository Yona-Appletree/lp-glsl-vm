# Test Utility Builder Pattern Plan

## Overview

Create a test utility that makes it very easy to write tests for the compiler infrastructure. The utility should use a builder pattern to compile IR functions to ELF, run them in the embive VM, and assert expectations.

## Goals

1. **Easy to use**: Write tests with minimal boilerplate
2. **Builder pattern**: Fluent API for composing tests
3. **Comprehensive**: Support various expectations (return values, memory state, panics, etc.)
4. **Clear errors**: Good error messages when tests fail
5. **Flexible**: Can test individual functions or full programs

## Design

### Basic Usage Example

```rust
use r5_test_util::FunctionTest;

#[test]
fn test_add() {
    // Build IR function
    let func = build_add_function(); // fn add(a: i32, b: i32) -> i32 { a + b }

    // Test it
    FunctionTest::new(func)
        .with_args(5, 10)
        .expect_return(15)
        .run();
}

#[test]
fn test_branch() {
    let func = build_max_function(); // fn max(a: i32, b: i32) -> i32 { if a > b { a } else { b } }

    FunctionTest::new(func)
        .with_args(10, 5)
        .expect_return(10)
        .run();

    FunctionTest::new(func)
        .with_args(5, 10)
        .expect_return(10)
        .run();
}
```

### Advanced Usage Examples

```rust
// Test with memory expectations
FunctionTest::new(func)
    .with_args(42)
    .expect_return(0)
    .expect_memory_at(0x80000000, 42u32.to_le_bytes())
    .run();

// Test panic handling
FunctionTest::new(func)
    .with_args(0)
    .expect_panic("division by zero")
    .run();

// Test multiple return values
FunctionTest::new(func)
    .with_args(5, 10)
    .expect_return_values(vec![15, 0]) // Multiple return values
    .run();

// Test with custom VM configuration
FunctionTest::new(func)
    .with_args(5, 10)
    .vm_ram_size(8 * 1024 * 1024) // 8MB RAM
    .expect_return(15)
    .run();
```

## Implementation Plan

### Phase 1: Basic Builder Structure

**Crate**: `r5-test-util` (or add to existing crate)

**Structure:**

```rust
pub struct FunctionTest {
    func: lpc_lpir::Function,
    args: Vec<i32>,
    vm_config: VmConfig,
    expectations: Vec<Expectation>,
}

pub struct VmConfig {
    ram_size: usize,
    timeout_ms: Option<u64>,
}

pub enum Expectation {
    ReturnValue(i32),
    ReturnValues(Vec<i32>),
    Memory { address: u32, value: Vec<u8> },
    Panic { message: Option<String> },
    NoPanic,
    // Future: RegisterState, StackState, etc.
}
```

**Builder Methods:**

- `FunctionTest::new(func)` - Create new test
- `.with_args(...)` - Set function arguments (variadic or Vec)
- `.expect_return(value)` - Expect single return value
- `.expect_return_values(values)` - Expect multiple return values
- `.expect_panic(message)` - Expect panic with optional message
- `.expect_no_panic()` - Expect no panic
- `.expect_memory_at(address, value)` - Expect memory value
- `.vm_ram_size(size)` - Configure VM RAM size
- `.vm_timeout_ms(ms)` - Set timeout
- `.run()` - Execute test and assert expectations

**Files to create:**

- `crates/r5-test-util/Cargo.toml`
- `crates/r5-test-util/src/lib.rs`
- `crates/r5-test-util/src/test_builder.rs`
- `crates/r5-test-util/src/expectations.rs`
- `crates/r5-test-util/src/vm_runner.rs`

### Phase 2: Core Implementation

#### 2.1 Function Test Builder

**Tasks:**

- [ ] Implement `FunctionTest` struct
- [ ] Implement builder methods
- [ ] Store function, args, and expectations
- [ ] Validate inputs

**Implementation:**

```rust
impl FunctionTest {
    pub fn new(func: lpc_lpir::Function) -> Self {
        Self {
            func,
            args: Vec::new(),
            vm_config: VmConfig::default(),
            expectations: Vec::new(),
        }
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = i32>) -> Self {
        self.args = args.into_iter().collect();
        self
    }

    pub fn expect_return(mut self, value: i32) -> Self {
        self.expectations.push(Expectation::ReturnValue(value));
        self
    }

    // ... other builder methods

    pub fn run(self) {
        // Compile, run, assert
    }
}
```

#### 2.2 Compilation Pipeline

**Tasks:**

- [ ] Compile IR function to RISC-V code
- [ ] Generate ELF file
- [ ] Create minimal program wrapper if needed

**Implementation:**

- Use `r5_target_riscv32::compile_function()` to compile IR
- Use `r5_target_riscv32::generate_elf()` to create ELF
- May need to wrap function in a minimal program that:
  - Sets up arguments in `a0`-`a7`
  - Calls the function
  - Stores return value somewhere accessible
  - Exits via syscall

**Files:**

- `crates/r5-test-util/src/compiler.rs`

#### 2.3 VM Runner

**Tasks:**

- [ ] Load ELF into VM
- [ ] Set up function arguments
- [ ] Run VM
- [ ] Extract results
- [ ] Handle timeouts

**Implementation:**

- Use `R5Vm` from `lp-glsl-vm`
- Load compiled ELF
- Set up arguments in memory or registers (depends on calling convention)
- Run VM with timeout
- Extract return value from `a0` or memory
- Handle panics

**Files:**

- `crates/r5-test-util/src/vm_runner.rs`

#### 2.4 Expectation Assertions

**Tasks:**

- [ ] Implement expectation checking
- [ ] Provide clear error messages
- [ ] Support multiple expectations

**Implementation:**

```rust
impl Expectation {
    fn check(&self, result: &TestResult) -> Result<(), String> {
        match self {
            Expectation::ReturnValue(expected) => {
                if result.return_value != Some(*expected) {
                    Err(format!("Expected return value {}, got {:?}",
                        expected, result.return_value))
                } else {
                    Ok(())
                }
            }
            // ... other expectations
        }
    }
}
```

**Files:**

- `crates/r5-test-util/src/expectations.rs`

### Phase 3: Program Wrapper Generation

**Problem:** We need to wrap a function in a minimal program that can be executed.

**Solution:** Generate a minimal program that:

1. Sets up arguments (`a0`-`a7` from memory or constants)
2. Calls the function
3. Stores return value to a known location
4. Exits via syscall

**Tasks:**

- [ ] Generate program entry point
- [ ] Set up function arguments
- [ ] Call function
- [ ] Store return value
- [ ] Exit via syscall

**Implementation:**

- Create a wrapper function in IR that:
  - Takes no parameters
  - Calls the test function with provided arguments
  - Stores return value to memory
  - Calls syscall to exit
- Or generate RISC-V code directly for the wrapper

**Files:**

- `crates/r5-test-util/src/program_wrapper.rs`

### Phase 4: Enhanced Expectations

**Tasks:**

- [ ] Memory state expectations
- [ ] Register state expectations (if accessible)
- [ ] Panic message matching
- [ ] Custom assertion callbacks

**Implementation:**

```rust
// Memory expectations
.expect_memory_at(0x80000000, &[0x42, 0x00, 0x00, 0x00])
.expect_memory_range(0x80000000..0x80000100, |data| {
    assert_eq!(data[0], 42);
})

// Panic expectations
.expect_panic("division by zero")
.expect_panic_contains("error")

// Custom assertions
.expect_custom(|result| {
    assert!(result.return_value.unwrap() > 0);
    Ok(())
})
```

**Files:**

- `crates/r5-test-util/src/expectations.rs` (extend)

### Phase 5: Convenience Helpers

**Tasks:**

- [ ] Helper to build common test functions
- [ ] Macro for easier test writing
- [ ] Test fixtures

**Implementation:**

```rust
// Helper to build simple functions
pub fn build_add_function() -> lpc_lpir::Function {
    // Build fn add(a: i32, b: i32) -> i32 { a + b }
}

// Macro for tests
#[r5_test]
fn test_add(func: lpc_lpir::Function) {
    FunctionTest::new(func)
        .with_args(5, 10)
        .expect_return(15)
        .run();
}
```

**Files:**

- `crates/r5-test-util/src/helpers.rs`
- `crates/r5-test-util/src/macros.rs` (if using proc macros)

## File Structure

```
crates/r5-test-util/
├── Cargo.toml
└── src/
    ├── lib.rs              # Main exports
    ├── test_builder.rs     # FunctionTest builder
    ├── expectations.rs     # Expectation types and checking
    ├── vm_runner.rs        # VM execution logic
    ├── compiler.rs         # Compilation pipeline
    ├── program_wrapper.rs # Program wrapper generation
    ├── helpers.rs          # Convenience helpers
    └── macros.rs           # Test macros (optional)
```

## Dependencies

```toml
[dependencies]
lpc-lpir = { path = "../lpc-lpir" }
r5-target-riscv32 = { path = "../r5-target-riscv32" }
lp-glsl-vm = { path = "../lp-glsl-vm" }
embive = { path = "/path/to/embive", default-features = false, features = ["transpiler"] }
```

## Test Examples

### Example 1: Simple Arithmetic

```rust
#[test]
fn test_add() {
    use r5_builder::FunctionBuilder;
    use lpc_lpir::{Signature, Type};
    use r5_test_util::FunctionTest;

    let sig = Signature::new(vec![Type::I32, Type::I32], vec![Type::I32]);
    let mut builder = FunctionBuilder::new(sig);
    let block_idx = builder.create_block();

    let a = builder.new_value();
    let b = builder.new_value();
    let result = builder.new_value();

    {
        let mut block_builder = builder.block_builder(block_idx);
        block_builder.iadd(result, a, b);
        block_builder.return_(&vec![result]);
    }

    let func = builder.finish();

    FunctionTest::new(func)
        .with_args(5, 10)
        .expect_return(15)
        .run();
}
```

### Example 2: With Memory

```rust
#[test]
fn test_store_and_load() {
    // Function that stores a value and loads it back
    let func = build_store_load_function();

    FunctionTest::new(func)
        .with_args(42)
        .expect_return(42)
        .expect_memory_at(0x80000000, 42u32.to_le_bytes())
        .run();
}
```

### Example 3: Multiple Test Cases

```rust
#[test]
fn test_max_multiple_cases() {
    let func = build_max_function();

    let test_cases = vec![
        (10, 5, 10),
        (5, 10, 10),
        (0, 0, 0),
        (-5, -10, -5),
    ];

    for (a, b, expected) in test_cases {
        FunctionTest::new(func.clone())
            .with_args(a, b)
            .expect_return(expected)
            .run();
    }
}
```

### Example 4: Panic Handling

```rust
#[test]
fn test_division_by_zero() {
    let func = build_divide_function();

    FunctionTest::new(func)
        .with_args(10, 0)
        .expect_panic("division by zero")
        .run();
}
```

## Error Messages

Good error messages are critical. Examples:

```
Test failed: Expected return value 15, got Some(10)
  Function: add
  Arguments: [5, 10]
  Expected: 15
  Actual: 10
```

```
Test failed: Expected panic with message containing "division by zero", but no panic occurred
  Function: divide
  Arguments: [10, 0]
```

```
Test failed: Memory mismatch at address 0x80000000
  Expected: [0x2a, 0x00, 0x00, 0x00]
  Actual:   [0x00, 0x00, 0x00, 0x00]
```

## Implementation Details

### Program Wrapper Strategy

**Option 1: IR Wrapper**

- Build wrapper function in IR
- Wrapper calls test function
- Compile wrapper + test function together
- Pros: Uses existing IR infrastructure
- Cons: More complex, need to link functions

**Option 2: Direct RISC-V Wrapper**

- Generate RISC-V code directly for wrapper
- Set up arguments, call function, store result
- Pros: Simpler, more control
- Cons: Need to generate RISC-V code manually

**Option 3: Minimal Runtime**

- Create minimal runtime that handles argument setup
- Test function is called directly
- Pros: Closest to real execution
- Cons: Need runtime support

**Recommendation:** Start with Option 2 (Direct RISC-V wrapper) for simplicity, can migrate to Option 1 later if needed.

### Argument Passing

For now, arguments can be passed via:

1. Constants in the wrapper (for simple cases)
2. Memory locations (for complex cases)
3. Direct register setup (once calling convention is implemented)

### Return Value Extraction

Return value can be extracted via:

1. `vm.last_result()` - if function uses syscall to return
2. Memory location - if function stores result to known address
3. Register `a0` - once we can inspect VM state

**Recommendation:** Use syscall-based return for now (like current `jit_test`), migrate to register-based later.

## Success Criteria

Phase 1 complete when:

- ✅ Can write a simple test with builder pattern
- ✅ Test compiles IR to ELF
- ✅ Test runs in VM
- ✅ Can assert return value

Phase 2 complete when:

- ✅ Can test functions with multiple arguments
- ✅ Can test functions with branches/loops
- ✅ Can test memory operations
- ✅ Error messages are clear

Phase 3 complete when:

- ✅ Can test panic scenarios
- ✅ Can test memory state
- ✅ Can test multiple cases easily
- ✅ Tests are easy to write and maintain

## Estimated Effort

- Phase 1: 1-2 days (basic builder and compilation)
- Phase 2: 1-2 days (VM runner and expectations)
- Phase 3: 1 day (program wrapper)
- Phase 4: 1 day (enhanced expectations)
- Phase 5: 1 day (convenience helpers)

Total: ~5-7 days for complete test utility.

## Future Enhancements

- Test multiple functions in sequence
- Test function calls (once calling convention is implemented)
- Performance benchmarking
- Code coverage analysis
- Property-based testing support
- Integration with existing test frameworks





