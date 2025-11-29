//! Block lowering order computation with critical edge splitting

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec,
    vec::Vec,
};

use lpc_lpir::{BlockEntity, ControlFlowGraph, DominatorTree};

use crate::backend3::{
    types::BlockIndex,
    vcode::{BlockLoweringOrder, LoweredBlock},
};

/// Compute block lowering order for a function
///
/// This performs:
/// 1. Critical edge detection and splitting
/// 2. Reverse post-order (RPO) traversal
/// 3. Cold block identification
/// 4. Indirect branch target tracking
pub fn compute_block_order(
    func: &lpc_lpir::Function,
    cfg: &ControlFlowGraph,
    _domtree: &DominatorTree,
) -> BlockLoweringOrder {
    // Build block index mapping
    let block_to_index: BTreeMap<BlockEntity, usize> = func
        .blocks()
        .enumerate()
        .map(|(idx, block)| (block, idx))
        .collect();

    // 1. Detect critical edges
    let critical_edges_vec = detect_critical_edges(func, cfg, &block_to_index);
    let critical_edges = &critical_edges_vec;

    // 2. Create edge blocks for critical edges
    let mut edge_blocks = BTreeMap::new();
    let mut next_edge_block_idx = func.block_count();
    for (from, to, _succ_idx) in critical_edges {
        let edge_block_idx = BlockIndex::new(next_edge_block_idx);
        next_edge_block_idx += 1;
        edge_blocks.insert((*from, *to, *_succ_idx), edge_block_idx);
    }

    // 3. Build lowered block order (RPO)
    let lowered_order =
        build_lowered_order(func, cfg, critical_edges, &edge_blocks, &block_to_index);

    // 4. Build successor lists for lowered blocks
    let lowered_succs = build_lowered_succs(
        func,
        cfg,
        critical_edges,
        &edge_blocks,
        &block_to_index,
        &lowered_order,
    );

    // 5. Build block_to_index mapping (IR blocks to lowered block indices)
    let mut block_to_lowered_index = BTreeMap::new();
    for (idx, lowered_block) in lowered_order.iter().enumerate() {
        match lowered_block {
            LoweredBlock::Orig { block } => {
                let lowered_idx = BlockIndex::new(idx);
                block_to_lowered_index.insert(*block, lowered_idx);
            }
            LoweredBlock::Edge {
                from: _,
                to: _,
                succ_idx: _,
            } => {
                // Edge blocks don't map to IR blocks, but we can still track them
                // For now, we'll skip them in the mapping
            }
        }
    }

    // 6. Identify cold blocks (basic heuristics)
    //
    // Cold blocks are blocks that are unlikely to execute (e.g., error handling paths).
    // These can be placed at the end of the function during block layout optimization
    // to improve code locality for the hot path.
    //
    // Basic heuristics implemented:
    // - Blocks that are not the entry block and have no predecessors (unreachable)
    // - Future: Profile data, blocks dominated by unlikely conditions, user annotations
    let cold_blocks = identify_cold_blocks(func, cfg, &block_to_index, &lowered_order);

    // 7. Identify indirect branch targets
    //
    // Indirect branch targets are blocks that are reached via indirect branches
    // (e.g., computed jumps, switch statements with jump tables). These blocks
    // may require special alignment or handling during emission.
    //
    // Currently, LPIR does not have indirect branches (no br_table, computed jumps, etc.).
    // When indirect branches are added to LPIR, this function should:
    // - Analyze branch instructions to identify indirect branches
    // - Track which blocks are targets of indirect branches
    // - Mark these blocks for special alignment if needed
    //
    // For now, we return an empty set since there are no indirect branches.
    let indirect_targets =
        identify_indirect_branch_targets(func, cfg, &block_to_index, &lowered_order);

    BlockLoweringOrder {
        lowered_order,
        lowered_succs,
        block_to_index: block_to_lowered_index,
        cold_blocks,
        indirect_targets,
    }
}

/// Detect critical edges in the CFG
///
/// A critical edge is an edge where:
/// - The source block has multiple successors, AND
/// - The target block has multiple predecessors
///
/// These edges need intermediate blocks for phi value moves.
///
/// Returns tuples of (from_block, to_block, succ_idx) where succ_idx is the index
/// of this edge in the source block's successor list.
fn detect_critical_edges(
    func: &lpc_lpir::Function,
    cfg: &ControlFlowGraph,
    block_to_index: &BTreeMap<BlockEntity, usize>,
) -> Vec<(BlockEntity, BlockEntity, u32)> {
    let mut critical_edges = Vec::new();

    for block in func.blocks() {
        let block_idx = match block_to_index.get(&block) {
            Some(&idx) => idx,
            None => continue,
        };

        let succs = cfg.successors(block_idx);
        if succs.len() <= 1 {
            continue; // Not multiple successors
        }

        // Check each successor, tracking the index in the successor list
        for (edge_idx, &succ_idx) in succs.iter().enumerate() {
            let preds = cfg.predecessors(succ_idx);
            if preds.len() > 1 {
                // Critical edge: source has multiple succs, target has multiple preds
                // Find the successor block entity
                let succ_block = func
                    .blocks()
                    .enumerate()
                    .find(|(idx, _)| *idx == succ_idx)
                    .map(|(_, block)| block);
                if let Some(succ_block) = succ_block {
                    critical_edges.push((block, succ_block, edge_idx as u32));
                }
            }
        }
    }

    critical_edges
}

/// Build lowered block order (RPO traversal)
///
/// This matches Cranelift's approach: edge blocks are interleaved immediately
/// after their source blocks in RPO order, not appended at the end.
fn build_lowered_order(
    func: &lpc_lpir::Function,
    cfg: &ControlFlowGraph,
    critical_edges: &[(BlockEntity, BlockEntity, u32)],
    _edge_blocks: &BTreeMap<(BlockEntity, BlockEntity, u32), BlockIndex>,
    _block_to_index: &BTreeMap<BlockEntity, usize>,
) -> Vec<LoweredBlock> {
    // Get RPO order of original blocks
    let rpo = cfg.reverse_post_order();

    // Build lowered order: original blocks in RPO, with edge blocks interleaved
    // immediately after their source blocks (matching Cranelift's approach)
    let mut lowered_order = Vec::new();

    // Add original blocks in RPO order, interleaving edge blocks immediately after
    for &block_idx in &rpo {
        let block = func
            .blocks()
            .enumerate()
            .find(|(idx, _)| *idx == block_idx)
            .map(|(_, block)| block);
        if let Some(block) = block {
            // Add the original block
            lowered_order.push(LoweredBlock::Orig { block });

            // Insert edge blocks immediately after this block (if any)
            // This matches Cranelift's approach where edge blocks follow their source
            for (from, to, succ_idx) in critical_edges {
                if *from == block {
                    lowered_order.push(LoweredBlock::Edge {
                        from: *from,
                        to: *to,
                        succ_idx: *succ_idx,
                    });
                }
            }
        }
    }

    lowered_order
}

/// Build successor lists for lowered blocks
fn build_lowered_succs(
    func: &lpc_lpir::Function,
    cfg: &ControlFlowGraph,
    critical_edges: &[(BlockEntity, BlockEntity, u32)],
    _edge_blocks: &BTreeMap<(BlockEntity, BlockEntity, u32), BlockIndex>,
    _block_to_index: &BTreeMap<BlockEntity, usize>,
    lowered_order: &[LoweredBlock],
) -> Vec<Vec<BlockIndex>> {
    // Build mapping from IR blocks to lowered block indices
    let mut ir_to_lowered: BTreeMap<BlockEntity, BlockIndex> = BTreeMap::new();
    for (idx, lowered_block) in lowered_order.iter().enumerate() {
        match lowered_block {
            LoweredBlock::Orig { block } => {
                ir_to_lowered.insert(*block, BlockIndex::new(idx));
            }
            LoweredBlock::Edge {
                from: _,
                to: _,
                succ_idx: _,
            } => {
                // Edge blocks are tracked separately
            }
        }
    }

    // Build mapping from edge blocks to their lowered indices
    let mut edge_to_lowered: BTreeMap<(BlockEntity, BlockEntity, u32), BlockIndex> =
        BTreeMap::new();
    for (idx, lowered_block) in lowered_order.iter().enumerate() {
        match lowered_block {
            LoweredBlock::Orig { block: _ } => {}
            LoweredBlock::Edge { from, to, succ_idx } => {
                edge_to_lowered.insert((*from, *to, *succ_idx), BlockIndex::new(idx));
            }
        }
    }

    // Build successor lists
    let mut lowered_succs = Vec::new();
    for lowered_block in lowered_order {
        match lowered_block {
            LoweredBlock::Orig { block } => {
                // Find block index by searching through blocks
                let block_idx = func
                    .blocks()
                    .enumerate()
                    .find(|(_, b)| *b == *block)
                    .map(|(idx, _)| idx)
                    .unwrap_or(0);
                let mut succs = Vec::new();

                // Get original successors, tracking the edge index
                for (edge_idx, &succ_idx) in cfg.successors(block_idx).iter().enumerate() {
                    let succ_block = func
                        .blocks()
                        .enumerate()
                        .find(|(idx, _)| *idx == succ_idx)
                        .map(|(_, block)| block);

                    if let Some(succ_block) = succ_block {
                        // Check if this edge is critical by matching (from, to, succ_idx)
                        let edge_key = (*block, succ_block, edge_idx as u32);
                        if critical_edges.iter().any(|(f, t, s)| {
                            *f == *block && *t == succ_block && *s == edge_idx as u32
                        }) {
                            // Use edge block instead
                            if let Some(&edge_block_idx) = edge_to_lowered.get(&edge_key) {
                                succs.push(edge_block_idx);
                            }
                        } else {
                            // Direct edge
                            if let Some(&lowered_idx) = ir_to_lowered.get(&succ_block) {
                                succs.push(lowered_idx);
                            }
                        }
                    }
                }

                lowered_succs.push(succs);
            }
            LoweredBlock::Edge {
                from: _,
                to,
                succ_idx: _,
            } => {
                // Edge block succeeds to the target block
                if let Some(&target_lowered_idx) = ir_to_lowered.get(to) {
                    lowered_succs.push(vec![target_lowered_idx]);
                } else {
                    lowered_succs.push(Vec::new());
                }
            }
        }
    }

    lowered_succs
}

/// Identify cold blocks using basic heuristics
///
/// Cold blocks are blocks that are unlikely to execute. This function implements
/// basic heuristics to identify such blocks:
///
/// 1. Unreachable blocks: Blocks that are not the entry block and have no predecessors
///    (these are dead code and should be marked as cold)
///
/// Future heuristics that could be added:
/// - Blocks dominated by unlikely conditions (e.g., error checks)
/// - Blocks that are only reached via error paths
/// - Profile-guided identification (if profile data is available)
/// - User annotations
fn identify_cold_blocks(
    func: &lpc_lpir::Function,
    cfg: &ControlFlowGraph,
    block_to_index: &BTreeMap<BlockEntity, usize>,
    lowered_order: &[LoweredBlock],
) -> BTreeSet<BlockIndex> {
    let mut cold_blocks = BTreeSet::new();

    // Find entry block
    let entry_block = func.entry_block();
    let entry_block_idx = entry_block.and_then(|b| block_to_index.get(&b).copied());

    // Identify unreachable blocks (blocks with no predecessors that aren't the entry block)
    for block in func.blocks() {
        let block_idx = match block_to_index.get(&block) {
            Some(&idx) => idx,
            None => continue,
        };

        // Skip entry block (it's always hot)
        if Some(block_idx) == entry_block_idx {
            continue;
        }

        // Check if block has predecessors
        let preds = cfg.predecessors(block_idx);
        if preds.is_empty() {
            // Unreachable block - mark as cold
            // Find the lowered block index for this IR block
            for (lowered_idx, lowered_block) in lowered_order.iter().enumerate() {
                match lowered_block {
                    LoweredBlock::Orig { block: orig_block } if *orig_block == block => {
                        cold_blocks.insert(BlockIndex::new(lowered_idx));
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    cold_blocks
}

/// Identify indirect branch targets
///
/// Indirect branch targets are blocks that are reached via indirect branches
/// (e.g., computed jumps, switch statements with jump tables). These blocks
/// may require special alignment or handling during emission.
///
/// # Current Status
///
/// LPIR does not currently have indirect branches (no br_table, computed jumps, etc.).
/// This function returns an empty set for now.
///
/// # Future Implementation
///
/// When indirect branches are added to LPIR, this function should:
///
/// 1. **Identify indirect branch instructions**: Look for opcodes like:
///    - `br_table` (switch statements with jump tables)
///    - Computed jumps (jumps where the target is computed at runtime)
///    - Indirect calls (if they can branch to blocks)
///
/// 2. **Track target blocks**: For each indirect branch, identify which blocks
///    can be reached via that branch and mark them as indirect targets.
///
/// 3. **Mark for alignment**: Indirect branch targets may require special alignment
///    (e.g., 4-byte alignment for jump tables) to ensure efficient branching.
///
/// # Example (Future)
///
/// ```rust,ignore
/// // When LPIR has br_table, this function will identify indirect branch targets:
/// // for block in func.blocks() {
/// //     for inst in func.block_insts(block) {
/// //         if let Some(inst_data) = func.dfg.inst_data(inst) {
/// //             if matches!(inst_data.opcode, Opcode::BrTable) {
/// //                 // Mark all targets of this br_table as indirect targets
/// //                 for target_block in br_table_targets {
/// //                     indirect_targets.insert(target_block);
/// //                 }
/// //             }
/// //         }
/// //     }
/// // }
/// ```
fn identify_indirect_branch_targets(
    _func: &lpc_lpir::Function,
    _cfg: &ControlFlowGraph,
    _block_to_index: &BTreeMap<BlockEntity, usize>,
    _lowered_order: &[LoweredBlock],
) -> BTreeSet<BlockIndex> {
    // LPIR does not currently have indirect branches
    // Return empty set for now
    BTreeSet::new()
}
