# VCode Assembler/Disassembler for Backend3 Tests

## Overview

Implement text-based serialization (disassembler) and deserialization (assembler) for VCode to enable tests to specify expected output in a readable format. This will replace the current property-based assertions with exact text comparisons.

## Motivation

Current tests use property-based assertions (e.g., `assert!(vcode.insts.len() >= 1)`), which are vague and don't clearly show what VCode is expected. A text format will make tests:
- More readable and self-documenting
- Easier to debug (can see exact expected vs actual)
- More maintainable (clear expectations)

## Components

### 1. Disassembler (VCode → Text)

**Location**: `crates/lpc-codegen/src/isa/riscv32/backend3/vcode_format.rs`

**Implementation**:

- Implement `Display` trait for `Riscv32MachInst` to format instructions as text
  - Format: `add v0, v1, v2` or `addi v0, v1, 42` or `move v0, v1`
  - Use existing `VReg::Display` which formats as `v0`, `v1`, etc.
- Implement `Display` trait for `VCode<Riscv32MachInst>` to format entire VCode
  - Iterate through `block_order.lowered_order` to get blocks in order
  - For each block:
    - Show block header: `block0:` or `block0(v0, v1):` (with params)
    - Mark edge blocks: `edge block1 -> block2:`
    - Show instructions using `block_ranges` to get instruction indices
    - Show branch targets and arguments using `block_succs` and `branch_block_args`
  - Format example:
    ```
    vcode {
      entry: block0
      
      block0(v0, v1):
        v2 = addi v0, 42
        v3 = add v2, v1
        br block1(v3)
      
      block1(v4):
        return v4
      
      edge block1 -> block2:
        v5 = move v6
        br block2(v5)
    }
    ```

**Key Details**:

- Use `block_ranges` to map blocks to instruction ranges
- Use `block_params_range` and `block_params` for block parameters
- Use `block_succ_range` and `block_succs` for successors
- Use `branch_block_arg_range` and `branch_block_args` for branch arguments
- Distinguish `LoweredBlock::Orig` vs `LoweredBlock::Edge`
- **Note**: VCode doesn't contain function name or signature - only the lowered code structure. Function parameters appear as the entry block's parameters.

### 2. Assembler (Text → VCode)

**Location**: `crates/lpc-codegen/src/isa/riscv32/backend3/vcode_parser.rs`

**Implementation**:

- Parse VCode text format back into `VCode<Riscv32MachInst>`
- Use nom parser (similar to existing `asm_parser.rs`)
- Parse instructions: `add v0, v1, v2`, `addi v0, v1, 42`, `move v0, v1`, etc.
- Parse blocks with parameters: `block0(v0, v1):`
- Parse edge blocks: `edge block1 -> block2:`
- Parse branches: `br block1(v2)` or `brif v0, block1, block2`
- Build VCode using `VCodeBuilder`

**Key Details**:

- Parse VReg identifiers: `v0`, `v1`, etc.
- Parse immediates: decimal (`42`) and hex (`0x2a`)
- Handle block structure and control flow
- Reconstruct `BlockLoweringOrder` from parsed blocks

### 3. Test Helper Functions

**Location**: `crates/lpc-codegen/src/backend3/tests/vcode_test_helpers.rs` (new file)

**Implementation**:

- `assert_vcode_eq(actual: &VCode<Riscv32MachInst>, expected: &str)` - Compare actual VCode with expected text
- `parse_vcode(text: &str) -> VCode<Riscv32MachInst>` - Parse VCode from text (for constructing test cases)
- Normalize whitespace for comparison (similar to LPIR tests)

### 4. Update Existing Tests

**Location**: `crates/lpc-codegen/src/backend3/tests/lower_tests.rs`

**Changes**:

- Replace property-based assertions with text format comparisons
- Example transformation:
  ```rust
  // Before:
  assert!(vcode.insts.len() >= 1);
  
  // After:
  let expected = r#"
  vcode {
    entry: block0
    
    block0(v0, v1):
      v2 = add v0, v1
      return v2
  }
  "#;
  assert_vcode_eq(&vcode, expected);
  ```

## File Structure

```
crates/lpc-codegen/src/isa/riscv32/backend3/
  - inst.rs (existing)
  - vcode_format.rs (new - Display implementations)
  - vcode_parser.rs (new - parsing)
  - mod.rs (export new modules)

crates/lpc-codegen/src/backend3/tests/
  - vcode_test_helpers.rs (new - test utilities)
  - lower_tests.rs (update to use text format)
```

## Format Specification

### Instruction Format

- `add v0, v1, v2` - ADD instruction
- `addi v0, v1, 42` - ADDI with immediate
- `sub v0, v1, v2` - SUB instruction
- `lui v0, 0x12345` - LUI with upper immediate
- `lw v0, 4(v1)` - Load word
- `sw v1, 4(v0)` - Store word
- `move v0, v1` - Move instruction

### Block Format

- `block0:` - Block without parameters
- `block0(v0, v1):` - Block with parameters (entry block params represent function params)
- `edge block1 -> block2:` - Edge block

### Control Flow

- `br block1` - Unconditional branch
- `br block1(v2)` - Branch with argument
- `brif v0, block1, block2` - Conditional branch
- `return v0` - Return instruction

**Note**: VCode doesn't contain function name or signature - only the lowered code structure. Function parameters appear as the entry block's parameters.

## Implementation Order

1. **Phase 1**: Disassembler (Display implementations)
   - Implement `Display` for `Riscv32MachInst`
   - Implement `Display` for `VCode<Riscv32MachInst>`
   - Test with existing VCode

2. **Phase 2**: Test helpers and update tests
   - Create `vcode_test_helpers.rs` with comparison function
   - Update `lower_tests.rs` to use text format
   - Verify tests pass

3. **Phase 3**: Assembler (optional, for round-trip)
   - Implement parser for VCode text format
   - Add round-trip tests
   - Use for constructing test cases programmatically

## Considerations

- **Whitespace normalization**: Tests should be flexible with whitespace (trim lines, normalize newlines)
- **Instruction ordering**: Instructions within blocks should match the order in `insts` array
- **Block ordering**: Blocks should be displayed in `block_order.lowered_order` order
- **Edge blocks**: Need to distinguish edge blocks from original blocks
- **Constants**: May be shown inline in instructions (e.g., `addi v0, v1, 42`) or separately if needed
- **Metadata**: Focus on essential structure (blocks, instructions, params, branches). Skip internal metadata like operand ranges unless needed for debugging
- **Function information**: VCode doesn't have function name/signature - only entry block parameters represent function parameters

## Testing Strategy

- Start with disassembler only (Phase 1-2) - sufficient for test clarity
- Add assembler later (Phase 3) if round-trip testing is needed
- Use existing tests as validation - ensure text output matches expectations

## Example Test Transformation

**Before**:
```rust
#[test]
fn test_lower_iadd() {
    let input = r#"
    function %test(i32, i32) -> i32 {
    block0(v0: i32, v1: i32):
        v2 = iadd v0, v1
        return v2
    }
    "#;
    let func = parse_function(input.trim()).expect("Failed to parse function");
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    assert!(vcode.insts.len() >= 1);
}
```

**After**:
```rust
#[test]
fn test_lower_iadd() {
    let input = r#"
    function %test(i32, i32) -> i32 {
    block0(v0: i32, v1: i32):
        v2 = iadd v0, v1
        return v2
    }
    "#;
    let func = parse_function(input.trim()).expect("Failed to parse function");
    let backend = Riscv32LowerBackend;
    let abi = Callee { abi: Riscv32ABI };
    let vcode = lower_function(func, &backend, abi);
    
    let expected = r#"
    vcode {
      entry: block0
      
      block0(v0, v1):
        v2 = add v0, v1
        return v2
    }
    "#;
    assert_vcode_eq(&vcode, expected);
}
```

This makes it immediately clear what VCode is expected, making tests much more maintainable and debuggable.

