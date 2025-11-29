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
    let mut next_edge_block_idx = func.block_count() as u32;
    for (from, to) in critical_edges {
        let edge_block_idx = BlockIndex::new(next_edge_block_idx);
        next_edge_block_idx += 1;
        edge_blocks.insert((*from, *to), edge_block_idx);
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
                let lowered_idx = BlockIndex::new(idx as u32);
                block_to_lowered_index.insert(*block, lowered_idx);
            }
            LoweredBlock::Edge { from: _, to: _ } => {
                // Edge blocks don't map to IR blocks, but we can still track them
                // For now, we'll skip them in the mapping
            }
        }
    }

    // 6. Identify cold blocks (deferred: mark blocks unlikely to execute)
    let cold_blocks = BTreeSet::new();

    // 7. Identify indirect branch targets (deferred: track blocks that are indirect targets)
    let indirect_targets = BTreeSet::new();

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
fn detect_critical_edges(
    func: &lpc_lpir::Function,
    cfg: &ControlFlowGraph,
    block_to_index: &BTreeMap<BlockEntity, usize>,
) -> Vec<(BlockEntity, BlockEntity)> {
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

        // Check each successor
        for &succ_idx in succs {
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
                    critical_edges.push((block, succ_block));
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
    critical_edges: &[(BlockEntity, BlockEntity)],
    _edge_blocks: &BTreeMap<(BlockEntity, BlockEntity), BlockIndex>,
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
            for (from, to) in critical_edges {
                if *from == block {
                    lowered_order.push(LoweredBlock::Edge {
                        from: *from,
                        to: *to,
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
    critical_edges: &[(BlockEntity, BlockEntity)],
    _edge_blocks: &BTreeMap<(BlockEntity, BlockEntity), BlockIndex>,
    _block_to_index: &BTreeMap<BlockEntity, usize>,
    lowered_order: &[LoweredBlock],
) -> Vec<Vec<BlockIndex>> {
    // Build mapping from IR blocks to lowered block indices
    let mut ir_to_lowered: BTreeMap<BlockEntity, BlockIndex> = BTreeMap::new();
    for (idx, lowered_block) in lowered_order.iter().enumerate() {
        match lowered_block {
            LoweredBlock::Orig { block } => {
                ir_to_lowered.insert(*block, BlockIndex::new(idx as u32));
            }
            LoweredBlock::Edge { from: _, to: _ } => {
                // Edge blocks are tracked separately
            }
        }
    }

    // Build mapping from edge blocks to their lowered indices
    let mut edge_to_lowered: BTreeMap<(BlockEntity, BlockEntity), BlockIndex> = BTreeMap::new();
    for (idx, lowered_block) in lowered_order.iter().enumerate() {
        match lowered_block {
            LoweredBlock::Orig { block: _ } => {}
            LoweredBlock::Edge { from, to } => {
                edge_to_lowered.insert((*from, *to), BlockIndex::new(idx as u32));
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

                // Get original successors
                for &succ_idx in cfg.successors(block_idx) {
                    let succ_block = func
                        .blocks()
                        .enumerate()
                        .find(|(idx, _)| *idx == succ_idx)
                        .map(|(_, block)| block);

                    if let Some(succ_block) = succ_block {
                        // Check if this edge is critical
                        let edge_key = (*block, succ_block);
                        if critical_edges.contains(&edge_key) {
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
            LoweredBlock::Edge { from: _, to } => {
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
