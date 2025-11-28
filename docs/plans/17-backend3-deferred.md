# Backend3: Deferred Features

This document tracks features and optimizations that are **not** part of the initial backend3 implementation but may be added later.

## Advanced Optimizations

### Branch Optimization
- **Branch Threading**: Eliminate empty blocks by redirecting labels through unconditional jumps
- **Empty Block Elimination**: Remove blocks that only contain unconditional branches
- **Advanced Fallthrough Optimization**: Reorder blocks to maximize fallthrough opportunities
- **Branch Inversion**: Optimize conditional branches by inverting conditions when beneficial

### Block Layout Optimization
- **Profile-Guided Optimization**: Use execution profiles to identify hot/cold blocks
- **Advanced Cold Block Sinking**: More sophisticated cold block placement
- **Block Reordering**: Optimize block order based on branch probabilities

## Code Generation Features

### Out-of-Range Branch Handling
- **Veneer/Island Insertion**: Handle branches that exceed ±4KB range
- **Long-Range Jump Support**: Support for functions > 4KB
- **Deadline Tracking**: Track when branches go out of range and insert islands

### Constant Pool
- **Large Constant Storage**: Store large constants in data section
- **PC-Relative Constant Loading**: Load constants via PC-relative addressing
- **Constant Deduplication**: Share constants across functions

### Frame Pointer
- **Optional Frame Pointer**: Use frame pointer for easier debugging
- **Frame Pointer Optimization**: Only use FP when needed (variable-sized allocations, etc.)

## Debugging and Diagnostics

### Debug Information
- **Source Location Tracking**: Track source locations through compilation
- **Debug Tags**: Emit debug metadata for debugging tools
- **Value Label Ranges**: Track where values are live in machine code

### Unwind Information
- **Exception Handling**: Generate unwind info for exception handling
- **Stack Unwinding**: Support for stack unwinding (SystemV, Windows, etc.)

## Advanced Register Allocation

### Register Allocation Optimizations
- **Rematerialization**: Recompute values instead of spilling
- **Coalescing**: Merge related virtual registers
- **Live Range Splitting**: Split live ranges for better allocation

## Other Features

### Multi-Function Compilation
- **Module Compilation**: Compile multiple functions together
- **Cross-Function Optimization**: Optimize across function boundaries
- **Function Address Resolution**: Resolve function addresses for calls

### Performance Monitoring
- **Instruction Counting**: Track instruction counts for optimization
- **Register Pressure Analysis**: Analyze register pressure for optimization
- **Code Size Optimization**: Optimize for code size vs. performance

## Notes

- These features are **not** part of the initial implementation
- They can be added incrementally as needed
- Some may require significant refactoring
- Prioritize based on actual needs and performance requirements

