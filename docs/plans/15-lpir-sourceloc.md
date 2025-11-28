# Add Source Location Tracking to IR

## Overview

Add source location tracking to the IR following Cranelift's design. This will allow instructions to track their original source code positions, enabling better debugging and correlation between source code and generated machine code.

## Implementation Plan

### 1. Create Source Location Types

Create `crates/lpc-lpir/src/sourceloc.rs` with:
- `SourceLoc`: Opaque u32 wrapper (similar to Cranelift)
  - Default value: `!0` (all-ones bit pattern)
  - Methods: `new()`, `bits()`, `is_default()`, `Display` impl
- `RelSourceLoc`: Relative source location for efficiency
  - Stores offset relative to base source location
  - Methods: `new()`, `from_base_offset()`, `expand()`, `is_default()`, `Display` impl

### 2. Add Source Location Storage to Function

Modify `crates/lpc-lpir/src/function.rs`:
- Add `base_srcloc: Option<SourceLoc>` field to `Function`
- Add `srclocs: PrimaryMap<Inst, RelSourceLoc>` field to `Function`
- Add methods:
  - `base_srcloc() -> SourceLoc` - get base source location (default if not set)
  - `ensure_base_srcloc(srcloc: SourceLoc) -> SourceLoc` - set base if not already set
  - `set_srcloc(inst: Inst, srcloc: SourceLoc)` - set absolute source location for instruction
  - `srcloc(inst: Inst) -> SourceLoc` - get absolute source location for instruction

### 3. Update Module Exports

Modify `crates/lpc-lpir/src/lib.rs`:
- Add `mod sourceloc;`
- Export `SourceLoc` and `RelSourceLoc` types

### 4. Update Builder API (Optional)

Modify `crates/lpc-lpir/src/builder/traits.rs`:
- Add optional source location parameter to builder methods (can be added incrementally)
- Or add a separate method like `with_srcloc()` for setting source locations

### 5. Update Parser (Future Work)

The parser (`crates/lpc-lpir/src/parser/`) can be updated later to capture source positions and call `set_srcloc()` when creating instructions. This is not required for the initial implementation.

### 6. Update Backend (Future Work)

The backend (`crates/lpc-riscv32/src/backend/`) can be updated later to use source locations when emitting machine code for debugging/relocation purposes.

## Files to Modify

1. **New file**: `crates/lpc-lpir/src/sourceloc.rs` - Source location types
2. **Modify**: `crates/lpc-lpir/src/function.rs` - Add source location storage and methods
3. **Modify**: `crates/lpc-lpir/src/lib.rs` - Export new types

## Design Decisions

- **Use PrimaryMap instead of SecondaryMap**: Since we don't have SecondaryMap, use PrimaryMap with default values. Instructions without source locations will have `RelSourceLoc::default()`.
- **Follow Cranelift's design**: Use relative source locations for efficiency, storing offsets relative to a base source location per function.
- **Opaque u32**: SourceLoc is an opaque u32, allowing frontends to encode file/line/column information however they want.
- **Default value**: Use `!0` (all-ones) as the default/invalid source location, matching Cranelift.

## Testing

Add tests for:
- SourceLoc creation and default handling
- RelSourceLoc conversion and expansion
- Function source location storage and retrieval
- Default source locations for instructions without explicit locations

