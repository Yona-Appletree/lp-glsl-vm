//! Dominance analysis using Cooper's "Simple, Fast Dominator Algorithm".

use alloc::{collections::BTreeSet, vec, vec::Vec};

use super::cfg::ControlFlowGraph;

/// Dominator tree for a function.
///
/// Computes dominance relationships between basic blocks using
/// Keith D. Cooper's "Simple, Fast Dominator Algorithm".
#[derive(Debug, Clone)]
pub struct DominatorTree {
    /// Immediate dominator for each block (None = unreachable or entry)
    idom: Vec<Option<usize>>,
    /// Reverse post-order numbers for efficient dominance queries
    rpo_numbers: Vec<u32>,
    /// Entry block index
    entry: usize,
    /// Number of blocks
    num_blocks: usize,
}

impl DominatorTree {
    /// Compute dominator tree from CFG.
    pub fn from_cfg(cfg: &ControlFlowGraph) -> Self {
        let num_blocks = cfg.num_blocks();
        let entry = cfg.entry();

        // Compute reverse post-order
        let rpo = cfg.reverse_post_order();

        // Build RPO number map: block -> RPO number
        let mut rpo_numbers = vec![0; num_blocks];
        for (rpo_idx, &block) in rpo.iter().enumerate() {
            // Use 1-based indexing, leaving 0 for unreachable
            rpo_numbers[block] = (rpo_idx + 1) as u32;
        }

        // Initialize immediate dominators
        // Entry has no dominator (None)
        // All other blocks initially point to their first predecessor (simple initialization)
        let mut idom = vec![None; num_blocks];
        for &block in &rpo {
            if block == entry {
                continue; // Entry has no dominator
            }

            // Find first reachable predecessor - use it as initial guess
            let reachable_preds: Vec<usize> = cfg
                .predecessors(block)
                .iter()
                .filter(|&&pred| rpo_numbers[pred] > 0)
                .copied()
                .collect();

            if !reachable_preds.is_empty() {
                // Initialize to first predecessor (will be refined in iteration)
                idom[block] = Some(reachable_preds[0]);
            }
        }

        // Iterate until fixed point (for irreducible CFG)
        // Add safety limit to prevent infinite loops
        const MAX_ITERATIONS: usize = 100;
        let mut iteration_count = 0;
        let mut changed = true;
        while changed && iteration_count < MAX_ITERATIONS {
            iteration_count += 1;
            changed = false;
            for &block in &rpo {
                if block == entry {
                    continue;
                }

                let reachable_preds: Vec<usize> = cfg
                    .predecessors(block)
                    .iter()
                    .filter(|&&pred| rpo_numbers[pred] > 0)
                    .copied()
                    .collect();

                if reachable_preds.is_empty() {
                    continue;
                }

                let mut candidate = reachable_preds[0];
                for &pred in reachable_preds.iter().skip(1) {
                    candidate = Self::common_dominator(candidate, pred, &idom, &rpo_numbers);
                }

                if idom[block] != Some(candidate) {
                    idom[block] = Some(candidate);
                    changed = true;
                }
            }
        }

        Self {
            idom,
            rpo_numbers,
            entry,
            num_blocks,
        }
    }

    /// Check if block_a dominates block_b.
    ///
    /// A block dominates itself. Entry block dominates all reachable blocks.
    pub fn dominates(&self, block_a: usize, block_b: usize) -> bool {
        if block_a >= self.num_blocks || block_b >= self.num_blocks {
            return false;
        }

        // A block always dominates itself
        if block_a == block_b {
            return true;
        }

        // Unreachable blocks don't dominate anything (except themselves)
        if self.rpo_numbers[block_a] == 0 {
            return false;
        }

        // Unreachable blocks aren't dominated by anything (except themselves)
        if self.rpo_numbers[block_b] == 0 {
            return false;
        }

        // Entry dominates all reachable blocks
        if block_a == self.entry {
            return true;
        }

        // Walk up dominator tree from block_b until we find block_a
        let rpo_a = self.rpo_numbers[block_a];
        let mut current = block_b;

        // Walk up while current's RPO number is greater than block_a's
        while self.rpo_numbers[current] > rpo_a {
            match self.idom[current] {
                Some(idom) => current = idom,
                None => return false, // Reached unreachable/entry without finding block_a
            }
        }

        // Check if we found block_a
        current == block_a
    }

    /// Get immediate dominator of a block.
    pub fn immediate_dominator(&self, block: usize) -> Option<usize> {
        if block >= self.num_blocks {
            return None;
        }
        self.idom[block]
    }

    /// Get all blocks dominated by a given block.
    ///
    /// This is computed by finding all blocks where walking up
    /// the dominator tree reaches the given block.
    pub fn dominated_blocks(&self, block: usize) -> BTreeSet<usize> {
        let mut dominated = BTreeSet::new();

        if block >= self.num_blocks {
            return dominated;
        }

        // A block always dominates itself
        dominated.insert(block);

        // Check all blocks to see if they're dominated
        for other_block in 0..self.num_blocks {
            if other_block != block && self.dominates(block, other_block) {
                dominated.insert(other_block);
            }
        }

        dominated
    }

    /// Compute common dominator of two blocks.
    ///
    /// The common dominator is the most recent common ancestor when walking up
    /// the dominator tree. If either block is unreachable, the function will
    /// return the reachable block (or entry if both are unreachable).
    ///
    /// # Preconditions
    ///
    /// This function is called internally during dominance computation with blocks
    /// that are known to be reachable (filtered before calling). However, it handles
    /// unreachable blocks gracefully by returning early when encountering None in
    /// the dominator tree.
    fn common_dominator(a: usize, b: usize, idom: &[Option<usize>], rpo_numbers: &[u32]) -> usize {
        // Debug assertion: both blocks should have valid RPO numbers when called
        // during normal dominance computation. This catches misuse in debug builds.
        debug_assert!(
            rpo_numbers[a] > 0 && rpo_numbers[b] > 0,
            "common_dominator called with unreachable blocks: a={}, b={}",
            a,
            b
        );

        let mut finger1 = a;
        let mut finger2 = b;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 1000; // Safety limit

        // Walk up both trees until they meet
        while finger1 != finger2 {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                // Safety: if we've iterated too much, something is wrong
                // Return the entry block (which dominates everything)
                return 0;
            }

            let rpo1 = rpo_numbers[finger1];
            let rpo2 = rpo_numbers[finger2];

            if rpo1 < rpo2 {
                // finger1 is higher in tree, move finger2 up
                match idom[finger2] {
                    Some(idom2) => finger2 = idom2,
                    None => {
                        // finger2 is entry or unreachable, use finger1
                        return finger1;
                    }
                }
            } else {
                // finger2 is higher in tree, move finger1 up
                match idom[finger1] {
                    Some(idom1) => finger1 = idom1,
                    None => {
                        // finger1 is entry or unreachable, use finger2
                        return finger2;
                    }
                }
            }
        }

        finger1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{block::Block, function::Function, inst::Inst, signature::Signature, value::Value};

    fn create_function() -> Function {
        Function::new(Signature::empty(), alloc::string::String::from("test"))
    }

    #[test]
    fn test_dominance_single_block() {
        let mut func = create_function();
        let mut block = Block::new();
        block.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block);

        let cfg = ControlFlowGraph::from_function(&func);
        let domtree = DominatorTree::from_cfg(&cfg);

        assert!(domtree.dominates(0, 0)); // Self-dominance
    }

    #[test]
    fn test_dominance_linear_chain() {
        let mut func = create_function();
        // block0 -> block1 -> block2
        let mut block0 = Block::new();
        block0.push_inst(Inst::Jump {
            target: 1,
            args: Vec::new(),
        });
        func.add_block(block0);

        let mut block1 = Block::new();
        block1.push_inst(Inst::Jump {
            target: 2,
            args: Vec::new(),
        });
        func.add_block(block1);

        let mut block2 = Block::new();
        block2.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block2);

        let cfg = ControlFlowGraph::from_function(&func);
        let domtree = DominatorTree::from_cfg(&cfg);

        // Entry dominates all
        assert!(domtree.dominates(0, 0));
        assert!(domtree.dominates(0, 1));
        assert!(domtree.dominates(0, 2));

        // block1 dominates block2
        assert!(domtree.dominates(1, 2));

        // block2 doesn't dominate others
        assert!(!domtree.dominates(2, 0));
        assert!(!domtree.dominates(2, 1));
    }

    #[test]
    fn test_dominance_diamond_pattern() {
        let mut func = create_function();
        // block0 -> block1, block2
        // block1 -> block3
        // block2 -> block3
        let mut block0 = Block::new();
        block0.push_inst(Inst::Br {
            condition: Value::new(0),
            target_true: 1,
            args_true: Vec::new(),
            target_false: 2,
            args_false: Vec::new(),
        });
        func.add_block(block0);

        let mut block1 = Block::new();
        block1.push_inst(Inst::Jump {
            target: 3,
            args: Vec::new(),
        });
        func.add_block(block1);

        let mut block2 = Block::new();
        block2.push_inst(Inst::Jump {
            target: 3,
            args: Vec::new(),
        });
        func.add_block(block2);

        let mut block3 = Block::new();
        block3.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block3);

        let cfg = ControlFlowGraph::from_function(&func);
        let domtree = DominatorTree::from_cfg(&cfg);

        // Entry dominates all
        assert!(domtree.dominates(0, 0));
        assert!(domtree.dominates(0, 1));
        assert!(domtree.dominates(0, 2));
        assert!(domtree.dominates(0, 3));

        // block0 dominates block3 (all paths go through block0)
        assert!(domtree.dominates(0, 3));

        // block1 doesn't dominate block3 (path through block2 doesn't go through block1)
        assert!(!domtree.dominates(1, 3));

        // block2 doesn't dominate block3 (path through block1 doesn't go through block2)
        assert!(!domtree.dominates(2, 3));
    }

    #[test]
    fn test_dominance_loop() {
        let mut func = create_function();
        // block0 -> block1
        // block1 -> block1 (loop), block2
        let mut block0 = Block::new();
        block0.push_inst(Inst::Jump {
            target: 1,
            args: Vec::new(),
        });
        func.add_block(block0);

        let mut block1 = Block::new();
        block1.push_inst(Inst::Br {
            condition: Value::new(0),
            target_true: 1, // Loop back
            args_true: Vec::new(),
            target_false: 2,
            args_false: Vec::new(),
        });
        func.add_block(block1);

        let mut block2 = Block::new();
        block2.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block2);

        let cfg = ControlFlowGraph::from_function(&func);
        let domtree = DominatorTree::from_cfg(&cfg);

        // Entry dominates all
        assert!(domtree.dominates(0, 1));
        assert!(domtree.dominates(0, 2));

        // block1 dominates block2 (all paths to block2 go through block1)
        assert!(domtree.dominates(1, 2));

        // block1 doesn't dominate itself via the loop (self-loop doesn't create self-dominance)
        // Actually, block1 does dominate itself (self-dominance), but the loop edge
        // doesn't affect the dominator tree structure
        assert!(domtree.dominates(1, 1)); // Self-dominance
    }

    #[test]
    fn test_dominance_immediate_dominator() {
        let mut func = create_function();
        // block0 -> block1 -> block2
        let mut block0 = Block::new();
        block0.push_inst(Inst::Jump {
            target: 1,
            args: Vec::new(),
        });
        func.add_block(block0);

        let mut block1 = Block::new();
        block1.push_inst(Inst::Jump {
            target: 2,
            args: Vec::new(),
        });
        func.add_block(block1);

        let mut block2 = Block::new();
        block2.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block2);

        let cfg = ControlFlowGraph::from_function(&func);
        let domtree = DominatorTree::from_cfg(&cfg);

        // Entry has no immediate dominator
        assert_eq!(domtree.immediate_dominator(0), None);

        // block1's immediate dominator is block0
        assert_eq!(domtree.immediate_dominator(1), Some(0));

        // block2's immediate dominator is block1
        assert_eq!(domtree.immediate_dominator(2), Some(1));
    }

    #[test]
    fn test_dominance_dominated_blocks() {
        let mut func = create_function();
        // block0 -> block1 -> block2
        let mut block0 = Block::new();
        block0.push_inst(Inst::Jump {
            target: 1,
            args: Vec::new(),
        });
        func.add_block(block0);

        let mut block1 = Block::new();
        block1.push_inst(Inst::Jump {
            target: 2,
            args: Vec::new(),
        });
        func.add_block(block1);

        let mut block2 = Block::new();
        block2.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block2);

        let cfg = ControlFlowGraph::from_function(&func);
        let domtree = DominatorTree::from_cfg(&cfg);

        let dominated_by_0 = domtree.dominated_blocks(0);
        assert!(dominated_by_0.contains(&0));
        assert!(dominated_by_0.contains(&1));
        assert!(dominated_by_0.contains(&2));

        let dominated_by_1 = domtree.dominated_blocks(1);
        assert!(dominated_by_1.contains(&1));
        assert!(dominated_by_1.contains(&2));
        assert!(!dominated_by_1.contains(&0));
    }

    #[test]
    fn test_dominance_unreachable_blocks() {
        let mut func = create_function();
        let mut block0 = Block::new();
        block0.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block0);

        // block1 is unreachable (no path from entry)
        let mut block1 = Block::new();
        block1.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block1);

        let cfg = ControlFlowGraph::from_function(&func);
        let domtree = DominatorTree::from_cfg(&cfg);

        // Unreachable block dominates itself
        assert!(domtree.dominates(1, 1));

        // Unreachable block doesn't dominate reachable blocks
        assert!(!domtree.dominates(1, 0));

        // Reachable blocks don't dominate unreachable blocks
        assert!(!domtree.dominates(0, 1));

        // Entry doesn't dominate unreachable blocks
        assert!(!domtree.dominates(0, 1));

        // Unreachable block has no immediate dominator
        assert_eq!(domtree.immediate_dominator(1), None);

        // Unreachable block only dominates itself
        let dominated_by_1 = domtree.dominated_blocks(1);
        assert_eq!(dominated_by_1.len(), 1);
        assert!(dominated_by_1.contains(&1));
    }

    #[test]
    fn test_dominance_nested_loops() {
        // Test dominance with nested loops
        // block0 -> block1 -> block2 -> block1 (inner loop)
        // block1 -> block3 -> block1 (outer loop)
        let mut func = create_function();
        let mut block0 = Block::new();
        block0.push_inst(Inst::Jump {
            target: 1,
            args: Vec::new(),
        });
        func.add_block(block0);

        let mut block1 = Block::new();
        block1.push_inst(Inst::Br {
            condition: Value::new(0),
            target_true: 2,
            args_true: Vec::new(),
            target_false: 3,
            args_false: Vec::new(),
        });
        func.add_block(block1);

        let mut block2 = Block::new();
        block2.push_inst(Inst::Jump {
            target: 1,
            args: Vec::new(),
        });
        func.add_block(block2);

        let mut block3 = Block::new();
        block3.push_inst(Inst::Br {
            condition: Value::new(0),
            target_true: 1,
            args_true: Vec::new(),
            target_false: 4,
            args_false: Vec::new(),
        });
        func.add_block(block3);

        let mut block4 = Block::new();
        block4.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block4);

        let cfg = ControlFlowGraph::from_function(&func);
        let domtree = DominatorTree::from_cfg(&cfg);

        // Entry dominates all
        assert!(domtree.dominates(0, 1));
        assert!(domtree.dominates(0, 2));
        assert!(domtree.dominates(0, 3));
        assert!(domtree.dominates(0, 4));

        // block1 dominates block2 and block3 (they're in loops controlled by block1)
        assert!(domtree.dominates(1, 2));
        assert!(domtree.dominates(1, 3));

        // block1 dominates block4 (all paths go through block1)
        assert!(domtree.dominates(1, 4));

        // block2 doesn't dominate block3 (path block1->block3 doesn't go through block2)
        assert!(!domtree.dominates(2, 3));
    }

    #[test]
    fn test_dominance_common_dominator_unreachable() {
        // Test that common_dominator handles unreachable blocks gracefully
        // (This tests the internal common_dominator function indirectly through dominance computation)
        let mut func = create_function();
        let mut block0 = Block::new();
        block0.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block0);

        // block1 is unreachable
        let mut block1 = Block::new();
        block1.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block1);

        // block2 is unreachable
        let mut block2 = Block::new();
        block2.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block2);

        let cfg = ControlFlowGraph::from_function(&func);
        let domtree = DominatorTree::from_cfg(&cfg);

        // Unreachable blocks don't dominate each other
        assert!(!domtree.dominates(1, 2));
        assert!(!domtree.dominates(2, 1));

        // Unreachable blocks have no immediate dominator
        assert_eq!(domtree.immediate_dominator(1), None);
        assert_eq!(domtree.immediate_dominator(2), None);
    }
}
