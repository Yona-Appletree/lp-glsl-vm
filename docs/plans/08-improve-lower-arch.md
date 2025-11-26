# Lowering Code Structure Improvements

## Overview

Improve the structure and maintainability of the lowering code (`crates/r5-target-riscv32/src/lower`) to reduce bugs related to stack and register allocation. Focus on type safety, better abstractions, debug tooling, and test organization.

## 1. Add Type Safety for Offsets

**Problem**: Byte offsets and instruction offsets are both `usize`, leading to confusion and bugs (e.g., forgetting to multiply by 4).

**Solution**: Create wrapper types to distinguish between:

- `InstOffset`: Instruction index (0, 1, 2, ...) - wraps `usize`
- `ByteOffset`: Byte offset - wraps `i32` (signed, since stack offsets are signed in RISC-V)

**Files to modify**:

- `crates/r5-target-riscv32/src/lower/types.rs` - Add new types with trait implementations
- `crates/r5-target-riscv32/src/lower/function.rs` - Use `InstOffset` for relocations
- `crates/r5-target-riscv32/src/lower/call.rs` - Use `InstOffset` for `instruction_count()`
- `crates/r5-target-riscv32/src/lower/branch.rs` - Use `InstOffset` for relocations
- `crates/r5-target-riscv32/src/lower/return_.rs` - Use `InstOffset` for relocations
- `crates/r5-target-riscv32/src/emit.rs` - Methods return `InstOffset` (e.g., `instruction_count()`)
- `crates/r5-target-riscv32/src/frame.rs` - Methods return `ByteOffset` (e.g., `spill_slot_offset()`)
- `crates/r5-target-riscv32/src/lib.rs` - Update offset calculations (lines 390-678)

**Implementation**:

- Create `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]` wrapper types
- Use `#[repr(transparent)]` to ensure no runtime overhead
- Implement trait conversions:
  - `From<InstOffset> for ByteOffset` - multiply by 4
  - `From<usize> for InstOffset` - for backward compatibility during migration
  - `From<i32> for ByteOffset` - for direct stack offset values
- Update all offset calculations to use these types
- `CodeBuffer::instruction_count()` returns `InstOffset`
- `CodeBuffer::len()` returns `usize` (unsigned byte count, for compatibility)

**Migration Strategy**:

- Phase 1: Add types, make them optional initially (use `Option<InstOffset>` where needed)
- Phase 2: Migrate one module at a time (start with `emit.rs`, then `lower/types.rs`, then others)
- Phase 3: Remove old `usize` usage and add type conversion tests

**Documentation**:

- Add doc comments explaining why `InstOffset` vs `ByteOffset` distinction matters
- Document when to use each type (instruction indices vs byte offsets)
- Document conversion patterns and checked arithmetic where overflow is possible

## 2. Centralize Frame Layout Logic

**Problem**: Logic about where to store/spill things is scattered across multiple files.

**Solution**: Move more decision logic into `FrameLayout` or create a new `FrameAccess` helper module.

**Files to modify**:

- `crates/r5-target-riscv32/src/frame.rs` - Add helper methods:
  - `store_value_location(value, allocation) -> Option<StorageLocation>`
  - `load_value_location(value, allocation) -> StorageLocation`
  - Methods to compute actual stack offsets accounting for SP state (before/after prologue)
- `crates/r5-target-riscv32/src/lower/call.rs` - Use centralized helpers
- `crates/r5-target-riscv32/src/lower/prologue.rs` - Use centralized helpers
- `crates/r5-target-riscv32/src/lower/helpers.rs` - Refactor to use centralized helpers

**New types**:

```rust
enum StorageLocation {
    Register(Gpr),
    SpillSlot { slot: u32, offset: ByteOffset },
    IncomingStackArg { index: usize, offset: ByteOffset },
    OutgoingStackArg { index: usize, offset: ByteOffset },
    CalleeSaved { reg: Gpr, offset: ByteOffset },
}
```

**Implementation details**:

- `store_value_location()` and `load_value_location()` centralize the decision logic currently scattered in `load_value_into_reg()` and call sites
- These methods return `StorageLocation` which can then be used to generate appropriate load/store instructions
- `load_value_into_reg()` can be refactored to use `load_value_location()` internally
- Add method `incoming_arg_offset_after_prologue(&self, arg_index: usize) -> ByteOffset` to compute offsets accounting for SP adjustment after prologue
- Update `FrameLayout` methods to return `ByteOffset` instead of `i32`:
  - `spill_slot_offset()` -> `ByteOffset`
  - `callee_saved_offset()` -> `Option<ByteOffset>`
  - `incoming_arg_offset()` -> `Option<ByteOffset>`
  - `outgoing_arg_offset()` -> `Option<ByteOffset>`
  - `return_value_offset()` -> `Option<ByteOffset>`

## 3. Add Debug Logging Infrastructure

**Problem**: No way to trace frame layout decisions and offset calculations during development.

**Solution**: Add feature-gated debug logging using a lightweight crate (e.g., `log` with `std` feature for tests).

**Files to create/modify**:

- `crates/r5-target-riscv32/src/lower/debug.rs` - New debug logging module
- `crates/r5-target-riscv32/Cargo.toml` - Add `log` dependency (optional, std feature)
- All lowering files - Add debug log calls at key decision points

**Implementation**:

- Use `#[cfg(feature = "debug-lowering")]` for debug logging (more specific than just "debug")
- In tests, enable via feature flag: `cargo test --features debug-lowering`
- Verify `log` crate is truly no_std compatible when feature is disabled (may need custom macro)
- Alternative: Use compile-time flag `const DEBUG: bool = cfg!(feature = "debug-lowering");` with custom macros
- Log: frame layout computation, offset calculations, spill/reload decisions, register assignments
- Consider structured logging format: `debug!("frame_layout", setup_area, clobber, spills, total)`
- Use `log::debug!()` macro or custom `debug_lowering!()` macro that compiles to nothing when disabled

**Example log output**:

```
[DEBUG] FrameLayout::compute: setup_area=8, clobber=16, spills=32, total=56
[DEBUG] spill_slot_offset(slot=2): base=-24, offset=-32
[DEBUG] load_value_into_reg: v0 -> a0 (from register t0)
```

## 4. Move Tests to Unit Tests

**Problem**: Integration tests in `tests/` are harder to test specific module behavior.

**Solution**: Move relevant tests from `tests/` to unit tests in respective modules.

**Files to reorganize**:

- `crates/r5-target-riscv32/tests/caller_saved.rs` - Extract frame layout tests to `frame.rs`
- `crates/r5-target-riscv32/tests/stack_args.rs` - Extract to `frame.rs` and `call.rs`
- `crates/r5-target-riscv32/tests/stack_tests.rs` - Extract to `frame.rs`
- Keep integration tests in `tests/` for end-to-end scenarios

**Strategy**:

- Unit tests: Test individual functions/methods in isolation
- Integration tests: Test full compilation and execution flows
- Use `#[cfg(test)]` modules in each source file (can access private functions/methods)

**Tests to keep as integration tests**:

- End-to-end compilation and execution flows
- Tests that require full module compilation
- Smoke tests that verify overall system behavior
- Document which tests stay in `tests/` and why (e.g., "requires full module context")

## 5. Align with Cranelift Structure

**Reference**: Local Cranelift reference at `wasmtime/cranelift/codegen/src/isa/riscv64` (adjust path as needed)

**Note**: This review should happen early (after type safety) to inform other changes, as it may reveal issues requiring updates to sections 1-4.

**Areas to review**:

- Frame layout computation order
- Offset calculation patterns
- Spill/reload insertion points
- Register allocation integration

**Files to review**:

- `crates/r5-target-riscv32/src/frame.rs` - Verify layout matches Cranelift
- `crates/r5-target-riscv32/src/lower/prologue.rs` - Verify prologue sequence
- `crates/r5-target-riscv32/src/lower/epilogue.rs` - Verify epilogue sequence

**Documentation**:

- Document specific differences found between our implementation and Cranelift
- Note whether differences need to be addressed or are intentional
- Update other sections if Cranelift review reveals necessary changes

## Implementation Order

1. **Type safety** (highest impact, prevents bugs)
2. **Cranelift alignment review** (early verification to inform other changes)
3. **Debug logging** (helps with debugging other changes)
4. **Centralize frame logic** (reduces duplication)
5. **Move tests** (improves maintainability)

## Testing Strategy

- Run existing tests after each change
- Add new unit tests for type conversions (`InstOffset` <-> `ByteOffset`, `From` trait implementations)
- Add tests for checked arithmetic where overflow is possible
- Enable debug logging in tests to verify output: `cargo test --features debug-lowering`
- Verify no performance regression (debug logging disabled in production, wrapper types are zero-cost with `#[repr(transparent)]`)
- Test migration path: verify backward compatibility during phased migration
