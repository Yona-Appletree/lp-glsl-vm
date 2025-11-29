# lpc-glsl Code Review and Fixes

## Overview

The `lpc-glsl` crate provides a GLSL frontend that parses, type-checks, and generates LPIR. The tests validate generated LPIR by comparing string representations. This review identifies several critical issues that need to be addressed.

## Critical Issues Found

### 1. SSA Form Violations

**Problem**: Variable reassignments break SSA (Single Static Assignment) form. In `codegen.rs`, assignments directly update the variables map:

```250:256:crates/lpc-glsl/src/codegen.rs
            Expr::Assignment(lhs, _op, rhs) => {
                let rhs_value = self.generate_expr(rhs)?;
                // For now, we only support variable assignments
                if let Expr::Variable(ident) = lhs.as_ref() {
                    let name = ident.0.clone();
                    self.variables.insert(name, rhs_value);
                    Ok(rhs_value)
```

This violates SSA because the same variable name maps to different values. In loops, variables modified in the body need phi nodes to merge values from different paths.

**Impact**: Generated LPIR may not be valid SSA, and variable updates in loops are incorrect.

### 2. Loop Variable Tracking Issues

**Problem**: In while loops, variables modified in the loop body aren't properly tracked. The implementation regenerates the condition in a new block but doesn't account for variable updates:

```486:534:crates/lpc-glsl/src/codegen.rs
            IterationStatement::While(cond, body) => {
                // Create blocks: condition, body, exit
                let cond_block = self.current_block.expect("No current block");
                let body_block = self.builder.create_block();
                let exit_block = self.builder.create_block();

                // Generate condition first (before getting block builder)
                let cond_value = match cond {
                    glsl::syntax::Condition::Expr(expr) => self.generate_expr(expr)?,
```

The condition is regenerated in a new block, but variables updated in the body aren't merged back via phi nodes.

**Impact**: Tests like `test_while_with_variable` show incorrect LPIR where variable updates aren't properly tracked.

### 3. Test Correctness Issues

**Problem**: Some expected LPIR outputs have incorrect types:

- `test_logical_and` and `test_logical_or` in `expression_tests.rs` show function signatures with `i32` parameters but should be `u32` (bool maps to u32)
- Some tests show incorrect variable value tracking in loops

**Files affected**:
- `crates/lpc-glsl/tests/expression_tests.rs` (lines 248-256, 270-278)

### 4. Missing LPIR Validation

**Problem**: Tests only compare string representations but don't validate:
- Structural correctness (proper SSA form, block termination)
- Type correctness (parameter/return types match signature)
- Control flow correctness (all blocks reachable, proper phi nodes)

**Impact**: Tests may pass even if generated LPIR is invalid or incorrect.

**Current state**: The `GlslTest` helper in `crates/lpc-glsl/tests/glsl_test.rs` only compares string output:

```61:91:crates/lpc-glsl/tests/glsl_test.rs
    pub fn assert_lpir(&self, function_name: &str, expected_lpir: &str) {
        // Find function by name
        let (_, func) = self
            .functions
            .iter()
            .find(|(name, _)| name == function_name)
            .unwrap_or_else(|| {
                panic!(
                    "Function '{}' not found. Available functions: {:?}",
                    function_name,
                    self.functions.iter().map(|(n, _)| n).collect::<Vec<_>>()
                )
            });

        // Format Function using Display trait
        let actual_lpir = format!("{}", func);

        // Normalize both actual and expected
        let normalized_actual = Self::normalize_lpir(&actual_lpir);
        let normalized_expected = Self::normalize_lpir(expected_lpir);

        // Compare line-by-line
        if normalized_actual != normalized_expected {
            panic!(
                "LPIR mismatch for function '{}':\n\nExpected:\n{}\n\nActual:\n{}\n",
                function_name,
                normalized_expected.join("\n"),
                normalized_actual.join("\n")
            );
        }
    }
```

**Solution**: Use `lpc_lpir::verifier::verify()` to validate the generated Function before comparing strings.

### 5. Variable Scoping Issues

**Problem**: Compound statement handling restores the old variables map:

```102:116:crates/lpc-glsl/src/codegen.rs
    fn generate_compound_statement(&mut self, compound: &CompoundStatement) -> GlslResult<()> {
        // Push new scope for variables
        let old_variables = self.variables.clone();

        // Generate each statement
        for stmt in &compound.statement_list {
            self.generate_statement(stmt)?;
        }

        // Restore previous scope (pop variables that were declared in this scope)
        // For simplicity, we'll keep all variables (shadowing is handled by lookup)
        // In a more sophisticated implementation, we'd track scope depth
        self.variables = old_variables;

        Ok(())
    }
```

This doesn't properly handle variable updates or shadowing - if a variable is updated in an inner scope, the update is lost when the scope is popped.

## Implementation Plan

### Phase 1: Add LPIR Validation to Test Utility

**Priority**: HIGH - This will catch invalid LPIR immediately

1. **Update `GlslTest::new()`** to validate each generated function:
   - Call `lpc_lpir::verifier::verify()` on each function after generation
   - Panic with detailed error messages if validation fails
   - This ensures all generated LPIR is structurally valid

2. **Update `GlslTest::assert_lpir()`** to validate before comparing:
   - Validate the function before string comparison
   - Include validation errors in panic messages if validation fails
   - This ensures tests fail fast on invalid LPIR

3. **Add helper method `GlslTest::validate_function()`**:
   - Wrapper around `verify()` that formats errors nicely
   - Can be called explicitly in tests that want to check validation separately

**Files to modify**:
- `crates/lpc-glsl/tests/glsl_test.rs`

**Example implementation**:
```rust
use lpc_lpir::verifier::{verify, VerifierError};

impl GlslTest {
    /// Validate that a function is well-formed LPIR.
    ///
    /// # Panics
    ///
    /// Panics if validation fails, with detailed error messages.
    fn validate_function(&self, function_name: &str) {
        let func = self.get_function(function_name)
            .unwrap_or_else(|| panic!("Function '{}' not found", function_name));
        
        if let Err(errors) = verify(func, None) {
            let error_msgs: Vec<String> = errors.iter()
                .map(|e| {
                    if let Some(loc) = &e.location {
                        format!("  {}: {}", loc, e.message)
                    } else {
                        format!("  {}", e.message)
                    }
                })
                .collect();
            panic!(
                "LPIR validation failed for function '{}':\n{}",
                function_name,
                error_msgs.join("\n")
            );
        }
    }
    
    pub fn assert_lpir(&self, function_name: &str, expected_lpir: &str) {
        // Validate first
        self.validate_function(function_name);
        
        // Then compare strings (existing code)
        // ...
    }
}
```

### Phase 2: Fix SSA Form Violations

**Priority**: HIGH - Core correctness issue

1. **Track variable versions**: Instead of mapping variable names to values, track versions (SSA values) properly
2. **Implement phi node generation**: For variables modified in loops, generate phi nodes at loop headers
3. **Fix assignment handling**: Assignments should create new SSA values, not update the map directly

**Approach**:
- Variables map should track the current SSA value for each variable name
- When entering a loop, identify variables that will be modified
- At loop header, create phi nodes for modified variables
- In loop body, assignments create new SSA values
- After loop, use phi node results

### Phase 3: Fix Loop Variable Tracking

**Priority**: HIGH - Affects correctness of loop codegen

1. **Proper loop structure**: Use a single condition block that loops back, with phi nodes for variables modified in the loop
2. **Variable state tracking**: Track which variables are modified in loop bodies and generate appropriate phi nodes
3. **Update while/for/do-while implementations**: Ensure all loop types properly handle variable updates

**Approach for while loops**:
```
entry -> cond_block (with phi nodes for modified vars)
cond_block -> [body_block | exit_block]
body_block -> cond_block (with updated values)
```

### Phase 4: Fix Test Correctness

**Priority**: MEDIUM - Tests should have correct expected outputs

1. **Fix type mismatches**: Update expected LPIR in `expression_tests.rs` to use correct types (`u32` for bool parameters)
2. **Regenerate expected outputs**: After fixing codegen, regenerate all expected LPIR outputs
3. **Add validation helpers**: Create helper functions to validate LPIR structure

**Files to modify**:
- `crates/lpc-glsl/tests/expression_tests.rs` (fix bool parameter types)

### Phase 5: Improve Variable Scoping

**Priority**: MEDIUM - Affects correctness but less critical than SSA

1. **Proper scope tracking**: Implement proper scope depth tracking
2. **Variable shadowing**: Handle variable shadowing correctly
3. **Scope restoration**: Only remove variables declared in the current scope when popping

**Approach**:
- Track scope depth
- When declaring a variable, check if it shadows an outer scope variable
- When popping scope, only remove variables declared in that scope
- Variable updates should update the innermost scope where the variable is visible

## Files to Modify

1. `crates/lpc-glsl/src/codegen.rs` - Fix SSA violations, loop variable tracking, scoping
2. `crates/lpc-glsl/tests/glsl_test.rs` - Add LPIR validation
3. `crates/lpc-glsl/tests/expression_tests.rs` - Fix type mismatches in expected outputs
4. Potentially all test files - Regenerate expected outputs after fixes

## Testing Strategy

1. **Add validation first**: Update test utility to validate LPIR - this will immediately catch issues
2. **Run existing tests**: Identify which tests fail due to invalid LPIR
3. **Fix codegen issues**: Address SSA violations and loop variable tracking
4. **Regenerate expected outputs**: Update expected LPIR strings after fixes
5. **Add validation tests**: Ensure all tests pass with proper validation
6. **Verify end-to-end**: Run full test suite to ensure everything works

## Validation Checks

The LPIR verifier (`lpc_lpir::verifier::verify`) checks:

1. **Format validation**: Instruction format correctness
2. **Entity validation**: Values, blocks, instructions are valid
3. **CFG validation**: Control flow graph consistency
4. **SSA validation**: Single Static Assignment properties
5. **Dominance validation**: Value usage is dominated by definition
6. **Type validation**: Instruction types are correct
7. **Terminator validation**: All blocks have proper terminators

All of these checks should pass for generated LPIR.

## Expected Outcomes

After implementing these fixes:

1. **All generated LPIR is valid**: Validation will catch structural issues immediately
2. **Proper SSA form**: Variables follow SSA rules with phi nodes where needed
3. **Correct loop codegen**: Loops properly track variable updates
4. **Accurate tests**: Expected outputs match actual (correct) LPIR
5. **Better error messages**: Validation errors provide clear feedback on what's wrong

