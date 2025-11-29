# Backend3: Deferred Features

This document tracks features and optimizations that are **not** part of the initial backend3 implementation but may be added later.

## Advanced Optimizations

### Branch Optimization

- **Branch Threading**: Eliminate empty blocks by redirecting labels through unconditional jumps
  - When a label is bound to an unconditional jump, redirect all references to the jump's target
  - Creates "label aliases" that effectively remove empty blocks
  - Requires tracking which labels are bound to which offsets
- **Empty Block Elimination**: Remove blocks that only contain unconditional branches
- **Advanced Fallthrough Optimization**: Reorder blocks to maximize fallthrough opportunities
- **Branch Inversion**: Optimize conditional branches by inverting conditions when beneficial
  - Track "latest branches" (contiguous branches at buffer tail)
  - When conditional + unconditional pair is detected, invert condition if it improves fallthrough
  - Requires tracking both normal and inverted forms of conditional branches
- **Unnecessary Jump Elimination**: Remove jumps to immediately following blocks
  - Detect when branch target is bound to fallthrough location
  - Remove branch instruction entirely
- **Latest Branches Tracking**: Track contiguous branches at buffer tail for optimization
  - Enables branch editing by truncating buffer
  - Cleared when non-branch code is emitted

### Block Layout Optimization

- **Profile-Guided Optimization**: Use execution profiles to identify hot/cold blocks
- **Advanced Cold Block Sinking**: More sophisticated cold block placement
- **Block Reordering**: Optimize block order based on branch probabilities

## Code Generation Features

### Out-of-Range Branch Handling

- **Veneer/Island Insertion**: Handle branches that exceed ±4KB range
  - **Islands**: Code chunks inserted in the middle of emission to hold veneers
  - **Veneers**: Trampoline instructions that extend branch range (e.g., conditional branch → unconditional jump)
  - **Deadline Tracking**: Track when forward branches approach range limits and insert islands proactively
  - **Current Limitation**: Panic if branch out of range (assumes functions < 4KB)
- **Long-Range Jump Support**: Support for functions > 4KB
- **Deadline Tracking**: Track when branches go out of range and insert islands
  - Monitor forward branch references as code is emitted
  - Calculate worst-case size of upcoming blocks
  - Insert islands when deadline is reached (before branch goes out of range)
  - Process fixups during island emission

### Constant Pool

- **Large Constant Storage**: Store large constants in data section
- **PC-Relative Constant Loading**: Load constants via PC-relative addressing
- **Constant Deduplication**: Share constants across functions

### Frame Pointer

- **Optional Frame Pointer**: Use frame pointer for easier debugging
- **Frame Pointer Optimization**: Only use FP when needed (variable-sized allocations, etc.)

## Debugging and Diagnostics

### Debug Information

- **Debug Tags**: Emit debug metadata for debugging tools
  - Pre-instruction and post-instruction debug tag placement
  - Debug tag pooling for efficient storage
- **Value Label Ranges**: Track where values are live in machine code
  - Map virtual registers to machine code offsets
  - Track live ranges for debuggers
  - Requires instruction offset tracking during emission
- **DWARF Debug Info**: Generate DWARF debug sections for debugging tools
- **CFG Metadata**: Track block offsets and edges in final machine code
  - `bb_offsets`: Code offset of each block start
  - `bb_edges`: Final CFG edges in terms of code offsets
  - Useful for debugging and analysis tools

### Unwind Information

- **Exception Handling**: Generate unwind info for exception handling
- **Stack Unwinding**: Support for stack unwinding (SystemV, Windows, etc.)

## Advanced Register Allocation

### Register Allocation Optimizations

- **Rematerialization**: Recompute values instead of spilling
- **Coalescing**: Merge related virtual registers
- **Live Range Splitting**: Split live ranges for better allocation

### Clobber Computation Optimization

- **Dead Write Elimination**: The current algorithm saves all callee-saved registers that are written to, even if the value is dead (never read). Could optimize by:
  - Only saving callee-saved registers that are live across calls
  - Skipping saves for dead writes to callee-saved registers
  - This would reduce prologue/epilogue overhead for functions that write to callee-saved registers but don't use the values

## Other Features

### Multi-Function Compilation

- **Module Compilation**: Compile multiple functions together
- **Cross-Function Optimization**: Optimize across function boundaries
- **Function Address Resolution**: Resolve function addresses for calls

### Performance Monitoring

- **Instruction Counting**: Track instruction counts for optimization
- **Register Pressure Analysis**: Analyze register pressure for optimization
- **Code Size Optimization**: Optimize for code size vs. performance

### Emission Optimizations

- **Edit Counting Per Block**: Count edits per block ahead of time for lookahead island emission
  - Used to estimate worst-case block size
  - Helps determine when islands are needed
- **Block Padding**: Optional padding between blocks for stress testing
  - Tests island/veneer insertion under pressure
  - Useful for fuzzing and testing edge cases

### Safepoint and Stack Map Handling

- **Safepoint Detection**: Identify safepoint instructions (`is_safepoint()`)
- **Stack Map Generation**: Generate stack maps at safepoints for GC
- **User Stack Maps**: Handle user-provided stack map metadata
- **Note**: Only needed if garbage collection is required

## Notes

- These features are **not** part of the initial implementation
- They can be added incrementally as needed
- Some may require significant refactoring
- Prioritize based on actual needs and performance requirements

  // 6. Identify cold blocks (deferred: mark blocks unlikely to execute)
  //
  // Cold blocks are blocks that are unlikely to execute (e.g., error handling paths).
  // These can be placed at the end of the function during block layout optimization
  // to improve code locality for the hot path.
  //
  // TODO: Implement cold block identification in a future phase. This could use:
  // - Profile data (if available)
  // - Heuristics (e.g., blocks dominated by unlikely conditions)
  // - User annotations
