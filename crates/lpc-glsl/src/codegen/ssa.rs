//! SSA construction for GLSL codegen.
//!
//! This module provides automatic phi node insertion for proper SSA form,
//! using lazy dominance computation similar to LLVM's SSAUpdater.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use lpc_lpir::{BlockEntity, ControlFlowGraph, Function, FunctionBuilder, Opcode, Type, Value};

use crate::error::{GlslError, GlslResult};

/// Sentinel value to represent undef (unreachable definitions)
/// Using u32::MAX as sentinel since it's unlikely to conflict with real values
fn undef_value() -> Value {
    Value::new(u32::MAX)
}

/// Check if a value is the undef sentinel
fn is_undef_value(val: Value) -> bool {
    val.index() == u32::MAX
}

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
        crate::debug!("[SSA] record_def: var='{}', block={:?}, value={:?}", var, block, value);
        
        // Check what's already recorded for this variable
        let blocks_before: Vec<_> = self.defs.get(var)
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();
        crate::debug!("[SSA] record_def: '{}' blocks before: {:?}", var, blocks_before);
        
        self.defs
            .entry(var.to_string())
            .or_insert_with(BTreeMap::new)
            .insert(block, value);

        // Verify it was recorded
        let blocks_after: Vec<_> = self.defs.get(var)
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();
        crate::debug!("[SSA] record_def: '{}' blocks after: {:?}", var, blocks_after);
        
        // Mark as needing phi if defined in multiple blocks
        if let Some(blocks) = self.defs.get(var) {
            if blocks.len() > 1 {
                self.needs_phi.insert(var.to_string());
                crate::debug!("[SSA] record_def: '{}' now needs phi (defined in {} blocks)", var, blocks.len());
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
        crate::debug!("[SSA] get_value_at_end_of_block: var='{}', block={:?}", var, block);
        
        // Check if we already have a value for this block
        if let Some(blocks_map) = self.defs.get(var) {
            let available_blocks: Vec<_> = blocks_map.keys().collect();
            crate::debug!("[SSA] get_value_at_end_of_block: '{}' available in blocks: {:?}", var, available_blocks);
            
            if let Some(value) = blocks_map.get(&block) {
                // Check if this block is terminated - if so, don't return its value
                // Terminated blocks don't dominate anything, so their values shouldn't be used
                if self.is_terminated_block(block, function) {
                    crate::debug!("[SSA] get_value_at_end_of_block: Block {:?} is terminated, returning None instead of value", block);
                    return Ok(None);
                }
                crate::debug!("[SSA] get_value_at_end_of_block: Found '{}' directly in block {:?}, value={:?}", var, block, value);
                return Ok(Some(*value));
            } else {
                crate::debug!("[SSA] get_value_at_end_of_block: '{}' not found directly in block {:?}, will use lazy dominance", var, block);
            }
        } else {
            crate::debug!("[SSA] get_value_at_end_of_block: '{}' has no definitions in SSABuilder", var);
        }
        
        if let Some(value) = self.defs.get(var).and_then(|m| m.get(&block)) {
            // Check if this block is terminated - if so, don't return its value
            // Terminated blocks don't dominate anything, so their values shouldn't be used
            if self.is_terminated_block(block, function) {
                crate::debug!("[SSA] get_value_at_end_of_block: Block {:?} is terminated, returning None instead of value", block);
                return Ok(None);
            }
            return Ok(Some(*value));
        }

        // Build block list backwards-reachable from target block
        let (mut block_list, pseudo_entry) = self.build_block_list(var, block, function)?;

        // Special case: unreachable block
        if block_list.is_empty() {
            return Ok(None);
        }

        // Sort block_list by postorder_num to ensure correct processing order
        // (higher postorder_num = processed later in reverse postorder)
        block_list.sort_by_key(|b| b.postorder_num);

        // Compute dominators for the subset
        self.find_dominators(&mut block_list, &pseudo_entry, function);

        // Find where PHIs are needed
        self.find_phi_placement(&mut block_list);

        // Compute available values
        crate::debug!("[SSA] find_available_values: block_list has {} blocks", block_list.len());
        for (idx, info) in block_list.iter().enumerate() {
            crate::debug!("[SSA] find_available_values: block_list[{}] = Block({:?}), available_val={:?}, def_block={:?}, preds={:?}", 
                idx, info.block, info.available_val, info.def_block, info.preds);
        }
        self.find_available_values(var, &mut block_list, function)?;
        crate::debug!("[SSA] After find_available_values:");
        for (idx, info) in block_list.iter().enumerate() {
            crate::debug!("[SSA] block_list[{}] = Block({:?}), available_val={:?}", 
                idx, info.block, info.available_val);
        }

        // Find the target block's info and return its value
        if let Some(info) = block_list.iter().find(|info| info.block == block) {
            // If the available value is undef, return None instead
            // This prevents passing undef values to PHI nodes, which the validator rejects
            if let Some(val) = info.available_val {
                if is_undef_value(val) {
                    crate::debug!("[SSA] Target block {:?} has undef value, returning None", block);
                    Ok(None)
                } else {
                    Ok(Some(val))
                }
            } else {
                Ok(None)
            }
        } else {
            crate::debug!("[SSA] Target block {:?} not found in block_list", block);
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
        let mut target_info = BlockInfo::new(target_block, target_val);
        // Set predecessors for target block immediately
        let target_pred_indices: Vec<usize> = cfg.predecessors(target_idx).iter().copied().collect();
        let target_preds: Vec<BlockEntity> = target_pred_indices
            .iter()
            .filter_map(|&idx| index_to_block.get(idx).copied())
            .collect();
        target_info.preds = target_preds.clone();
        target_info.num_preds = target_preds.len();
        block_map.insert(target_block, target_info);
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
        // Start from roots, but also include target block if it's not a root
        let mut worklist: Vec<BlockEntity> = root_list.clone();
        if !root_list.contains(&target_block) {
            // Target block doesn't define the variable, so add it to worklist
            worklist.push(target_block);
        }
        let mut postorder_num = 1u32;
        let mut visited: BTreeSet<BlockEntity> = BTreeSet::new();

        // Mark roots as visited and mark them as processing
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
            let is_root = root_list.contains(&current);

            if current_info.postorder_num == 0 || (is_root && current_info.postorder_num == u32::MAX) {
                // Not yet visited - mark as processing
                // For roots, they're already marked with u32::MAX, so we need to handle them specially
                if current_info.postorder_num == 0 {
                    current_info.postorder_num = u32::MAX;
                    worklist.push(current);
                    visited.insert(current);
                }

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
                // Roots will be added later with their IDoms set, but they need postorder numbers
                if current_info.available_val.is_none() {
                    block_list.push(block_map.remove(&current).unwrap());
                }
                // Roots stay in block_map with their postorder numbers assigned
            }
        }

        // Ensure all roots have postorder numbers assigned
        // If a root wasn't processed (no successors), assign it a number now
        for root in &root_list {
            if let Some(info) = block_map.get_mut(root) {
                if info.postorder_num == u32::MAX {
                    info.postorder_num = postorder_num;
                    postorder_num += 1;
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
        // First, add all non-root blocks that are in block_map but not yet in block_list
        // (these are blocks that were visited during backwards traversal but aren't roots)
        let mut all_blocks: Vec<BlockInfo> = block_list;
        let mut blocks_to_add: Vec<BlockInfo> = Vec::new();
        for (block, info) in block_map.iter() {
            // Check if this block is already in all_blocks
            if !all_blocks.iter().any(|b| b.block == *block) {
                // This block was visited but not added - add it now
                // This can happen for blocks that are predecessors but don't define the variable
                blocks_to_add.push(BlockInfo {
                    block: info.block,
                    available_val: info.available_val,
                    def_block: info.def_block,
                    postorder_num: info.postorder_num,
                    idom: info.idom,
                    num_preds: info.num_preds,
                    preds: info.preds.clone(),
                    phi: info.phi,
                });
            }
        }
        // Add the blocks we collected
        all_blocks.extend(blocks_to_add);
        // Then add roots (which define the variable)
        for root in root_list {
            if let Some(info) = block_map.remove(&root) {
                // Check if already added
                if !all_blocks.iter().any(|b| b.block == root) {
                    all_blocks.push(info);
                }
            }
        }

        Ok((all_blocks, pseudo_entry))
    }

    /// Check if a block ends with a return or halt instruction (is terminated).
    fn is_terminated_block(&self, block: BlockEntity, function: &Function) -> bool {
        let insts: Vec<_> = function.block_insts(block).collect();
        if let Some(last_inst) = insts.last() {
            if let Some(inst_data) = function.dfg.inst_data(*last_inst) {
                matches!(inst_data.opcode, Opcode::Return | Opcode::Halt)
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Compute immediate dominators using Cooper-Harvey-Kennedy algorithm.
    fn find_dominators(&self, block_list: &mut [BlockInfo], pseudo_entry: &BlockInfo, function: &Function) {
        // Create block-to-index map for O(1) lookups
        let mut block_to_index: BTreeMap<BlockEntity, usize> = BTreeMap::new();
        for (idx, info) in block_list.iter().enumerate() {
            block_to_index.insert(info.block, idx);
        }

        // Track unreachable terminated blocks to assign proper postorder numbers
        let mut unreachable_count = 0u32;

        let mut changed = true;
        while changed {
            changed = false;

            // Iterate in reverse postorder (forward on CFG edges)
            // Process blocks in order of decreasing postorder number
            // Since block_list is already sorted by postorder_num (ascending),
            // we iterate in reverse to get reverse postorder
            let mut indices: Vec<usize> = (0..block_list.len()).collect();
            indices.sort_by(|&a, &b| {
                // Sort by postorder_num in descending order (reverse postorder)
                block_list[b].postorder_num.cmp(&block_list[a].postorder_num)
            });

            for i in indices {
                if block_list[i].postorder_num == pseudo_entry.postorder_num {
                    continue; // Skip pseudo-entry
                }

                let mut new_idom: Option<BlockEntity> = None;

                // Find IDom as intersection of all predecessors' IDoms
                // According to LLVM's SSAUpdaterImpl.h, we use the predecessor block itself,
                // not pred.idom. The intersect_dominators function will walk up the IDom chain.
                for pred_block in &block_list[i].preds {
                    // Look up predecessor in block_list using the map
                    let pred_idx = match block_to_index.get(pred_block) {
                        Some(&idx) => idx,
                        None => {
                            // Predecessor not in block_list - this can happen if it's a root
                            // (defines the variable) or if it's not backwards-reachable.
                            // According to LLVM, all backwards-reachable predecessors should be in BBMap.
                            // For now, skip it - roots are handled separately with IDom = pseudo_entry
                            continue;
                        }
                    };

                    // Treat an unreachable predecessor as a definition (per LLVM SSAUpdaterImpl.h lines 242-249)
                    if block_list[pred_idx].postorder_num == 0 {
                        // Check if this is a terminated block (unreachable from forward traversal)
                        if self.is_terminated_block(block_list[pred_idx].block, function) {
                            // Treat as unreachable definition with undef (per LLVM lines 242-249)
                            // In LLVM: Pred->AvailableVal = Traits::GetUndefVal(Pred->BB, Updater)
                            //          Pred->DefBB = Pred
                            //          Pred->BlkNum = PseudoEntry->BlkNum; PseudoEntry->BlkNum++
                            block_list[pred_idx].available_val = Some(undef_value());
                            block_list[pred_idx].def_block = Some(block_list[pred_idx].block);
                            // Assign postorder number from pseudo-entry (per LLVM)
                            // In LLVM: Pred->BlkNum = PseudoEntry->BlkNum; PseudoEntry->BlkNum++
                            block_list[pred_idx].postorder_num = pseudo_entry.postorder_num + unreachable_count;
                            unreachable_count += 1;
                            // Continue to use it in IDom computation
                        } else {
                            // Not terminated - truly unreachable, skip
                            continue;
                        }
                    }
                    
                    let pred = &block_list[pred_idx];

                    if new_idom.is_none() {
                        // Start with predecessor block itself
                        new_idom = Some(pred.block);
                    } else {
                        // Intersect with predecessor: use pred.block as the candidate
                        // IntersectDominators will walk up the dominator tree
                        new_idom = Some(self.intersect_dominators(
                            new_idom.unwrap(),
                            pred.block,
                            block_list,
                            &block_to_index,
                        ));
                    }
                }
                
                // If we still don't have an IDom and this block has predecessors, that's a problem
                // (unless all predecessors were unreachable or roots, which are handled above)
                if new_idom.is_none() && !block_list[i].preds.is_empty() {
                    // This shouldn't happen - all backwards-reachable predecessors should be in block_list
                    // But we'll leave it as None and let the algorithm handle it
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
    /// Optimized version using block-to-index map for O(1) lookups.
    fn intersect_dominators(
        &self,
        block1: BlockEntity,
        block2: BlockEntity,
        block_list: &[BlockInfo],
        block_to_index: &BTreeMap<BlockEntity, usize>,
    ) -> BlockEntity {
        let mut b1 = block1;
        let mut b2 = block2;

        while b1 != b2 {
            // Use block_to_index map for O(1) lookups instead of linear search
            let idx1 = block_to_index.get(&b1);
            let idx2 = block_to_index.get(&b2);

            let num1 = idx1.and_then(|&i| block_list.get(i)).map(|i| i.postorder_num).unwrap_or(0);
            let num2 = idx2.and_then(|&i| block_list.get(i)).map(|i| i.postorder_num).unwrap_or(0);

            if num1 < num2 {
                match idx1.and_then(|&i| block_list.get(i)) {
                    Some(info) => match info.idom {
                        Some(idom) => b1 = idom,
                        None => return b2,
                    },
                    None => return b2,
                }
            } else {
                match idx2.and_then(|&i| block_list.get(i)) {
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
        // Create block-to-index map for O(1) lookups
        let mut block_to_index: BTreeMap<BlockEntity, usize> = BTreeMap::new();
        for (idx, info) in block_list.iter().enumerate() {
            block_to_index.insert(info.block, idx);
        }

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
                // Per LLVM SSAUpdaterImpl.h line 296: Info->IDom->DefBB
                let mut new_def_block = None;
                if let Some(idom) = info.idom {
                    if let Some(&idom_idx) = block_to_index.get(&idom) {
                        if let Some(idom_info) = block_list.get(idom_idx) {
                            new_def_block = idom_info.def_block;
                        }
                    }
                    // If IDom is not in block_list, it might be a root or pseudo-entry
                    // In that case, new_def_block stays None, which is handled below
                }
                // If IDom is None, this block has no dominator (shouldn't happen except for pseudo-entry)
                // In that case, new_def_block stays None

                // Check if any predecessor is in dominance frontier of a definition
                for pred_block in &info.preds {
                    if let Some(&pred_idx) = block_to_index.get(pred_block) {
                        if let Some(pred) = block_list.get(pred_idx) {
                            if self.is_def_in_dom_frontier(pred, info.idom, block_list, &block_to_index) {
                                // Need a PHI here
                                new_def_block = Some(info.block);
                                break;
                            }
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
        block_to_index: &BTreeMap<BlockEntity, usize>,
    ) -> bool {
        let mut current = pred.block;
        let idom_block = match idom {
            Some(idom) => idom,
            None => return false,
        };

        // Walk up from pred to idom, checking for definitions
        while current != idom_block {
            if let Some(&idx) = block_to_index.get(&current) {
                if let Some(info) = block_list.get(idx) {
                    if info.def_block == Some(info.block) {
                        return true;
                    }
                    match info.idom {
                        Some(idom) => current = idom,
                        None => return false,
                    }
                } else {
                    return false;
                }
            } else {
                return false;
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
    ///
    /// `var` is the variable name we're computing values for.
    /// `function` is needed to check if blocks are terminated.
    fn find_available_values(&mut self, var: &str, block_list: &mut [BlockInfo], function: &Function) -> GlslResult<()> {
        // Create block-to-index map for O(1) lookups
        let mut block_to_index: BTreeMap<BlockEntity, usize> = BTreeMap::new();
        for (idx, info) in block_list.iter().enumerate() {
            block_to_index.insert(info.block, idx);
        }

        // First, ensure all blocks that define the variable have their available_val set
        for i in 0..block_list.len() {
            if block_list[i].available_val.is_none() {
                // Check if this block directly defines THIS variable
                if let Some(def_val) = self.defs.get(var)
                    .and_then(|m| m.get(&block_list[i].block))
                    .copied() {
                    block_list[i].available_val = Some(def_val);
                    crate::debug!("[SSA] find_available_values: Set Block({:?}) available_val from direct def", 
                        block_list[i].block);
                }
            }
        }
        
        // Forward pass: Compute available values for non-PHI blocks
        // Iterate in reverse postorder (forward on CFG) to ensure predecessors are processed first
        for i in (0..block_list.len()).rev() {
            if block_list[i].def_block == Some(block_list[i].block) {
                // This block needs a PHI - we'll compute the value in the reverse pass
                continue;
            }

            // For blocks that don't define the variable, get value from immediate dominator
            // Per LLVM: Info->DefBB->AvailableVal (line 350)
            // Since DefBB != Info, we use DefBB's available value
            if let Some(def_block) = block_list[i].def_block {
                if let Some(&def_idx) = block_to_index.get(&def_block) {
                    if let Some(def_info) = block_list.get(def_idx) {
                        block_list[i].available_val = def_info.available_val;
                        crate::debug!("[SSA] find_available_values: Block({:?}) gets value from DefBB Block({:?})", 
                            block_list[i].block, def_block);
                        continue;
                    }
                }
            }

            // Fallback: use IDom if DefBB lookup failed
            if let Some(idom) = block_list[i].idom {
                if let Some(&idom_idx) = block_to_index.get(&idom) {
                    if let Some(idom_info) = block_list.get(idom_idx) {
                        block_list[i].available_val = idom_info.available_val;
                        crate::debug!("[SSA] find_available_values: Block({:?}) gets value from IDom Block({:?})", 
                            block_list[i].block, idom);
                    }
                }
            }
        }

        // Reverse pass: For blocks that need PHIs, compute values from all predecessors
        // Iterate in forward postorder (backward on CFG) to ensure predecessors are processed first
        for i in 0..block_list.len() {
            if block_list[i].def_block == Some(block_list[i].block) {
                // This block needs a PHI - compute value from all predecessors
                // Per LLVM: Skip to nearest preceding definition using PredInfo->DefBB (line 364-365)
                let mut pred_values = Vec::new();
                let phi_block = block_list[i].block;
                for pred_block in &block_list[i].preds {
                    // Skip terminated blocks that are not the PHI block itself
                    // Terminated blocks (return/halt) don't dominate anything, so their
                    // values shouldn't be used in PHI nodes
                    // Instead of using undef (which the validator rejects), we skip them entirely
                    if self.is_terminated_block(*pred_block, function) && *pred_block != phi_block {
                        // Skip terminated blocks - don't add any value for them
                        // This means the PHI will have fewer operands, but that's correct
                        // since terminated blocks don't contribute to the control flow
                        continue;
                    }
                    
                    // Find the predecessor's BlockInfo
                    if let Some(&pred_idx) = block_to_index.get(pred_block) {
                        if let Some(pred_info) = block_list.get(pred_idx) {
                            // Skip to the nearest preceding definition (per LLVM line 364-365)
                            let def_block = pred_info.def_block.unwrap_or(pred_info.block);
                            if let Some(&def_idx) = block_to_index.get(&def_block) {
                                if let Some(def_info) = block_list.get(def_idx) {
                                    if let Some(val) = def_info.available_val {
                                        pred_values.push(val);
                                        continue;
                                    }
                                }
                            }
                            // Fallback: check if there's a direct definition for THIS variable
                            // BUT: if pred_info has an available_val (including undef), use that instead
                            // This ensures we use undef for unreachable terminated blocks
                            if let Some(val) = pred_info.available_val {
                                // Include undef values - they represent unreachable definitions
                                // LLVM includes them in PHI operands
                                pred_values.push(val);
                            } else if let Some(def_val) =
                                self.defs.get(var).and_then(|m| m.get(pred_block)).copied()
                            {
                                pred_values.push(def_val);
                            }
                        }
                    } else {
                        // Predecessor not in block_list - might be a root
                        // Check if it has a direct definition
                        if let Some(def_val) =
                            self.defs.get(var).and_then(|m| m.get(pred_block)).copied()
                        {
                            pred_values.push(def_val);
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

    /// Debug helper: Get all blocks where a variable is defined.
    pub fn debug_get_blocks_for_var(&self, var: &str) -> Vec<BlockEntity> {
        self.defs
            .get(var)
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default()
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

#[cfg(test)]
#[path = "ssa_tests.rs"]
mod ssa_tests;

