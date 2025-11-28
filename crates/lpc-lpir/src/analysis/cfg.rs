//! Control Flow Graph construction and analysis.

use alloc::{collections::BTreeSet, vec, vec::Vec};

use crate::{dfg::Opcode, entity::Block, function::Function};

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
        let num_blocks = func.block_count();
        let mut predecessors = vec![BTreeSet::new(); num_blocks];
        let mut successors = vec![BTreeSet::new(); num_blocks];

        // Build block index mapping
        let block_to_index: alloc::collections::BTreeMap<Block, usize> = func
            .blocks()
            .enumerate()
            .map(|(idx, block)| (block, idx))
            .collect();

        // Build CFG by examining jump and branch instructions
        for (block_idx, block) in func.blocks().enumerate() {
            for inst in func.block_insts(block) {
                if let Some(inst_data) = func.dfg.inst_data(inst) {
                    match inst_data.opcode {
                        Opcode::Jump | Opcode::Br => {
                            if let Some(block_args) = &inst_data.block_args {
                                for (target_block, _args) in &block_args.targets {
                                    if let Some(&target_idx) = block_to_index.get(target_block) {
                                        if target_idx < num_blocks {
                                            successors[block_idx].insert(target_idx);
                                            predecessors[target_idx].insert(block_idx);
                                        }
                                    }
                                }
                            }
                        }
                        Opcode::Return | Opcode::Halt => {
                            // These terminate the block, no successors
                        }
                        _ => {
                            // Other instructions don't affect control flow
                        }
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
    use crate::{dfg::InstData, function::Function, signature::Signature};

    fn create_function() -> Function {
        Function::new(Signature::empty(), alloc::string::String::from("test"))
    }

    #[test]
    fn test_cfg_single_block() {
        let mut func = create_function();
        let block = func.create_block();
        func.append_block(block);
        let inst = func.create_inst(InstData::return_(Vec::new()));
        func.append_inst(inst, block);

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
        let block0 = func.create_block();
        func.append_block(block0);
        let block1 = func.create_block();
        func.append_block(block1);
        let block2 = func.create_block();
        func.append_block(block2);

        let inst0 = func.create_inst(InstData::jump(block1, Vec::new()));
        func.append_inst(inst0, block0);

        let inst1 = func.create_inst(InstData::jump(block2, Vec::new()));
        func.append_inst(inst1, block1);

        let inst2 = func.create_inst(InstData::return_(Vec::new()));
        func.append_inst(inst2, block2);

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
        let block0 = func.create_block();
        func.append_block(block0);
        let block1 = func.create_block();
        func.append_block(block1);
        let block2 = func.create_block();
        func.append_block(block2);
        let block3 = func.create_block();
        func.append_block(block3);

        let cond = crate::value::Value::new(0);
        let inst0 = func.create_inst(InstData::branch(
            cond,
            block1,
            Vec::new(),
            block2,
            Vec::new(),
        ));
        func.append_inst(inst0, block0);

        let inst1 = func.create_inst(InstData::jump(block3, Vec::new()));
        func.append_inst(inst1, block1);

        let inst2 = func.create_inst(InstData::jump(block3, Vec::new()));
        func.append_inst(inst2, block2);

        let inst3 = func.create_inst(InstData::return_(Vec::new()));
        func.append_inst(inst3, block3);

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
        let block0 = func.create_block();
        func.append_block(block0);
        let block1 = func.create_block();
        func.append_block(block1);
        let block2 = func.create_block();
        func.append_block(block2);

        let inst0 = func.create_inst(InstData::jump(block1, Vec::new()));
        func.append_inst(inst0, block0);

        let cond = crate::value::Value::new(0);
        let inst1 = func.create_inst(InstData::branch(
            cond,
            block1, // Loop back
            Vec::new(),
            block2,
            Vec::new(),
        ));
        func.append_inst(inst1, block1);

        let inst2 = func.create_inst(InstData::return_(Vec::new()));
        func.append_inst(inst2, block2);

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
        let block0 = func.create_block();
        func.append_block(block0);
        let block1 = func.create_block();
        func.append_block(block1);
        let block2 = func.create_block();
        func.append_block(block2);
        let block3 = func.create_block();
        func.append_block(block3);

        let cond = crate::value::Value::new(0);
        let inst0 = func.create_inst(InstData::branch(
            cond,
            block1,
            Vec::new(),
            block2,
            Vec::new(),
        ));
        func.append_inst(inst0, block0);

        let inst1 = func.create_inst(InstData::jump(block3, Vec::new()));
        func.append_inst(inst1, block1);

        let inst2 = func.create_inst(InstData::jump(block3, Vec::new()));
        func.append_inst(inst2, block2);

        let inst3 = func.create_inst(InstData::return_(Vec::new()));
        func.append_inst(inst3, block3);

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
        let block0 = func.create_block();
        func.append_block(block0);
        let inst0 = func.create_inst(InstData::return_(Vec::new()));
        func.append_inst(inst0, block0);

        // block1 is unreachable (no path from entry)
        let block1 = func.create_block();
        func.append_block(block1);
        let inst1 = func.create_inst(InstData::return_(Vec::new()));
        func.append_inst(inst1, block1);

        let cfg = ControlFlowGraph::from_function(&func);
        assert!(cfg.is_reachable(0));
        assert!(!cfg.is_reachable(1));
    }
}
