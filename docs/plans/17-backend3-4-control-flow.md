# Backend3 Phase 4: Control Flow

**Goal**: Branches and calls

**Timeline**: Week 4

**Deliverable**: Can compile functions with branches and calls

## Cranelift References

**Primary Reference**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/machinst/`

- **Branch Resolution**: `buffer.rs` - Branch optimization and resolution
  - Two-dest to single-dest conversion
  - Fallthrough detection
  - Branch threading (label aliasing)
  - Latest-branches tracking
  - Branch range validation and veneer insertion
- **Lowering Branches**: `lower.rs` - Branch instruction lowering
  - Jump and Br lowering
  - Block parameter handling (phi moves)
  - Two-dest branch representation
- **Call Lowering**: `lower.rs` - Function call lowering
  - Argument preparation
  - Return value handling
  - Call relocations

**RISC-V Specific References**: `/Users/yona/dev/photomancer/wasmtime/cranelift/codegen/src/isa/riscv64/`

- **Branch Emission**: `inst/emit.rs` - RISC-V branch instruction emission
  - Conditional branch encoding
  - Unconditional jump encoding
  - Branch offset patching
- **Call Emission**: `inst/emit.rs` - Function call emission
  - JALR instruction for calls
  - Argument passing
  - Return value handling

## Tasks

### 1. Branch lowering

**Components**:

- Lower Jump and Br instructions
- Handle block parameters (phi moves in edge blocks)
- Two-dest branch representation
- Record branch relocations

**See**: Main plan for branch lowering details (`17-backend3.md`)

### 2. Branch resolution (ISA-agnostic)

**File**: `backend3/branch.rs`

**Purpose**: Resolve two-dest branches to single-dest branches during emission, and perform basic branch optimizations.

**Two-Dest Branch Representation**:

During lowering, conditional branches are represented with two targets:
- `Branch { kind, rs1, rs2, target_true, target_false }`

This allows the emitter to decide which target should be fallthrough based on block layout.

**Two-Dest to Single-Dest Conversion**:

During emission, two-dest branches are converted to single-dest branches. The algorithm:

1. **Determine Fallthrough Target**: Check which target (if any) is the next block in emission order
2. **Emit Branch**:
   - If one target is fallthrough: emit conditional branch to the other target
   - If neither is fallthrough: emit inverted conditional to one target, then unconditional to the other (or optimize later)
   - If both are fallthrough: eliminate branch (shouldn't happen)

**Fallthrough Detection Algorithm**:

```rust
fn determine_fallthrough(
    current_block: BlockIndex,
    emission_order: &[BlockIndex],
    target_true: BlockIndex,
    target_false: BlockIndex,
) -> Option<BlockIndex> {
    // Find current block's position in emission order
    let current_pos = emission_order.iter()
        .position(|&b| b == current_block)?;

    // Check if next block is one of our targets
    if current_pos + 1 < emission_order.len() {
        let next_block = emission_order[current_pos + 1];
        if next_block == target_false {
            return Some(target_false);
        } else if next_block == target_true {
            return Some(target_true);
        }
    }

    None
}
```

**Implementation** (integrated into emission):

```rust
impl VCode<MachInst> {
    fn emit_branch(
        &self,
        buffer: &mut InstBuffer,
        state: &mut EmitState,
        mut branch: MachInst,
        branch_info: BranchInfo,
        emission_order: &[BlockIndex],
        current_block: BlockIndex,
    ) {
        match branch_info {
            BranchInfo::TwoDest { target_true, target_false } => {
                // Determine which target (if any) is fallthrough
                let fallthrough = determine_fallthrough(
                    current_block,
                    emission_order,
                    target_true,
                    target_false,
                );

                let (target_label, invert) = match fallthrough {
                    Some(ft) if ft == target_false => {
                        // False is fallthrough, branch to true
                        (MachLabel::from_block(target_true), false)
                    }
                    Some(ft) if ft == target_true => {
                        // True is fallthrough, branch to false (invert condition)
                        (MachLabel::from_block(target_false), true)
                    }
                    None => {
                        // Neither is fallthrough - emit inverted branch to false,
                        // then unconditional to true (or optimize later)
                        // For now, just branch to true (false will need separate jump)
                        (MachLabel::from_block(target_true), false)
                    }
                };

                // Invert condition if needed
                if invert {
                    branch.invert_condition();
                }

                // Emit branch with label target
                let branch_offset = buffer.cur_offset();
                buffer.emit_branch_with_label(branch, target_label);

                // Resolve or record fixup
                state.resolve_or_record_fixup(
                    buffer,
                    branch_offset,
                    target_label,
                    BranchType::Conditional,
                );

                // If neither target was fallthrough, emit unconditional jump to other target
                if fallthrough.is_none() {
                    let other_target = if target_label == MachLabel::from_block(target_true) {
                        target_false
                    } else {
                        target_true
                    };
                    let jump_label = MachLabel::from_block(other_target);
                    let jump_offset = buffer.cur_offset();
                    buffer.emit_branch_with_label(
                        MachInst::Jal { rd: zero, imm: 0 }, // Placeholder
                        jump_label,
                    );
                    state.resolve_or_record_fixup(
                        buffer,
                        jump_offset,
                        jump_label,
                        BranchType::Unconditional,
                    );
                }
            }
            BranchInfo::OneDest { target } => {
                // Unconditional branch - emit directly
                let target_label = MachLabel::from_block(target);
                let branch_offset = buffer.cur_offset();
                buffer.emit_branch_with_label(branch, target_label);

                state.resolve_or_record_fixup(
                    buffer,
                    branch_offset,
                    target_label,
                    BranchType::Unconditional,
                );
            }
        }
    }
}
```

**Basic Branch Simplification** (Initial Implementation):

For the initial implementation, we keep it simple:

1. **Two-Dest Resolution**: Convert to single-dest as described above
2. **Fallthrough Optimization**: Handled by block emission order (cold blocks at end)
3. **No Empty Block Elimination**: Deferred (see deferred features)
4. **No Branch Threading**: Deferred (see deferred features)
5. **No Latest-Branches Tracking**: Deferred (see deferred features)

**Future Optimizations** (Deferred - see `17-backend3-deferred.md`):

- **Empty Block Elimination**: Remove blocks that only contain unconditional branches
- **Branch Threading**: Redirect labels through unconditional jumps
  - When a label is bound to an unconditional jump, create a label alias
  - All references to the label are redirected to the jump's target
  - Effectively removes empty blocks that only contain jumps
- **Unnecessary Jump Elimination**: Remove jumps to immediately following blocks
  - Detect when branch target is bound to fallthrough location
  - Remove branch instruction entirely
- **Branch Inversion**: Optimize condition inversion based on branch probabilities
  - Track "latest branches" (contiguous branches at buffer tail)
  - When conditional + unconditional pair is detected, invert condition if beneficial
  - Requires tracking both normal and inverted forms of conditional branches
- **Latest Branches Tracking**: Track contiguous branches at buffer tail for optimization
  - Enables branch editing by truncating buffer
  - Cleared when non-branch code is emitted
  - Used for advanced peephole optimizations

**Branch Range Validation**:

- Conditional branches: ±4KB (12-bit signed offset × 2 bytes)
- Unconditional jumps: ±1MB (20-bit signed offset × 2 bytes)
- Currently assuming functions < 4KB
- If out of range detected, panic (veneers deferred to later)

**Out-of-Range Branch Handling** (Deferred):

The current implementation panics if a branch offset exceeds its range. Future work will add:

- **Deadline Tracking**: Monitor forward branch references as code is emitted
  - Calculate worst-case size of upcoming blocks
  - Track when branches approach range limits
- **Island Insertion**: Insert code chunks in the middle of emission to hold veneers
  - Islands are inserted when deadline is reached (before branch goes out of range)
  - Islands are placed between blocks or with branch-around protection
- **Veneer Generation**: Create trampoline instructions that extend branch range
  - Conditional branch → unconditional jump (extends range from ±4KB to ±1MB)
  - Veneers are placed in islands
- **Fixup Processing**: Handle fixups during island emission
  - Split fixups into pending and final lists
  - Process fixups efficiently during island emission

See `17-backend3-deferred.md` for more details on island/veneer insertion.

### 3. Call lowering

**Components**:

- Lower Call instructions
- Argument preparation (registers + stack)
- Return value handling
- Record function call relocations

**See**: Main plan for call handling (`17-backend3.md`)

### 4. Multi-return support

**Components**:

- Return area mechanism
- Handle >2 return values
- Return area pointer passing

**See**: Main plan for multi-return details (`17-backend3.md`)

### 5. Relocation fixup (ISA-agnostic)

**File**: `backend3/reloc.rs`

**Components**:

- Relocation handling
- Fix function call addresses
- Resolve branch targets

**See**: Main plan for relocation handling (`17-backend3.md`)

## Testing

**Test Format Guidelines**:

- **Input**: Use textual LPIR format for clarity. Tests should define functions using the textual LPIR syntax to make the input code clear and readable, especially for control flow patterns.
- **Expected Output**: Use assembler format to clearly show the expected RISC-V 32 machine code. This is especially important for branch instructions, call sequences, and multi-return handling.

**Test Examples**:

```rust
#[test]
fn test_branch_lowering() {
    // Input: textual LPIR format for clarity
    let lpir_text = r#"
        function @test(i32 %a, i32 %b) -> i32 {
        entry:
            %cond = icmp eq %a, %b
            br %cond, then, else
        then:
            ret %a
        else:
            ret %b
        }
    "#;
    
    let func = parse_lpir_function(lpir_text);
    let vcode = Lower::new(func).lower(&block_order);
    
    // Verify branch lowering...
}

#[test]
fn test_branch_emission() {
    // Input: textual LPIR format
    let lpir_text = r#"
        function @test(i32 %a, i32 %b) -> i32 {
        entry:
            %cond = icmp eq %a, %b
            br %cond, then, else
        then:
            ret %a
        else:
            ret %b
        }
    "#;
    
    let func = parse_lpir_function(lpir_text);
    let vcode = Lower::new(func).lower(&block_order);
    let regalloc = vcode.run_regalloc();
    let buffer = vcode.emit(&regalloc);
    
    // Expected: assembler format showing branch instructions
    let expected_asm = r#"
        # Prologue...
        
        # Compare and branch
        beq  a0, a1, .Lthen
        j    .Lelse
    .Lthen:
        # Return %a
        lw   fp, 0(sp)
        lw   ra, 4(sp)
        addi sp, sp, 8
        jalr zero, ra, 0
    .Lelse:
        # Return %b
        mv   a0, a1
        lw   fp, 0(sp)
        lw   ra, 4(sp)
        addi sp, sp, 8
        jalr zero, ra, 0
    "#;
}

#[test]
fn test_call_lowering() {
    // Input: textual LPIR format
    let lpir_text = r#"
        function @callee(i32 %x) -> i32 {
        entry:
            %0 = iadd %x, %x
            ret %0
        }
        
        function @caller(i32 %a) -> i32 {
        entry:
            %0 = call @callee(%a)
            ret %0
        }
    "#;
    
    let func = parse_lpir_function(lpir_text);
    let vcode = Lower::new(func).lower(&block_order);
    
    // Verify call lowering...
}

#[test]
fn test_call_emission() {
    // Input: textual LPIR format
    let lpir_text = r#"
        function @callee(i32 %x) -> i32 {
        entry:
            %0 = iadd %x, %x
            ret %0
        }
        
        function @caller(i32 %a) -> i32 {
        entry:
            %0 = call @callee(%a)
            ret %0
        }
    "#;
    
    let func = parse_lpir_function(lpir_text);
    let vcode = Lower::new(func).lower(&block_order);
    let regalloc = vcode.run_regalloc();
    let buffer = vcode.emit(&regalloc);
    
    // Expected: assembler format showing call sequence
    let expected_asm = r#"
        # Prologue...
        
        # Call setup
        mv   a0, a0        # Pass argument
        jal  ra, callee    # Call function
        
        # Epilogue...
    "#;
}

#[test]
fn test_multi_return() {
    // Input: textual LPIR format
    let lpir_text = r#"
        function @test(i32 %a, i32 %b) -> (i32, i32, i32) {
        entry:
            %0 = iadd %a, %b
            %1 = isub %a, %b
            %2 = imul %a, %b
            ret %0, %1, %2
        }
    "#;
    
    let func = parse_lpir_function(lpir_text);
    let vcode = Lower::new(func).lower(&block_order);
    let regalloc = vcode.run_regalloc();
    let buffer = vcode.emit(&regalloc);
    
    // Expected: assembler format showing return area handling
    let expected_asm = r#"
        # Prologue...
        # Allocate return area on stack
        
        # Compute return values
        add  t0, a0, a1    # %0 = %a + %b
        sub  t1, a0, a1    # %1 = %a - %b
        mul  t2, a0, a1    # %2 = %a * %b
        
        # Store to return area
        sw   t0, 0(sp)
        sw   t1, 4(sp)
        sw   t2, 8(sp)
        
        # Epilogue...
    "#;
}
```

**Test Categories**:

- Unit tests for branch lowering
- Unit tests for branch resolution
- Unit tests for call lowering
- Unit tests for multi-return
- Unit tests for relocation fixup
- Integration test: Compile function with branches
- Integration test: Compile function with calls
- Integration test: Compile function with multi-return

## Implementation Notes

### Branch Optimization Strategy

The initial implementation focuses on correctness over optimization:

1. **Two-Dest Resolution**: Convert two-dest branches to single-dest with fallthrough detection
2. **Block Ordering**: Use emission order to maximize fallthrough opportunities
3. **Simple Branch Emission**: Emit branches with label-based fixups
4. **Range Validation**: Panic if branch out of range (acceptable for initial implementation)

Advanced optimizations (branch threading, latest-branches tracking, etc.) are deferred to maintain simplicity and correctness.

### Integration with Emission

Branch resolution is integrated into the emission phase (`emit.rs`):

- Branches are emitted during instruction emission loop
- Two-dest branches are converted to single-dest before emission
- Label fixups are resolved as labels are bound
- Branch range validation happens during patching

### Testing Strategy

- Test two-dest branch conversion with various fallthrough scenarios
- Test branch range validation (both in-range and out-of-range cases)
- Test label fixup resolution (forward and backward branches)
- Integration tests with complex control flow patterns

## Success Criteria

- ✅ Can lower branches (Jump, Br)
- ✅ Can resolve two-dest branches to single-dest
- ✅ Can lower function calls
- ✅ Can handle multi-return (>2 values)
- ✅ Can fix up relocations
- ✅ Can compile functions with branches and calls end-to-end
