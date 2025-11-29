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

    /// Add block metadata
    pub fn add_block_metadata(&mut self, metadata: BlockMetadata) {
        self.block_metadata.push(metadata);
    }

    /// Build the final VCode
    ///
    /// This consumes the builder and produces a VCode structure.
    /// The caller must provide the block order and ABI information.
    pub fn build(
        self,
        entry: BlockIndex,
        block_order: BlockLoweringOrder,
        abi: Callee<I::ABIMachineSpec>,
    ) -> VCode<I> {
        // Build block ranges from block_starts
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

        VCode {
            insts: self.insts,
            operands: Vec::new(), // Will be populated during operand collection
            operand_ranges: Ranges::new(), // Will be populated during operand collection
            clobbers: BTreeMap::new(), // Will be populated during operand collection
            block_ranges,
            block_succ_range: Ranges::new(), // Will be populated from block_order
            block_succs: Vec::new(),         // Will be populated from block_order
            block_pred_range: Ranges::new(), // Will be populated from block_order
            block_preds: Vec::new(),         // Will be populated from block_order
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
            block_metadata: self.block_metadata,
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
