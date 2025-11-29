//! SSA construction for GLSL codegen.
//!
//! This module provides automatic phi node insertion for proper SSA form,
//! using lazy dominance computation similar to LLVM's SSAUpdater.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec,
    vec::Vec,
};

use lpc_lpir::{BlockEntity, ControlFlowGraph, Function, FunctionBuilder, Type, Value};

use crate::error::GlslResult;

/// Per-block information for lazy dominance computation.
struct BlockInfo {
    /// The block entity
    block: BlockEntity,
    /// Value available in this block (if any)
    available_val: Option<Value>,
    /// Block that defines the available value (or self if this block needs a PHI)
    def_block: Option<BlockEntity>,
    /// Postorder number (for dominance computation)
    postorder_num: u32,
    /// Immediate dominator
    idom: Option<BlockEntity>,
    /// Number of predecessors
    num_preds: usize,
    /// Predecessor blocks
    preds: Vec<BlockEntity>,
    /// PHI node if this block needs one
    phi: Option<Value>,
}

impl BlockInfo {
    fn new(block: BlockEntity, available_val: Option<Value>) -> Self {
        let def_block = if available_val.is_some() {
            Some(block)
        } else {
            None
        };
        Self {
            block,
            available_val,
            def_block,
            postorder_num: 0,
            idom: None,
            num_preds: 0,
            preds: Vec::new(),
            phi: None,
        }
    }
}

/// Builder for SSA form that handles phi node insertion automatically.
///
/// This tracks variable definitions per block and automatically inserts
/// phi nodes when variables are modified in multiple blocks, using lazy
/// dominance computation similar to LLVM's SSAUpdater.
pub struct SSABuilder {
    /// Map from variable name to map of block -> value
    /// Tracks the definition of each variable in each block
    defs: BTreeMap<String, BTreeMap<BlockEntity, Value>>,

    /// Set of variables that need phi nodes (modified in multiple blocks)
    needs_phi: BTreeSet<String>,
}

impl SSABuilder {
    /// Create a new SSA builder.
    pub fn new() -> Self {
        Self {
            defs: BTreeMap::new(),
            needs_phi: BTreeSet::new(),
        }
    }

    /// Record a definition of a variable in a block.
    pub fn record_def(&mut self, var: &str, block: BlockEntity, value: Value) {
        self.defs
            .entry(var.to_string())
            .or_insert_with(BTreeMap::new)
            .insert(block, value);

        // Mark as needing phi if defined in multiple blocks
        if let Some(blocks) = self.defs.get(var) {
            if blocks.len() > 1 {
                self.needs_phi.insert(var.to_string());
            }
        }
    }

    /// Get the value of a variable at the end of a block.
    ///
    /// This lazily computes dominance to find the correct value.
    /// This is the main entry point for getting SSA values.
    pub fn get_value_at_end_of_block(
        &mut self,
        var: &str,
        block: BlockEntity,
        function: &Function,
    ) -> GlslResult<Option<Value>> {
        // Check if we already have a value for this block
        if let Some(value) = self.defs.get(var).and_then(|m| m.get(&block)) {
            return Ok(Some(*value));
        }

        // Build block list backwards-reachable from target block
        let (mut block_list, pseudo_entry) = self.build_block_list(var, block, function)?;

        // Special case: unreachable block
        if block_list.is_empty() {
            return Ok(None);
        }

        // Compute dominators for the subset
        self.find_dominators(&mut block_list, &pseudo_entry);

        // Find where PHIs are needed
        self.find_phi_placement(&mut block_list);

        // Compute available values
        self.find_available_values(&mut block_list)?;

        // Find the target block's info and return its value
        if let Some(info) = block_list.iter().find(|info| info.block == block) {
            Ok(info.available_val)
        } else {
            Ok(None)
        }
    }

    /// Build a list of blocks backwards-reachable from the target block.
    ///
    /// Returns a list of blocks that could affect the value at the target,
    /// along with a pseudo-entry block.
    fn build_block_list(
        &self,
        var: &str,
        target_block: BlockEntity,
        function: &Function,
    ) -> GlslResult<(Vec<BlockInfo>, BlockInfo)> {
        // Build CFG and block index mapping
        let cfg = ControlFlowGraph::from_function(function);
        let block_to_index: BTreeMap<BlockEntity, usize> = function
            .blocks()
            .enumerate()
            .map(|(idx, b)| (b, idx))
            .collect();
        let index_to_block: Vec<BlockEntity> = function.blocks().collect();

        let target_idx = match block_to_index.get(&target_block) {
            Some(&idx) => idx,
            None => {
                // Target block not found - return empty with dummy pseudo-entry
                let dummy_block = function.blocks().next().unwrap_or(target_block);
                return Ok((Vec::new(), BlockInfo::new(dummy_block, None)));
            }
        };

        let mut block_map: BTreeMap<BlockEntity, BlockInfo> = BTreeMap::new();
        let mut worklist: Vec<BlockEntity> = vec![target_block];
        let mut root_list: Vec<BlockEntity> = Vec::new();

        // Initialize target block
        let target_val = self
            .defs
            .get(var)
            .and_then(|m| m.get(&target_block))
            .copied();
        block_map.insert(target_block, BlockInfo::new(target_block, target_val));
        if target_val.is_some() {
            root_list.push(target_block);
        }

        // Backwards traversal: find all blocks that could affect the value
        while let Some(current) = worklist.pop() {
            let current_idx = match block_to_index.get(&current) {
                Some(&idx) => idx,
                None => continue,
            };

            // Get predecessors of current block from CFG
            let pred_indices: Vec<usize> = cfg.predecessors(current_idx).iter().copied().collect();
            let preds: Vec<BlockEntity> = pred_indices
                .iter()
                .filter_map(|&idx| index_to_block.get(idx).copied())
                .collect();

            let current_info = block_map.get_mut(&current).unwrap();
            current_info.num_preds = preds.len();
            current_info.preds = preds.clone();

            for pred in preds {
                if block_map.contains_key(&pred) {
                    continue;
                }

                // Check if predecessor defines the variable
                let pred_val = self.defs.get(var).and_then(|m| m.get(&pred)).copied();
                let pred_info = BlockInfo::new(pred, pred_val);
                block_map.insert(pred, pred_info);

                if pred_val.is_some() {
                    // This is a root (defines the variable)
                    root_list.push(pred);
                } else {
                    // Continue backwards traversal
                    worklist.push(pred);
                }
            }
        }

        // Forward traversal: assign postorder numbers
        let mut block_list: Vec<BlockInfo> = Vec::new();
        let mut worklist: Vec<BlockEntity> = root_list.clone();
        let mut postorder_num = 1u32;
        let mut visited: BTreeSet<BlockEntity> = BTreeSet::new();

        // Mark roots as visited
        for root in &root_list {
            visited.insert(*root);
            if let Some(info) = block_map.get_mut(root) {
                info.postorder_num = u32::MAX; // Mark as processing
            }
        }

        // Forward DFS to assign postorder numbers
        while let Some(current) = worklist.pop() {
            let current_idx = match block_to_index.get(&current) {
                Some(&idx) => idx,
                None => continue,
            };

            let current_info = block_map.get_mut(&current).unwrap();

            if current_info.postorder_num == 0 {
                // Not yet visited - mark as processing
                current_info.postorder_num = u32::MAX;
                worklist.push(current);

                // Add successors to worklist from CFG
                let succ_indices: Vec<usize> =
                    cfg.successors(current_idx).iter().copied().collect();
                for succ_idx in succ_indices {
                    if let Some(&succ) = index_to_block.get(succ_idx) {
                        if block_map.contains_key(&succ) && !visited.contains(&succ) {
                            visited.insert(succ);
                            worklist.push(succ);
                        }
                    }
                }
            } else if current_info.postorder_num == u32::MAX {
                // Finished processing - assign postorder number
                current_info.postorder_num = postorder_num;
                postorder_num += 1;

                // Add to block list if not a root
                if current_info.available_val.is_none() {
                    block_list.push(block_map.remove(&current).unwrap());
                }
            }
        }

        // Create pseudo-entry block (use first block as sentinel, but mark it specially)
        let pseudo_entry_block = if let Some(&first_block) = index_to_block.first() {
            first_block
        } else {
            // No blocks - return empty
            return Ok((Vec::new(), BlockInfo::new(target_block, None)));
        };

        let pseudo_entry = BlockInfo {
            block: pseudo_entry_block,
            available_val: None,
            def_block: None,
            postorder_num,
            idom: None,
            num_preds: 0,
            preds: Vec::new(),
            phi: None,
        };

        // Set IDom of roots to pseudo-entry
        for root in &root_list {
            if let Some(info) = block_map.get_mut(root) {
                info.idom = Some(pseudo_entry.block);
            }
        }

        // Rebuild block_list with all blocks (including roots) for processing
        let mut all_blocks: Vec<BlockInfo> = block_list;
        for root in root_list {
            if let Some(info) = block_map.remove(&root) {
                all_blocks.push(info);
            }
        }

        Ok((all_blocks, pseudo_entry))
    }

    /// Compute immediate dominators using Cooper-Harvey-Kennedy algorithm.
    fn find_dominators(&self, block_list: &mut [BlockInfo], pseudo_entry: &BlockInfo) {
        let mut changed = true;
        while changed {
            changed = false;

            // Iterate in reverse postorder (forward on CFG edges)
            for i in (0..block_list.len()).rev() {
                if block_list[i].postorder_num == pseudo_entry.postorder_num {
                    continue; // Skip pseudo-entry
                }

                let mut new_idom: Option<BlockEntity> = None;

                // Find IDom as intersection of all predecessors' IDoms
                for pred_block in &block_list[i].preds {
                    let pred_info = block_list.iter().find(|b| b.block == *pred_block);
                    if let Some(pred) = pred_info {
                        if pred.postorder_num == 0 {
                            // Unreachable predecessor - treat as definition
                            continue;
                        }

                        if new_idom.is_none() {
                            new_idom = pred.idom;
                        } else if let (Some(idom1), Some(idom2)) = (new_idom, pred.idom) {
                            new_idom = Some(self.intersect_dominators(idom1, idom2, block_list));
                        }
                    }
                }

                // Update IDom if changed
                if new_idom != block_list[i].idom {
                    block_list[i].idom = new_idom;
                    changed = true;
                }
            }
        }
    }

    /// Find common dominator of two blocks using postorder numbers.
    fn intersect_dominators(
        &self,
        block1: BlockEntity,
        block2: BlockEntity,
        block_list: &[BlockInfo],
    ) -> BlockEntity {
        let mut b1 = block1;
        let mut b2 = block2;

        while b1 != b2 {
            let info1 = block_list.iter().find(|b| b.block == b1);
            let info2 = block_list.iter().find(|b| b.block == b2);

            let num1 = info1.map(|i| i.postorder_num).unwrap_or(0);
            let num2 = info2.map(|i| i.postorder_num).unwrap_or(0);

            if num1 < num2 {
                match info1 {
                    Some(info) => match info.idom {
                        Some(idom) => b1 = idom,
                        None => return b2,
                    },
                    None => return b2,
                }
            } else {
                match info2 {
                    Some(info) => match info.idom {
                        Some(idom) => b2 = idom,
                        None => return b1,
                    },
                    None => return b1,
                }
            }
        }

        b1
    }

    /// Determine where PHIs are needed using dominance frontiers.
    fn find_phi_placement(&self, block_list: &mut [BlockInfo]) {
        let mut changed = true;
        while changed {
            changed = false;

            // Iterate in reverse postorder
            for i in (0..block_list.len()).rev() {
                let info = &block_list[i];

                // If this block already needs a PHI, skip
                if info.def_block == Some(info.block) {
                    continue;
                }

                // Default to use same def as immediate dominator
                let idom_info = info
                    .idom
                    .and_then(|idom| block_list.iter().find(|b| b.block == idom));
                let mut new_def_block = idom_info.and_then(|i| i.def_block);

                // Check if any predecessor is in dominance frontier of a definition
                for pred_block in &info.preds {
                    let pred_info = block_list.iter().find(|b| b.block == *pred_block);
                    if let Some(pred) = pred_info {
                        if self.is_def_in_dom_frontier(pred, info.idom, block_list) {
                            // Need a PHI here
                            new_def_block = Some(info.block);
                            break;
                        }
                    }
                }

                // Update if changed
                if new_def_block != block_list[i].def_block {
                    block_list[i].def_block = new_def_block;
                    changed = true;
                }
            }
        }
    }

    /// Check if a definition is in the dominance frontier.
    ///
    /// A block is in the dominance frontier of Def if:
    /// - Def dominates a predecessor of the block
    /// - Def does not dominate the block itself
    fn is_def_in_dom_frontier(
        &self,
        pred: &BlockInfo,
        idom: Option<BlockEntity>,
        block_list: &[BlockInfo],
    ) -> bool {
        let mut current = pred.block;
        let idom_block = match idom {
            Some(idom) => idom,
            None => return false,
        };

        // Walk up from pred to idom, checking for definitions
        while current != idom_block {
            let info = block_list.iter().find(|b| b.block == current);
            match info {
                Some(info) => {
                    if info.def_block == Some(info.block) {
                        return true;
                    }
                    match info.idom {
                        Some(idom) => current = idom,
                        None => return false,
                    }
                }
                None => return false,
            }
        }

        false
    }

    /// Compute available values for each block.
    ///
    /// This determines what value is available at each block.
    /// Based on LLVM's FindAvailableVals algorithm:
    /// 1. Forward pass: Set available values for non-PHI blocks from their IDom
    /// 2. Reverse pass: For PHI blocks, compute values from all predecessors
    fn find_available_values(&mut self, block_list: &mut [BlockInfo]) -> GlslResult<()> {
        // Forward pass: Compute available values for non-PHI blocks
        // Iterate in reverse postorder (forward on CFG)
        for i in (0..block_list.len()).rev() {
            if block_list[i].def_block == Some(block_list[i].block) {
                // This block needs a PHI - we'll compute the value in the reverse pass
                // For now, leave it as None
                continue;
            } else {
                // Use value from immediate dominator
                if let Some(idom) = block_list[i].idom {
                    let idom_info = block_list.iter().find(|b| b.block == idom);
                    if let Some(idom_info) = idom_info {
                        block_list[i].available_val = idom_info.available_val;
                    }
                }
            }
        }

        // Reverse pass: For blocks that need PHIs, compute values from all predecessors
        // Iterate in forward postorder (backward on CFG) to ensure predecessors are processed first
        for i in 0..block_list.len() {
            if block_list[i].def_block == Some(block_list[i].block) {
                // This block needs a PHI - compute value from all predecessors
                // For each predecessor, get the value available at the end of that predecessor
                let mut pred_values = Vec::new();
                for pred_block in &block_list[i].preds {
                    // Find the predecessor's BlockInfo
                    if let Some(pred_info) = block_list.iter().find(|b| b.block == *pred_block) {
                        // Get the value available at the end of the predecessor
                        // This is either:
                        // 1. A definition in the predecessor block itself
                        // 2. The value from the predecessor's def_block (which may be its IDom)
                        if let Some(pred_def_block) = pred_info.def_block {
                            // Find the BlockInfo for the def_block
                            if let Some(def_info) =
                                block_list.iter().find(|b| b.block == pred_def_block)
                            {
                                if let Some(val) = def_info.available_val {
                                    pred_values.push(val);
                                    continue;
                                }
                            }
                        }
                        // Fallback: check if there's a direct definition in the predecessor
                        if let Some(def_val) =
                            self.defs.values().find_map(|m| m.get(pred_block)).copied()
                        {
                            pred_values.push(def_val);
                        } else if let Some(val) = pred_info.available_val {
                            pred_values.push(val);
                        }
                    }
                }

                // For PHI blocks, we need to determine the value from all predecessors
                // Since we're computing this lazily, we'll use the value from the first
                // predecessor that has a value. The actual PHI will be created manually
                // in control flow codegen with all predecessor values.
                if let Some(first_val) = pred_values.first().copied() {
                    block_list[i].available_val = Some(first_val);
                } else {
                    // No predecessor has a value - this shouldn't happen in valid SSA
                    // but we'll leave it as None
                }
            }
        }

        Ok(())
    }

    /// Get the value defined in a specific block (simple lookup, no dominance).
    ///
    /// This is a simple lookup - for dominance-aware value lookup, use
    /// `get_value_at_end_of_block` instead.
    pub fn get_value(&self, var: &str, block: BlockEntity) -> Option<Value> {
        self.defs.get(var).and_then(|m| m.get(&block)).copied()
    }

    /// Check if a variable needs phi nodes.
    pub fn needs_phi(&self, var: &str) -> bool {
        self.needs_phi.contains(var)
    }

    /// Get all variables that need phi nodes.
    pub fn variables_needing_phi(&self) -> &BTreeSet<String> {
        &self.needs_phi
    }
}

impl Default for SSABuilder {
    fn default() -> Self {
        Self::new()
    }
}
