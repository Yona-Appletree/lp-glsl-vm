//! Control Flow Graph construction and analysis.

use alloc::{collections::BTreeSet, vec, vec::Vec};

use crate::{function::Function, inst::Inst};

/// Control Flow Graph for a function.
///
/// Represents the control flow relationships between basic blocks.
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    /// Map from block index to set of predecessor block indices
    predecessors: Vec<BTreeSet<usize>>,
    /// Map from block index to set of successor block indices
    successors: Vec<BTreeSet<usize>>,
    /// Entry block index (always 0)
    entry: usize,
}

impl ControlFlowGraph {
    /// Build CFG from function.
    pub fn from_function(func: &Function) -> Self {
        let num_blocks = func.blocks.len();
        let mut predecessors = vec![BTreeSet::new(); num_blocks];
        let mut successors = vec![BTreeSet::new(); num_blocks];

        // Build CFG by examining jump and branch instructions
        for (block_idx, block) in func.blocks.iter().enumerate() {
            for inst in &block.insts {
                match inst {
                    Inst::Jump { target, .. } => {
                        let target_idx = *target as usize;
                        if target_idx < num_blocks {
                            successors[block_idx].insert(target_idx);
                            predecessors[target_idx].insert(block_idx);
                        }
                    }
                    Inst::Br {
                        target_true,
                        target_false,
                        ..
                    } => {
                        let true_idx = *target_true as usize;
                        let false_idx = *target_false as usize;
                        if true_idx < num_blocks {
                            successors[block_idx].insert(true_idx);
                            predecessors[true_idx].insert(block_idx);
                        }
                        if false_idx < num_blocks {
                            successors[block_idx].insert(false_idx);
                            predecessors[false_idx].insert(block_idx);
                        }
                    }
                    Inst::Return { .. } | Inst::Halt => {
                        // These terminate the block, no successors
                    }
                    _ => {
                        // Other instructions don't affect control flow
                    }
                }
            }
        }

        Self {
            predecessors,
            successors,
            entry: 0,
        }
    }

    /// Get predecessors of a block.
    pub fn predecessors(&self, block: usize) -> &BTreeSet<usize> {
        &self.predecessors[block]
    }

    /// Get successors of a block.
    pub fn successors(&self, block: usize) -> &BTreeSet<usize> {
        &self.successors[block]
    }

    /// Get all blocks in reverse post-order (for dominance computation).
    ///
    /// Uses DFS from entry block to compute post-order, then reverses it.
    pub fn reverse_post_order(&self) -> Vec<usize> {
        let mut visited = BTreeSet::new();
        let mut post_order = Vec::new();

        fn dfs(
            block: usize,
            cfg: &ControlFlowGraph,
            visited: &mut BTreeSet<usize>,
            post_order: &mut Vec<usize>,
        ) {
            if visited.contains(&block) {
                return;
            }
            visited.insert(block);

            // Visit successors in deterministic order
            let mut successors: Vec<usize> = cfg.successors(block).iter().copied().collect();
            successors.sort();

            for succ in successors {
                dfs(succ, cfg, visited, post_order);
            }

            post_order.push(block);
        }

        dfs(self.entry, self, &mut visited, &mut post_order);
        post_order.reverse();
        post_order
    }

    /// Check if block is reachable from entry.
    pub fn is_reachable(&self, block: usize) -> bool {
        if block >= self.predecessors.len() {
            return false;
        }
        if block == self.entry {
            return true;
        }

        let mut visited = BTreeSet::new();
        let mut worklist = vec![self.entry];
        visited.insert(self.entry);

        while let Some(current) = worklist.pop() {
            if current == block {
                return true;
            }

            for &succ in self.successors(current) {
                if visited.insert(succ) {
                    worklist.push(succ);
                }
            }
        }

        false
    }

    /// Get the entry block index.
    pub fn entry(&self) -> usize {
        self.entry
    }

    /// Get the number of blocks in the CFG.
    pub fn num_blocks(&self) -> usize {
        self.predecessors.len()
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
    fn test_cfg_single_block() {
        let mut func = create_function();
        let mut block = Block::new();
        block.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block);

        let cfg = ControlFlowGraph::from_function(&func);
        assert_eq!(cfg.num_blocks(), 1);
        assert_eq!(cfg.predecessors(0).len(), 0);
        assert_eq!(cfg.successors(0).len(), 0);
        assert!(cfg.is_reachable(0));
    }

    #[test]
    fn test_cfg_linear_chain() {
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
        assert_eq!(cfg.num_blocks(), 3);
        assert_eq!(cfg.predecessors(0).len(), 0); // Entry has no predecessors
        assert_eq!(cfg.predecessors(1).len(), 1);
        assert!(cfg.predecessors(1).contains(&0));
        assert_eq!(cfg.predecessors(2).len(), 1);
        assert!(cfg.predecessors(2).contains(&1));

        assert_eq!(cfg.successors(0).len(), 1);
        assert!(cfg.successors(0).contains(&1));
        assert_eq!(cfg.successors(1).len(), 1);
        assert!(cfg.successors(1).contains(&2));
        assert_eq!(cfg.successors(2).len(), 0);

        assert!(cfg.is_reachable(0));
        assert!(cfg.is_reachable(1));
        assert!(cfg.is_reachable(2));
    }

    #[test]
    fn test_cfg_diamond_pattern() {
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
        assert_eq!(cfg.num_blocks(), 4);
        assert_eq!(cfg.predecessors(3).len(), 2);
        assert!(cfg.predecessors(3).contains(&1));
        assert!(cfg.predecessors(3).contains(&2));

        assert_eq!(cfg.successors(0).len(), 2);
        assert!(cfg.successors(0).contains(&1));
        assert!(cfg.successors(0).contains(&2));
    }

    #[test]
    fn test_cfg_loop() {
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
        assert_eq!(cfg.num_blocks(), 3);
        assert_eq!(cfg.predecessors(1).len(), 2);
        assert!(cfg.predecessors(1).contains(&0));
        assert!(cfg.predecessors(1).contains(&1)); // Self-loop

        assert_eq!(cfg.successors(1).len(), 2);
        assert!(cfg.successors(1).contains(&1)); // Self-loop
        assert!(cfg.successors(1).contains(&2));
    }

    #[test]
    fn test_cfg_reverse_post_order() {
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
        let rpo = cfg.reverse_post_order();

        // Entry should be first
        assert_eq!(rpo[0], 0);
        // block3 should be last (all paths converge there)
        assert_eq!(rpo[rpo.len() - 1], 3);
        // All blocks should be present
        assert_eq!(rpo.len(), 4);
    }

    #[test]
    fn test_cfg_unreachable_block() {
        let mut func = create_function();
        let mut block0 = Block::new();
        block0.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block0);

        // block1 is unreachable (no path from entry)
        let mut block1 = Block::new();
        block1.push_inst(Inst::Return { values: Vec::new() });
        func.add_block(block1);

        let cfg = ControlFlowGraph::from_function(&func);
        assert!(cfg.is_reachable(0));
        assert!(!cfg.is_reachable(1));
    }
}
