# Backend3 Phase 4: Control Flow

**Goal**: Branches and calls

**Timeline**: Week 4

**Deliverable**: Can compile functions with branches and calls

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

**Future Optimizations** (Deferred):

- **Empty Block Elimination**: Remove blocks that only contain unconditional branches
- **Branch Threading**: Redirect labels through unconditional jumps
- **Unnecessary Jump Elimination**: Remove jumps to immediately following blocks
- **Branch Inversion**: Optimize condition inversion based on branch probabilities

**Branch Range Validation**:

- Conditional branches: ±4KB (12-bit signed offset × 2 bytes)
- Unconditional jumps: ±1MB (20-bit signed offset × 2 bytes)
- Currently assuming functions < 4KB
- If out of range detected, panic (veneers deferred to later)

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

- Unit tests for branch lowering
- Unit tests for branch resolution
- Unit tests for call lowering
- Unit tests for multi-return
- Unit tests for relocation fixup
- Integration test: Compile function with branches
- Integration test: Compile function with calls
- Integration test: Compile function with multi-return

## Success Criteria

- ✅ Can lower branches (Jump, Br)
- ✅ Can resolve two-dest branches to single-dest
- ✅ Can lower function calls
- ✅ Can handle multi-return (>2 values)
- ✅ Can fix up relocations
- ✅ Can compile functions with branches and calls end-to-end
