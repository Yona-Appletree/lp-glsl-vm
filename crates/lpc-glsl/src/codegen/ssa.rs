//! SSA construction for GLSL codegen.
//!
//! This module provides automatic phi node insertion for proper SSA form,
//! similar to LLVM's SSAUpdater.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};

use lpc_lpir::{
    BlockEntity, ControlFlowGraph, DominatorTree, Function, FunctionBuilder, Type, Value,
};

use crate::error::GlslResult;

/// Builder for SSA form that handles phi node insertion automatically.
///
/// This tracks variable definitions per block and automatically inserts
/// phi nodes when variables are modified in multiple blocks.
pub struct SSABuilder {
    /// Map from variable name to map of block -> value
    /// Tracks the definition of each variable in each block
    defs: BTreeMap<String, BTreeMap<BlockEntity, Value>>,

    /// Set of variables that need phi nodes (modified in multiple blocks)
    needs_phi: BTreeSet<String>,

    /// Cached dominance tree (computed on demand)
    domtree: Option<DominatorTree>,

    /// Cached CFG (computed on demand)
    cfg: Option<ControlFlowGraph>,

    /// Block to index mapping (computed on demand)
    block_to_index: Option<BTreeMap<BlockEntity, usize>>,
}

impl SSABuilder {
    /// Create a new SSA builder.
    pub fn new() -> Self {
        Self {
            defs: BTreeMap::new(),
            needs_phi: BTreeSet::new(),
            domtree: None,
            cfg: None,
            block_to_index: None,
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

    /// Get the SSA value for a variable at a given block.
    ///
    /// This finds the value defined in this block, or the most recent definition
    /// in a dominating block. For phi node collection, this should return the
    /// value that's valid at the end of the given block.
    pub fn get_value(&self, var: &str, block: BlockEntity) -> Option<Value> {
        // Check if variable is defined in this block
        if let Some(value) = self.defs.get(var).and_then(|m| m.get(&block)) {
            return Some(*value);
        }

        // For now, return the most recent definition we can find
        // In a proper implementation, we'd walk up the dominator tree to find
        // the nearest dominating definition. For now, this is a best-effort
        // that will be validated by the dominance checker.
        if let Some(blocks) = self.defs.get(var) {
            // Return the last definition (most recent by block order)
            // This is a heuristic - proper implementation needs dominance analysis
            blocks.values().next_back().copied()
        } else {
            None
        }
    }

    /// Get the value defined in a specific block (for phi node collection).
    ///
    /// This is used when collecting values for phi nodes - we want the value
    /// that's defined at the end of the given block.
    pub fn get_value_in_block(&self, var: &str, block: BlockEntity) -> Option<Value> {
        self.defs.get(var).and_then(|m| m.get(&block)).copied()
    }

    /// Get a value that's valid at the given block, ensuring it's from a dominating definition.
    ///
    /// This finds a value that's defined in a block that dominates the given block.
    /// Uses cached dominance tree if available.
    pub fn get_dominating_value(&self, var: &str, target_block: BlockEntity) -> Option<Value> {
        let blocks = self.defs.get(var)?;

        // If we have dominance info, find a value from a dominating block
        if let (Some(domtree), Some(block_to_index)) = (&self.domtree, &self.block_to_index) {
            if let Some(&target_idx) = block_to_index.get(&target_block) {
                // Find a definition in a block that dominates target_block
                for (def_block, value) in blocks.iter() {
                    if let Some(&def_idx) = block_to_index.get(def_block) {
                        if domtree.dominates(def_idx, target_idx) {
                            return Some(*value);
                        }
                    }
                }
                // If no dominating definition found, return None
                return None;
            }
        }

        // No dominance info or block not found - return any definition
        // (will be validated by dominance checker)
        blocks.values().next().copied()
    }

    /// Compute dominance tree and CFG for the function.
    ///
    /// This should be called after the function structure is mostly complete.
    pub fn compute_dominance(&mut self, function: &Function) {
        let cfg = ControlFlowGraph::from_function(function);
        let domtree = DominatorTree::from_cfg(&cfg);

        // Build block to index mapping
        let block_to_index: BTreeMap<BlockEntity, usize> = function
            .blocks()
            .enumerate()
            .map(|(idx, b)| (b, idx))
            .collect();

        self.cfg = Some(cfg);
        self.domtree = Some(domtree);
        self.block_to_index = Some(block_to_index);
    }

    /// Finalize phi nodes for all variables that need them.
    ///
    /// This should be called after all code generation is complete.
    pub fn finalize_phi_nodes(&mut self, builder: &mut FunctionBuilder) -> GlslResult<()> {
        // Note: FunctionBuilder doesn't expose function() method directly.
        // This will need to be called after the function is built.
        // For now, this is a placeholder.
        // TODO: Implement proper phi node finalization

        // TODO: Insert phi nodes at merge points for variables that need them
        // This requires:
        // 1. Finding merge points (blocks with multiple predecessors)
        // 2. For each variable needing phi, collecting values from all predecessors
        // 3. Creating phi parameters and updating variable definitions

        Ok(())
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
