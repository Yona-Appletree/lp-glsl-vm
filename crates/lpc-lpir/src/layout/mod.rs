//! Function layout (block and instruction ordering).
//!
//! The Layout struct manages the ordering of blocks and instructions in a function.
//! It uses doubly-linked lists to enable O(1) insertion and deletion, which is
//! essential for efficient optimizations.
//!
//! Layout is separate from the actual instruction data (stored in DFG). Layout
//! only tracks WHERE instructions are, not WHAT they are.

use core::cmp::Ordering;

use crate::{
    entity::{Block, Inst},
    entity_map::PrimaryMap,
    layout::{
        block_node::BlockNode,
        inst_node::InstNode,
        packed_option::PackedOption,
        sequence::{midpoint, SequenceNumber, LOCAL_LIMIT, MAJOR_STRIDE, MINOR_STRIDE},
    },
};

pub mod block_node;
pub mod inst_node;
pub mod packed_option;
pub mod sequence;

/// Layout manages the ordering of blocks and instructions
///
/// This is separate from the actual instruction data (in DFG).
/// Layout only tracks WHERE instructions are, not WHAT they are.
#[derive(Debug, Clone)]
pub struct Layout {
    /// Linked list nodes for blocks
    blocks: PrimaryMap<Block, BlockNode>,
    /// Linked list nodes for instructions
    insts: PrimaryMap<Inst, InstNode>,
    /// First block in layout order
    first_block: Option<Block>,
    /// Last block in layout order
    last_block: Option<Block>,
}

impl Layout {
    /// Create a new empty layout
    pub fn new() -> Self {
        Self {
            blocks: PrimaryMap::new(),
            insts: PrimaryMap::new(),
            first_block: None,
            last_block: None,
        }
    }

    /// Clear the layout
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.insts.clear();
        self.first_block = None;
        self.last_block = None;
    }

    // ========================================================================
    // Block operations
    // ========================================================================

    /// Check if a block is currently inserted in the layout
    pub fn is_block_inserted(&self, block: Block) -> bool {
        Some(block) == self.first_block
            || self
                .blocks
                .get(block)
                .map(|n| n.prev.is_some())
                .unwrap_or(false)
    }

    /// Append a block to the end of the layout
    pub fn append_block(&mut self, block: Block) {
        debug_assert!(
            !self.is_block_inserted(block),
            "Cannot append block that is already in the layout"
        );

        let node = self
            .blocks
            .get_mut(block)
            .expect("Block must exist in blocks map");
        debug_assert!(node.first_inst.is_none() && node.last_inst.is_none());

        node.prev = self.last_block.into();
        node.next = PackedOption::none();

        if let Some(last) = self.last_block {
            self.blocks.get_mut(last).unwrap().next = PackedOption::some(block);
        } else {
            self.first_block = Some(block);
        }
        self.last_block = Some(block);
    }

    /// Insert a block before another block
    pub fn insert_block(&mut self, block: Block, before: Block) {
        debug_assert!(
            !self.is_block_inserted(block),
            "Cannot insert block that is already in the layout"
        );
        debug_assert!(
            self.is_block_inserted(before),
            "Insertion point block must be in the layout"
        );

        let after = self.blocks.get(before).unwrap().prev;
        {
            let node = self.blocks.get_mut(block).expect("Block must exist");
            node.next = PackedOption::some(before);
            node.prev = after;
        }

        self.blocks.get_mut(before).unwrap().prev = PackedOption::some(block);

        match after.expand() {
            None => self.first_block = Some(block),
            Some(a) => self.blocks.get_mut(a).unwrap().next = PackedOption::some(block),
        }
    }

    /// Insert a block after another block
    pub fn insert_block_after(&mut self, block: Block, after: Block) {
        debug_assert!(
            !self.is_block_inserted(block),
            "Cannot insert block that is already in the layout"
        );
        debug_assert!(
            self.is_block_inserted(after),
            "Insertion point block must be in the layout"
        );

        let before = self.blocks.get(after).unwrap().next;
        {
            let node = self.blocks.get_mut(block).expect("Block must exist");
            node.next = before;
            node.prev = PackedOption::some(after);
        }

        self.blocks.get_mut(after).unwrap().next = PackedOption::some(block);

        match before.expand() {
            None => self.last_block = Some(block),
            Some(b) => self.blocks.get_mut(b).unwrap().prev = PackedOption::some(block),
        }
    }

    /// Remove a block from the layout
    ///
    /// The block must be empty (have no instructions).
    pub fn remove_block(&mut self, block: Block) {
        debug_assert!(
            self.is_block_inserted(block),
            "Cannot remove block that is not in the layout"
        );
        debug_assert!(
            self.blocks.get(block).map(|n| n.is_empty()).unwrap_or(true),
            "Cannot remove block that contains instructions"
        );

        let prev = self.blocks.get(block).unwrap().prev;
        let next = self.blocks.get(block).unwrap().next;

        {
            let node = self.blocks.get_mut(block).unwrap();
            node.prev = PackedOption::none();
            node.next = PackedOption::none();
        }

        match prev.expand() {
            None => self.first_block = next.expand(),
            Some(p) => self.blocks.get_mut(p).unwrap().next = next,
        }

        match next.expand() {
            None => self.last_block = prev.expand(),
            Some(n) => self.blocks.get_mut(n).unwrap().prev = prev,
        }
    }

    /// Get an iterator over blocks in layout order
    pub fn blocks(&self) -> Blocks<'_> {
        Blocks {
            layout: self,
            next: self.first_block,
        }
    }

    /// Get the entry block (first block in layout order)
    pub fn entry_block(&self) -> Option<Block> {
        self.first_block
    }

    /// Get the last block in layout order
    pub fn last_block(&self) -> Option<Block> {
        self.last_block
    }

    /// Get the block preceding `block` in layout order
    pub fn prev_block(&self, block: Block) -> Option<Block> {
        self.blocks.get(block)?.prev.expand()
    }

    /// Get the block following `block` in layout order
    pub fn next_block(&self, block: Block) -> Option<Block> {
        self.blocks.get(block)?.next.expand()
    }

    /// Mark a block as "cold"
    ///
    /// Cold blocks are less frequently executed and can be placed
    /// out of the hot path during code generation.
    pub fn set_cold(&mut self, block: Block) {
        if let Some(node) = self.blocks.get_mut(block) {
            node.cold = true;
        }
    }

    /// Check if a block is marked as cold
    pub fn is_cold(&self, block: Block) -> bool {
        self.blocks.get(block).map(|n| n.cold).unwrap_or(false)
    }

    // ========================================================================
    // Instruction operations
    // ========================================================================

    /// Get the block containing an instruction
    pub fn inst_block(&self, inst: Inst) -> Option<Block> {
        self.insts.get(inst)?.block.expand()
    }

    /// Append an instruction to the end of a block
    pub fn append_inst(&mut self, inst: Inst, block: Block) {
        debug_assert!(
            self.inst_block(inst).is_none(),
            "Cannot append instruction that is already in the layout"
        );
        debug_assert!(
            self.is_block_inserted(block),
            "Cannot append instructions to block not in layout"
        );

        let block_node = self.blocks.get_mut(block).expect("Block must exist");
        let inst_node = self
            .insts
            .get_mut(inst)
            .expect("Instruction must exist in insts map");

        inst_node.block = PackedOption::some(block);
        inst_node.prev = block_node.last_inst;

        if block_node.first_inst.is_none() {
            block_node.first_inst = PackedOption::some(inst);
        } else {
            let last_inst = block_node.last_inst.expand().unwrap();
            self.insts.get_mut(last_inst).unwrap().next = PackedOption::some(inst);
        }

        block_node.last_inst = PackedOption::some(inst);

        self.assign_inst_seq(inst);
    }

    /// Insert an instruction before another instruction
    pub fn insert_inst(&mut self, inst: Inst, before: Inst) {
        debug_assert!(
            self.inst_block(inst).is_none(),
            "Cannot insert instruction that is already in the layout"
        );

        let block = self
            .inst_block(before)
            .expect("Instruction before insertion point must be in layout");
        let after = self.insts.get(before).unwrap().prev;

        {
            let inst_node = self.insts.get_mut(inst).expect("Instruction must exist");
            inst_node.block = PackedOption::some(block);
            inst_node.next = PackedOption::some(before);
            inst_node.prev = after;
        }

        self.insts.get_mut(before).unwrap().prev = PackedOption::some(inst);

        match after.expand() {
            None => {
                self.blocks.get_mut(block).unwrap().first_inst = PackedOption::some(inst);
            }
            Some(a) => {
                self.insts.get_mut(a).unwrap().next = PackedOption::some(inst);
            }
        }

        self.assign_inst_seq(inst);
    }

    /// Remove an instruction from the layout
    pub fn remove_inst(&mut self, inst: Inst) {
        let block = self
            .inst_block(inst)
            .expect("Instruction must be in layout to remove");

        let prev = self.insts.get(inst).unwrap().prev;
        let next = self.insts.get(inst).unwrap().next;

        {
            let inst_node = self.insts.get_mut(inst).unwrap();
            inst_node.block = PackedOption::none();
            inst_node.prev = PackedOption::none();
            inst_node.next = PackedOption::none();
        }

        match prev.expand() {
            None => {
                self.blocks.get_mut(block).unwrap().first_inst = next;
            }
            Some(p) => {
                self.insts.get_mut(p).unwrap().next = next;
            }
        }

        match next.expand() {
            None => {
                self.blocks.get_mut(block).unwrap().last_inst = prev;
            }
            Some(n) => {
                self.insts.get_mut(n).unwrap().prev = prev;
            }
        }
    }

    /// Get the first instruction in a block
    pub fn first_inst(&self, block: Block) -> Option<Inst> {
        self.blocks.get(block)?.first_inst.expand()
    }

    /// Get the last instruction in a block
    pub fn last_inst(&self, block: Block) -> Option<Inst> {
        self.blocks.get(block)?.last_inst.expand()
    }

    /// Get the instruction following `inst`
    pub fn next_inst(&self, inst: Inst) -> Option<Inst> {
        self.insts.get(inst)?.next.expand()
    }

    /// Get the instruction preceding `inst`
    pub fn prev_inst(&self, inst: Inst) -> Option<Inst> {
        self.insts.get(inst)?.prev.expand()
    }

    /// Get an iterator over instructions in a block
    pub fn block_insts(&self, block: Block) -> Insts<'_> {
        Insts {
            layout: self,
            head: self.first_inst(block),
            tail: self.last_inst(block),
        }
    }

    /// Check if a block contains exactly one instruction
    pub fn block_contains_exactly_one_inst(&self, block: Block) -> bool {
        if let Some(block_node) = self.blocks.get(block) {
            block_node.first_inst.is_some() && block_node.first_inst == block_node.last_inst
        } else {
            false
        }
    }

    /// Split the block containing `before` in two
    ///
    /// Insert `new_block` after the old block and move `before` and the
    /// following instructions to `new_block`.
    pub fn split_block(&mut self, new_block: Block, before: Inst) {
        let old_block = self
            .inst_block(before)
            .expect("Instruction must be in layout");
        debug_assert!(!self.is_block_inserted(new_block));

        // Insert new_block after old_block
        let next_block = self.blocks.get(old_block).unwrap().next;
        let last_inst = self.blocks.get(old_block).unwrap().last_inst;

        {
            let node = self
                .blocks
                .get_mut(new_block)
                .expect("New block must exist");
            node.prev = PackedOption::some(old_block);
            node.next = next_block;
            node.first_inst = PackedOption::some(before);
            node.last_inst = last_inst;
        }

        self.blocks.get_mut(old_block).unwrap().next = PackedOption::some(new_block);

        // Fix backwards link
        if Some(old_block) == self.last_block {
            self.last_block = Some(new_block);
        } else {
            let next = next_block.expand().unwrap();
            self.blocks.get_mut(next).unwrap().prev = PackedOption::some(new_block);
        }

        // Disconnect the instruction links
        let prev_inst = self.insts.get(before).unwrap().prev;
        self.insts.get_mut(before).unwrap().prev = PackedOption::none();
        self.blocks.get_mut(old_block).unwrap().last_inst = prev_inst;

        match prev_inst.expand() {
            None => {
                self.blocks.get_mut(old_block).unwrap().first_inst = PackedOption::none();
            }
            Some(pi) => {
                self.insts.get_mut(pi).unwrap().next = PackedOption::none();
            }
        }

        // Fix the instruction -> block pointers
        let mut opt_i = Some(before);
        while let Some(i) = opt_i {
            debug_assert_eq!(self.insts.get(i).unwrap().block.expand(), Some(old_block));
            self.insts.get_mut(i).unwrap().block = PackedOption::some(new_block);
            opt_i = self.insts.get(i).unwrap().next.expand();
        }
    }

    /// Compare two program points in the same block
    ///
    /// Returns `Ordering::Less` if `a` appears before `b` in program order.
    pub fn pp_cmp(&self, a: impl Into<ProgramPoint>, b: impl Into<ProgramPoint>) -> Ordering {
        let a = a.into();
        let b = b.into();

        let a_block = self.pp_block(a);
        let b_block = self.pp_block(b);
        debug_assert_eq!(a_block, b_block, "Program points must be in the same block");

        let a_seq = match a {
            ProgramPoint::Block(_) => 0,
            ProgramPoint::Inst(inst) => self.insts.get(inst).map(|n| n.seq).unwrap_or(0),
        };

        let b_seq = match b {
            ProgramPoint::Block(_) => 0,
            ProgramPoint::Inst(inst) => self.insts.get(inst).map(|n| n.seq).unwrap_or(0),
        };

        a_seq.cmp(&b_seq)
    }

    /// Get the block containing a program point
    pub fn pp_block(&self, pp: ProgramPoint) -> Block {
        match pp {
            ProgramPoint::Block(block) => block,
            ProgramPoint::Inst(inst) => self
                .inst_block(inst)
                .expect("Program point must be in layout"),
        }
    }

    // ========================================================================
    // Internal: Sequence number management
    // ========================================================================

    /// Assign a sequence number to an instruction
    ///
    /// This may require renumbering if there's no room between the
    /// previous and next instruction.
    fn assign_inst_seq(&mut self, inst: Inst) {
        let inst_node = self.insts.get(inst).expect("Instruction must exist");
        let prev_seq = match inst_node.prev.expand() {
            Some(prev_inst) => self.insts.get(prev_inst).unwrap().seq,
            None => 0,
        };

        let next_seq = match inst_node.next.expand() {
            Some(next_inst) => self.insts.get(next_inst).unwrap().seq,
            None => {
                // No next instruction, use major stride
                self.insts.get_mut(inst).unwrap().seq = prev_seq + MAJOR_STRIDE;
                return;
            }
        };

        // Check if there's room between sequence numbers
        if let Some(seq) = midpoint(prev_seq, next_seq) {
            self.insts.get_mut(inst).unwrap().seq = seq;
        } else {
            // No room, need to renumber
            self.renumber_insts(inst, prev_seq + MINOR_STRIDE, prev_seq + LOCAL_LIMIT);
        }
    }

    /// Renumber instructions starting from `inst` until the end of the block
    ///
    /// If sequence numbers exceed `limit`, switch to a full block renumbering.
    fn renumber_insts(&mut self, inst: Inst, seq: SequenceNumber, limit: SequenceNumber) {
        let mut current_inst = inst;
        let mut current_seq = seq;

        loop {
            self.insts.get_mut(current_inst).unwrap().seq = current_seq;

            // Move to next instruction
            current_inst = match self.insts.get(current_inst).unwrap().next.expand() {
                None => return,
                Some(next) => next,
            };

            // Check if we've caught up to existing sequence numbers
            let existing_seq = self.insts.get(current_inst).unwrap().seq;
            if current_seq < existing_seq {
                // Sequence caught up, we're done
                return;
            }

            // Check if we've exceeded the limit
            if current_seq > limit {
                // Switch to full block renumbering
                let block = self
                    .inst_block(inst)
                    .expect("Instruction must be in layout");
                self.full_block_renumber(block);
                return;
            }

            current_seq += MINOR_STRIDE;
        }
    }

    /// Renumber all instructions in a block
    ///
    /// This gives more room in sequence numbers for future insertions.
    fn full_block_renumber(&mut self, block: Block) {
        let mut seq = MAJOR_STRIDE;
        let mut next_inst = self.first_inst(block);

        while let Some(inst) = next_inst {
            self.insts.get_mut(inst).unwrap().seq = seq;
            seq += MAJOR_STRIDE;
            next_inst = self.next_inst(inst);
        }
    }

    // ========================================================================
    // Internal: Map management
    // ========================================================================

    /// Ensure a block exists in the blocks map
    ///
    /// This is called when creating blocks to ensure they're registered.
    pub(crate) fn ensure_block(&mut self, block: Block) {
        if self.blocks.get(block).is_none() {
            let _ = self.blocks.push(BlockNode::new());
        }
    }

    /// Ensure an instruction exists in the insts map
    ///
    /// This is called when creating instructions to ensure they're registered.
    pub(crate) fn ensure_inst(&mut self, inst: Inst) {
        if self.insts.get(inst).is_none() {
            let _ = self.insts.push(InstNode::new());
        }
    }
}

/// Iterator over blocks in layout order
pub struct Blocks<'f> {
    layout: &'f Layout,
    next: Option<Block>,
}

impl<'f> Iterator for Blocks<'f> {
    type Item = Block;

    fn next(&mut self) -> Option<Block> {
        let current = self.next?;
        self.next = self.layout.next_block(current);
        Some(current)
    }
}

/// Iterator over instructions in a block
pub struct Insts<'f> {
    layout: &'f Layout,
    head: Option<Inst>,
    tail: Option<Inst>,
}

impl<'f> Iterator for Insts<'f> {
    type Item = Inst;

    fn next(&mut self) -> Option<Inst> {
        let current = self.head?;

        if Some(current) == self.tail {
            self.head = None;
            self.tail = None;
        } else {
            self.head = self.layout.next_inst(current);
        }

        Some(current)
    }
}

impl<'f> DoubleEndedIterator for Insts<'f> {
    fn next_back(&mut self) -> Option<Inst> {
        let current = self.tail?;

        if Some(current) == self.head {
            self.head = None;
            self.tail = None;
        } else {
            self.tail = self.layout.prev_inst(current);
        }

        Some(current)
    }
}

/// Program point (block or instruction)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramPoint {
    Block(Block),
    Inst(Inst),
}

impl From<Block> for ProgramPoint {
    fn from(block: Block) -> Self {
        ProgramPoint::Block(block)
    }
}

impl From<Inst> for ProgramPoint {
    fn from(inst: Inst) -> Self {
        ProgramPoint::Inst(inst)
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::*;

    #[test]
    fn test_layout_new() {
        let layout = Layout::new();
        assert_eq!(layout.entry_block(), None);
        assert_eq!(layout.last_block(), None);
        assert_eq!(layout.blocks().count(), 0);
    }

    #[test]
    fn test_layout_append_block() {
        let mut layout = Layout::new();
        let b1 = Block::new(0);
        let b2 = Block::new(1);

        layout.ensure_block(b1);
        layout.ensure_block(b2);

        layout.append_block(b1);
        layout.append_block(b2);

        assert!(layout.is_block_inserted(b1));
        assert!(layout.is_block_inserted(b2));
        assert_eq!(layout.entry_block(), Some(b1));

        let blocks: Vec<_> = layout.blocks().collect();
        assert_eq!(blocks, vec![b1, b2]);
    }

    #[test]
    fn test_layout_insert_block() {
        let mut layout = Layout::new();
        let b1 = Block::new(0);
        let b2 = Block::new(1);
        let b3 = Block::new(2);

        layout.ensure_block(b1);
        layout.ensure_block(b2);
        layout.ensure_block(b3);

        layout.append_block(b1);
        layout.append_block(b3);
        layout.insert_block(b2, b3);

        let blocks: Vec<_> = layout.blocks().collect();
        assert_eq!(blocks, vec![b1, b2, b3]);
    }

    #[test]
    fn test_layout_insert_block_after() {
        let mut layout = Layout::new();
        let b1 = Block::new(0);
        let b2 = Block::new(1);
        let b3 = Block::new(2);

        layout.ensure_block(b1);
        layout.ensure_block(b2);
        layout.ensure_block(b3);

        layout.append_block(b1);
        layout.append_block(b3);
        layout.insert_block_after(b2, b1);

        let blocks: Vec<_> = layout.blocks().collect();
        assert_eq!(blocks, vec![b1, b2, b3]);
    }

    #[test]
    fn test_layout_remove_block() {
        let mut layout = Layout::new();
        let b1 = Block::new(0);
        let b2 = Block::new(1);

        layout.ensure_block(b1);
        layout.ensure_block(b2);

        layout.append_block(b1);
        layout.append_block(b2);

        layout.remove_block(b1);

        assert!(!layout.is_block_inserted(b1));
        assert!(layout.is_block_inserted(b2));
        assert_eq!(layout.entry_block(), Some(b2));
    }

    #[test]
    fn test_layout_append_inst() {
        let mut layout = Layout::new();
        let b1 = Block::new(0);
        let i1 = Inst::new(0);
        let i2 = Inst::new(1);

        layout.ensure_block(b1);
        layout.ensure_inst(i1);
        layout.ensure_inst(i2);

        layout.append_block(b1);
        layout.append_inst(i1, b1);
        layout.append_inst(i2, b1);

        assert_eq!(layout.inst_block(i1), Some(b1));
        assert_eq!(layout.inst_block(i2), Some(b1));

        let insts: Vec<_> = layout.block_insts(b1).collect();
        assert_eq!(insts, vec![i1, i2]);
    }

    #[test]
    fn test_layout_insert_inst() {
        let mut layout = Layout::new();
        let b1 = Block::new(0);
        let i1 = Inst::new(0);
        let i2 = Inst::new(1);
        let i3 = Inst::new(2);

        layout.ensure_block(b1);
        layout.ensure_inst(i1);
        layout.ensure_inst(i2);
        layout.ensure_inst(i3);

        layout.append_block(b1);
        layout.append_inst(i1, b1);
        layout.append_inst(i3, b1);
        layout.insert_inst(i2, i3);

        let insts: Vec<_> = layout.block_insts(b1).collect();
        assert_eq!(insts, vec![i1, i2, i3]);
    }

    #[test]
    fn test_layout_remove_inst() {
        let mut layout = Layout::new();
        let b1 = Block::new(0);
        let i1 = Inst::new(0);
        let i2 = Inst::new(1);

        layout.ensure_block(b1);
        layout.ensure_inst(i1);
        layout.ensure_inst(i2);

        layout.append_block(b1);
        layout.append_inst(i1, b1);
        layout.append_inst(i2, b1);

        layout.remove_inst(i1);

        assert_eq!(layout.inst_block(i1), None);
        assert_eq!(layout.inst_block(i2), Some(b1));

        let insts: Vec<_> = layout.block_insts(b1).collect();
        assert_eq!(insts, vec![i2]);
    }

    #[test]
    fn test_layout_split_block() {
        let mut layout = Layout::new();
        let b1 = Block::new(0);
        let b2 = Block::new(1);
        let i1 = Inst::new(0);
        let i2 = Inst::new(1);
        let i3 = Inst::new(2);

        layout.ensure_block(b1);
        layout.ensure_block(b2);
        layout.ensure_inst(i1);
        layout.ensure_inst(i2);
        layout.ensure_inst(i3);

        layout.append_block(b1);
        layout.append_inst(i1, b1);
        layout.append_inst(i2, b1);
        layout.append_inst(i3, b1);

        layout.split_block(b2, i2);

        assert_eq!(layout.inst_block(i1), Some(b1));
        assert_eq!(layout.inst_block(i2), Some(b2));
        assert_eq!(layout.inst_block(i3), Some(b2));

        let insts1: Vec<_> = layout.block_insts(b1).collect();
        let insts2: Vec<_> = layout.block_insts(b2).collect();

        assert_eq!(insts1, vec![i1]);
        assert_eq!(insts2, vec![i2, i3]);
    }

    #[test]
    fn test_layout_pp_cmp() {
        let mut layout = Layout::new();
        let b1 = Block::new(0);
        let i1 = Inst::new(0);
        let i2 = Inst::new(1);

        layout.ensure_block(b1);
        layout.ensure_inst(i1);
        layout.ensure_inst(i2);

        layout.append_block(b1);
        layout.append_inst(i1, b1);
        layout.append_inst(i2, b1);

        assert_eq!(layout.pp_cmp(i1, i2), Ordering::Less);
        assert_eq!(layout.pp_cmp(i2, i1), Ordering::Greater);
        assert_eq!(layout.pp_cmp(i1, i1), Ordering::Equal);
    }
}
