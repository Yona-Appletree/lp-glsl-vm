//! VCode builder for constructing VCode incrementally

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use lpc_lpir::RelSourceLoc;

use crate::backend3::{
    types::{BlockIndex, InsnIndex, Range, Ranges, VReg},
    vcode::{
        BlockLoweringOrder, BlockMetadata, Callee, Constant, MachInst, RelocKind, VCode,
        VCodeConstants, VCodeReloc,
    },
};

/// Builder for constructing VCode incrementally
pub struct VCodeBuilder<I: MachInst> {
    /// Instructions being built
    insts: Vec<I>,
    /// Source locations (parallel to insts)
    srclocs: Vec<RelSourceLoc>,
    /// Constants
    constants: BTreeMap<VReg, Constant>,
    /// Relocations
    relocations: Vec<VCodeReloc>,
    /// Block metadata
    block_metadata: Vec<BlockMetadata>,
    /// Next virtual register index
    next_vreg: u32,
    /// Current block being built
    current_block: Option<BlockIndex>,
    /// Block instruction ranges (start index for each block)
    block_starts: Vec<usize>,
    /// Block parameter VRegs
    block_params: Vec<VReg>,
    /// Block parameter ranges
    block_params_range: Ranges,
    /// Branch block arguments
    branch_block_args: Vec<VReg>,
    /// Branch block arg ranges
    branch_block_arg_range: Ranges,
    /// Branch block arg succ ranges
    branch_block_arg_succ_range: Ranges,
}

impl<I: MachInst> VCodeBuilder<I> {
    /// Create a new VCodeBuilder
    pub fn new() -> Self {
        VCodeBuilder {
            insts: Vec::new(),
            srclocs: Vec::new(),
            constants: BTreeMap::new(),
            relocations: Vec::new(),
            block_metadata: Vec::new(),
            next_vreg: 0,
            current_block: None,
            block_starts: Vec::new(),
            block_params: Vec::new(),
            block_params_range: Ranges::new(),
            branch_block_args: Vec::new(),
            branch_block_arg_range: Ranges::new(),
            branch_block_arg_succ_range: Ranges::new(),
        }
    }

    /// Allocate a new virtual register
    pub fn alloc_vreg(&mut self) -> VReg {
        let vreg = VReg::new(self.next_vreg);
        self.next_vreg += 1;
        vreg
    }

    /// Push an instruction with source location
    pub fn push(&mut self, inst: I, srcloc: RelSourceLoc) {
        self.insts.push(inst);
        self.srclocs.push(srcloc);
    }

    /// Record a constant for a virtual register
    pub fn record_constant(&mut self, vreg: VReg, constant: Constant) {
        self.constants.insert(vreg, constant);
    }

    /// Record a relocation
    pub fn record_reloc(&mut self, inst_idx: InsnIndex, kind: RelocKind, target: String) {
        self.relocations.push(VCodeReloc {
            inst_idx,
            kind,
            target,
        });
    }

    /// Start a new block
    pub fn start_block(&mut self, block_idx: BlockIndex, params: Vec<VReg>) {
        self.current_block = Some(block_idx);
        let start_inst = self.insts.len();
        self.block_starts.push(start_inst);

        // Record block parameters
        let param_start = self.block_params.len();
        self.block_params.extend(params.iter().copied());
        let param_end = self.block_params.len();
        self.block_params_range
            .push(Range::new(param_start, param_end));
    }

    /// End the current block
    pub fn end_block(&mut self) {
        self.current_block = None;
    }

    /// Add branch arguments for a block's successors
    ///
    /// This should be called after end_block() to record the branch arguments
    /// for each successor of the block that was just ended.
    pub fn add_branch_args(&mut self, succs: &[BlockIndex], args_per_succ: &[Vec<VReg>]) {
        assert_eq!(succs.len(), args_per_succ.len());

        let succ_start = self.branch_block_arg_succ_range.len();
        for args in args_per_succ {
            let arg_start = self.branch_block_args.len();
            self.branch_block_args.extend(args.iter().copied());
            let arg_end = self.branch_block_args.len();
            self.branch_block_arg_range
                .push(Range::new(arg_start, arg_end));
        }
        let succ_end = self.branch_block_arg_succ_range.len();
        self.branch_block_arg_succ_range
            .push(Range::new(succ_start, succ_end));
    }

    /// Add block metadata
    pub fn add_block_metadata(&mut self, metadata: BlockMetadata) {
        self.block_metadata.push(metadata);
    }

    /// Compute predecessors from successors using counting sort
    ///
    /// This implements the inverse relationship: for each block that appears
    /// as a successor, we record which blocks have it as a successor.
    fn compute_preds_from_succs(
        num_blocks: usize,
        block_succ_range: &Ranges,
        block_succs: &[BlockIndex],
    ) -> (Ranges, Vec<BlockIndex>) {
        // Step 1: Count how many times each block appears as a successor
        let mut starts = Vec::with_capacity(num_blocks);
        starts.resize(num_blocks, 0u32);
        for succ in block_succs {
            let idx = succ.index() as usize;
            if idx < starts.len() {
                starts[idx] += 1;
            }
        }

        // Step 2: Determine starting positions for each block's predecessors
        let mut block_pred_range = Ranges::new();
        let mut end = 0;
        for count in starts.iter_mut() {
            let start = end;
            end += *count;
            *count = start;
            block_pred_range.push(Range::new(start as usize, end as usize));
        }
        let end = end as usize;

        // Step 3: Walk over successors again, pushing predecessors at correct positions
        let mut block_preds = Vec::with_capacity(end);
        block_preds.resize(end, BlockIndex::new(0));
        for (pred_idx, range) in block_succ_range.iter().enumerate() {
            let pred = BlockIndex::new(pred_idx as u32);
            for succ in &block_succs[range.start..range.end] {
                let succ_idx = succ.index() as usize;
                if succ_idx < starts.len() {
                    let pos = &mut starts[succ_idx];
                    if (*pos as usize) < block_preds.len() {
                        block_preds[*pos as usize] = pred;
                        *pos += 1;
                    }
                }
            }
        }

        (block_pred_range, block_preds)
    }

    /// Collect operands from all instructions
    ///
    /// This iterates over all instructions, calls `get_operands()` on each,
    /// and builds flat arrays for regalloc2.
    fn collect_operands(
        insts: &[I],
    ) -> (
        Vec<crate::backend3::vcode::Operand>,
        Ranges,
        BTreeMap<InsnIndex, crate::backend3::vcode::PRegSet>,
    ) {
        use crate::backend3::vcode::{Operand, OperandKind, OperandVisitor};

        struct OperandCollector {
            operands: Vec<Operand>,
        }

        impl OperandVisitor for OperandCollector {
            fn visit_use(
                &mut self,
                vreg: VReg,
                constraint: crate::backend3::vcode::OperandConstraint,
            ) {
                self.operands.push(Operand {
                    vreg,
                    constraint,
                    kind: OperandKind::Use,
                });
            }

            fn visit_def(
                &mut self,
                vreg: VReg,
                constraint: crate::backend3::vcode::OperandConstraint,
            ) {
                self.operands.push(Operand {
                    vreg,
                    constraint,
                    kind: OperandKind::Def,
                });
            }

            fn visit_mod(
                &mut self,
                vreg: VReg,
                constraint: crate::backend3::vcode::OperandConstraint,
            ) {
                self.operands.push(Operand {
                    vreg,
                    constraint,
                    kind: OperandKind::Mod,
                });
            }
        }

        let mut operands = Vec::new();
        let mut operand_ranges = Ranges::new();
        let mut clobbers = BTreeMap::new();

        for (idx, inst) in insts.iter().enumerate() {
            let mut collector = OperandCollector {
                operands: Vec::new(),
            };

            // Create mutable clone to call get_operands
            let mut inst_clone = inst.clone();
            inst_clone.get_operands(&mut collector);

            // Record operand range for this instruction
            let start = operands.len();
            operands.append(&mut collector.operands);
            let end = operands.len();
            operand_ranges.push(Range::new(start, end));

            // Collect clobbers
            if let Some(clobber_set) = inst_clone.get_clobbers() {
                clobbers.insert(InsnIndex::new(idx as u32), clobber_set);
            }
        }

        (operands, operand_ranges, clobbers)
    }

    /// Build the final VCode
    ///
    /// This consumes the builder and produces a VCode structure.
    /// The caller must provide the block order and ABI information.
    ///
    /// Note: `block_starts` contains one entry per lowered block (both original
    /// and edge blocks) in the order they were lowered. This matches the order
    /// in `block_order.lowered_order`.
    pub fn build(
        self,
        entry: BlockIndex,
        block_order: BlockLoweringOrder,
        abi: Callee<I::ABIMachineSpec>,
    ) -> VCode<I> {
        // Build block ranges from block_starts
        // Each entry in block_starts corresponds to one lowered block (original or edge)
        let mut block_ranges = Ranges::new();
        for i in 0..self.block_starts.len() {
            let start = self.block_starts[i];
            let end = if i + 1 < self.block_starts.len() {
                self.block_starts[i + 1]
            } else {
                self.insts.len()
            };
            block_ranges.push(Range::new(start, end));
        }

        // Populate block successors from block_order
        let mut block_succs = Vec::new();
        let mut block_succ_range = Ranges::new();
        for succ_list in &block_order.lowered_succs {
            let start = block_succs.len();
            block_succs.extend(succ_list.iter().copied());
            let end = block_succs.len();
            block_succ_range.push(Range::new(start, end));
        }

        // Compute predecessors from successors
        let (block_pred_range, block_preds) =
            Self::compute_preds_from_succs(block_ranges.len(), &block_succ_range, &block_succs);

        // Collect operands from instructions
        let (operands, operand_ranges, clobbers) = Self::collect_operands(&self.insts);

        // Build block metadata from block_order
        let mut block_metadata = Vec::new();
        for (idx, _lowered_block) in block_order.lowered_order.iter().enumerate() {
            let block_idx = BlockIndex::new(idx as u32);
            let cold = block_order.cold_blocks.contains(&block_idx);
            let indirect_target = block_order.indirect_targets.contains(&block_idx);
            block_metadata.push(BlockMetadata {
                cold,
                indirect_target,
                alignment: None, // Not implemented yet
            });
        }

        VCode {
            insts: self.insts,
            operands,
            operand_ranges,
            clobbers,
            block_ranges,
            block_succ_range,
            block_succs,
            block_pred_range,
            block_preds,
            block_params_range: self.block_params_range,
            block_params: self.block_params,
            branch_block_args: self.branch_block_args,
            branch_block_arg_range: self.branch_block_arg_range,
            branch_block_arg_succ_range: self.branch_block_arg_succ_range,
            entry,
            block_order,
            abi,
            constants: VCodeConstants {
                constants: self.constants,
            },
            block_metadata,
            relocations: self.relocations,
            srclocs: self.srclocs,
        }
    }
}

impl<I: MachInst> Default for VCodeBuilder<I> {
    fn default() -> Self {
        Self::new()
    }
}
