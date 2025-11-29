//! VCode builder for constructing VCode incrementally

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use lpc_lpir::RelSourceLoc;
use regalloc2::{Operand, OperandKind, OperandPos, PRegSet, RegClass};

use crate::backend3::{
    types::{BlockIndex, InsnIndex, PINNED_VREGS, Range, Ranges, VReg},
    vcode::{
        BlockLoweringOrder, BlockMetadata, Callee, Constant, MachInst, OperandVisitor, RelocKind,
        VCode, VCodeConstants, VCodeReloc,
    },
};

/// Builder for constructing VCode incrementally
pub struct VCodeBuilder<I: MachInst> {
    /// Instructions being built
    insts: Vec<I>,
    /// Source locations (parallel to insts)
    srclocs: Vec<RelSourceLoc>,
    /// Constants (indexed by VReg)
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
    /// Block parameter VRegs (stored as regalloc2::VReg)
    block_params: Vec<VReg>,
    /// Block parameter ranges
    block_params_range: Ranges,
    /// Branch block arguments (stored as regalloc2::VReg)
    branch_block_args: Vec<VReg>,
    /// Branch block arg ranges
    branch_block_arg_range: Ranges,
    /// Branch block arg succ ranges
    branch_block_arg_succ_range: Ranges,
    /// ISA-specific emission information
    emit_info: I::Info,
}

impl<I: MachInst> VCodeBuilder<I> {
    /// Create a new VCodeBuilder
    pub fn new(emit_info: I::Info) -> Self {
        VCodeBuilder {
            insts: Vec::new(),
            srclocs: Vec::new(),
            constants: BTreeMap::new(),
            relocations: Vec::new(),
            block_metadata: Vec::new(),
            next_vreg: PINNED_VREGS as u32, // Start allocating VRegs after pinned range
            current_block: None,
            block_starts: Vec::new(),
            block_params: Vec::new(),
            block_params_range: Ranges::new(),
            branch_block_args: Vec::new(),
            branch_block_arg_range: Ranges::new(),
            branch_block_arg_succ_range: Ranges::new(),
            emit_info,
        }
    }

    /// Allocate a new virtual register
    ///
    /// Returns a regalloc2::VReg with the specified register class.
    /// Defaults to Int (GPR) for RISC-V 32.
    pub fn alloc_vreg(&mut self, regclass: RegClass) -> VReg {
        let vreg = VReg::new(self.next_vreg as usize, regclass);
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
    ///
    /// `params` are VRegs that will be stored directly.
    pub fn start_block(&mut self, block_idx: BlockIndex, params: Vec<VReg>) {
        self.current_block = Some(block_idx);
        let start_inst = self.insts.len();
        self.block_starts.push(start_inst);

        // Record block parameters
        let param_start = self.block_params.len();
        self.block_params.extend(params);
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
    ///
    /// `block_idx` is the BlockIndex of the block these branch args belong to.
    /// `args_per_succ` contains VRegs that will be stored directly.
    pub fn add_branch_args(
        &mut self,
        block_idx: BlockIndex,
        succs: &[BlockIndex],
        args_per_succ: &[Vec<VReg>],
    ) {
        assert_eq!(succs.len(), args_per_succ.len());

        // Ensure branch_block_arg_succ_range is large enough
        let block_idx_usize = block_idx.index();
        while self.branch_block_arg_succ_range.len() <= block_idx_usize {
            // Push empty range for blocks without branch args
            self.branch_block_arg_succ_range.push(Range::new(0, 0));
        }

        // Record where we start adding ranges for this block's successors
        let succ_start = self.branch_block_arg_range.len();

        // Add argument ranges for each successor
        for args in args_per_succ {
            let arg_start = self.branch_block_args.len();
            self.branch_block_args.extend(args.iter().copied());
            let arg_end = self.branch_block_args.len();
            self.branch_block_arg_range
                .push(Range::new(arg_start, arg_end));
        }

        // Record the range in branch_block_arg_range that corresponds to this block's successors
        let succ_end = self.branch_block_arg_range.len();
        self.branch_block_arg_succ_range
            .set(block_idx_usize, Range::new(succ_start, succ_end));
    }

    /// Add block metadata
    pub fn add_block_metadata(&mut self, metadata: BlockMetadata) {
        self.block_metadata.push(metadata);
    }

    /// Validate VCode invariants before building
    ///
    /// This performs comprehensive checks to ensure the VCode structure is valid:
    /// - Block ranges cover all instructions exactly once
    /// - Operand ranges match instruction count
    /// - Source locations match instruction count
    /// - Entry block is valid
    /// - Block metadata matches block count
    /// - Branch arguments match target block parameter counts
    /// - Branch targets exist in block_order
    ///
    /// Note: Block termination validation (ensuring blocks end with terminators)
    /// is not performed here as it requires ISA-specific knowledge of which
    /// instructions are terminators. This could be added in the future if needed.
    fn validate_vcode_invariants(
        insts: &[I],
        srclocs: &[RelSourceLoc],
        block_ranges: &Ranges,
        operand_ranges: &Ranges,
        operands: &[Operand],
        block_succ_range: &Ranges,
        block_succs: &[BlockIndex],
        block_pred_range: &Ranges,
        _block_preds: &[BlockIndex],
        block_params_range: &Ranges,
        block_metadata: &[BlockMetadata],
        entry: BlockIndex,
        _block_order: &BlockLoweringOrder,
        branch_block_args: &[VReg],
        branch_block_arg_range: &Ranges,
        branch_block_arg_succ_range: &Ranges,
    ) {
        // Check source locations match instruction count
        assert_eq!(
            srclocs.len(),
            insts.len(),
            "Source locations must match instruction count"
        );

        // Check operand ranges match instruction count
        assert_eq!(
            operand_ranges.len(),
            insts.len(),
            "Operand ranges must match instruction count"
        );

        // Check block metadata matches block count
        assert_eq!(
            block_metadata.len(),
            block_ranges.len(),
            "Block metadata must match block count"
        );

        // Check block parameter ranges match block count
        assert_eq!(
            block_params_range.len(),
            block_ranges.len(),
            "Block parameter ranges must match block count"
        );

        // Check entry block is valid
        assert!(
            entry.index() < block_ranges.len(),
            "Entry block index {} must be less than block count {}",
            entry.index(),
            block_ranges.len()
        );

        // Check block ranges cover all instructions exactly once
        let mut total_covered = 0;
        for i in 0..block_ranges.len() {
            if let Some(range) = block_ranges.get(i) {
                assert!(
                    range.start <= range.end,
                    "Block range {}: start {} must be <= end {}",
                    i,
                    range.start,
                    range.end
                );
                assert!(
                    range.end <= insts.len(),
                    "Block range {}: end {} must be <= instruction count {}",
                    i,
                    range.end,
                    insts.len()
                );
                total_covered += range.len();

                // Check contiguity with next range
                if i + 1 < block_ranges.len() {
                    if let Some(next_range) = block_ranges.get(i + 1) {
                        assert_eq!(
                            range.end,
                            next_range.start,
                            "Block ranges must be contiguous: range {} ends at {}, range {} \
                             starts at {}",
                            i,
                            range.end,
                            i + 1,
                            next_range.start
                        );
                    }
                }
            }
        }
        assert_eq!(
            total_covered,
            insts.len(),
            "Block ranges must cover all instructions exactly once (covered {}, total {})",
            total_covered,
            insts.len()
        );

        // Check operand ranges are valid and contiguous
        let mut total_operands_covered = 0;
        for i in 0..operand_ranges.len() {
            if let Some(range) = operand_ranges.get(i) {
                assert!(
                    range.start <= range.end,
                    "Operand range {}: start {} must be <= end {}",
                    i,
                    range.start,
                    range.end
                );
                assert!(
                    range.end <= operands.len(),
                    "Operand range {}: end {} must be <= operand count {}",
                    i,
                    range.end,
                    operands.len()
                );
                total_operands_covered += range.len();

                // Check contiguity with next range
                if i + 1 < operand_ranges.len() {
                    if let Some(next_range) = operand_ranges.get(i + 1) {
                        assert_eq!(
                            range.end,
                            next_range.start,
                            "Operand ranges must be contiguous: range {} ends at {}, range {} \
                             starts at {}",
                            i,
                            range.end,
                            i + 1,
                            next_range.start
                        );
                    }
                }
            }
        }
        assert_eq!(
            total_operands_covered,
            operands.len(),
            "Operand ranges must cover all operands exactly once (covered {}, total {})",
            total_operands_covered,
            operands.len()
        );

        // Check predecessor/successor ranges match block count
        assert_eq!(
            block_succ_range.len(),
            block_ranges.len(),
            "Block successor ranges must match block count"
        );
        assert_eq!(
            block_pred_range.len(),
            block_ranges.len(),
            "Block predecessor ranges must match block count"
        );

        // Validate branch arguments match target block parameter counts
        Self::validate_branch_args(
            block_ranges,
            block_succ_range,
            block_succs,
            block_params_range,
            branch_block_args,
            branch_block_arg_range,
            branch_block_arg_succ_range,
        );

        // Validate branch targets exist in block_order
        Self::validate_branch_targets(block_succs, block_ranges.len());
    }

    /// Validate that branch arguments match target block parameter counts
    ///
    /// This validates that:
    /// 1. Blocks with successors must have branch args recorded (even if empty)
    /// 2. Branch argument counts match target block parameter counts
    fn validate_branch_args(
        block_ranges: &Ranges,
        block_succ_range: &Ranges,
        block_succs: &[BlockIndex],
        block_params_range: &Ranges,
        _branch_block_args: &[VReg],
        branch_block_arg_range: &Ranges,
        branch_block_arg_succ_range: &Ranges,
    ) {
        // For each block, check that branch arguments match target block parameters
        for block_idx in 0..block_ranges.len() {
            // Get successors for this block
            if let Some(succ_range) = block_succ_range.get(block_idx) {
                let succs = &block_succs[succ_range.start..succ_range.end];

                // Skip blocks with no successors (e.g., return blocks)
                if succs.is_empty() {
                    continue;
                }

                // Blocks with successors must have branch args recorded
                // But only if branch args were actually recorded (branch_block_arg_succ_range has an entry)
                if let Some(arg_succ_range) = branch_block_arg_succ_range.get(block_idx) {
                    let arg_succ_start = arg_succ_range.start;
                    let arg_succ_end = arg_succ_range.end;

                    // Each successor should have a corresponding argument range
                    assert_eq!(
                        arg_succ_end - arg_succ_start,
                        succs.len(),
                        "Block {}: branch argument successor range count {} must match successor \
                         count {}",
                        block_idx,
                        arg_succ_end - arg_succ_start,
                        succs.len()
                    );

                    // Check each successor's arguments match its parameter count
                    for (succ_idx, &succ_block) in succs.iter().enumerate() {
                        let succ_block_idx = succ_block.index() as usize;

                        // Get argument range for this successor
                        if let Some(arg_range) =
                            branch_block_arg_range.get(arg_succ_start + succ_idx)
                        {
                            let arg_count = arg_range.len();

                            // Get parameter count for target block
                            if let Some(param_range) = block_params_range.get(succ_block_idx) {
                                let param_count = param_range.len();

                                assert_eq!(
                                    arg_count, param_count,
                                    "Block {} -> Block {}: branch argument count {} must match \
                                     target block parameter count {}",
                                    block_idx, succ_block_idx, arg_count, param_count
                                );
                            }
                        }
                    }
                } else {
                    // Block has successors but no branch args recorded - this is an error
                    // It means a Br/Jump instruction didn't record branch args
                    panic!(
                        "Block {} has {} successors but no branch arguments recorded (missing \
                         Br/Jump branch args)",
                        block_idx,
                        succs.len()
                    );
                }
            }
        }
    }

    /// Validate that all branch targets exist in block_order
    fn validate_branch_targets(block_succs: &[BlockIndex], num_blocks: usize) {
        for &target in block_succs {
            assert!(
                target.index() < num_blocks,
                "Branch target block {} must be less than block count {}",
                target.index(),
                num_blocks
            );
        }
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
        starts.resize(num_blocks, 0usize);
        for succ in block_succs {
            let idx = succ.index();
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
            let pred = BlockIndex::new(pred_idx);
            for succ in &block_succs[range.start..range.end] {
                let succ_idx = succ.index();
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
    /// and collects regalloc2 operands directly without conversion.
    fn collect_operands(insts: &[I]) -> (Vec<Operand>, Ranges, BTreeMap<InsnIndex, PRegSet>) {
        struct OperandCollector {
            operands: Vec<Operand>,
        }

        impl OperandVisitor for OperandCollector {
            fn visit_use(&mut self, vreg: VReg, constraint: regalloc2::OperandConstraint) {
                // VReg already has the correct register class from allocation
                let operand = Operand::new(
                    vreg,
                    constraint,
                    OperandKind::Use,
                    OperandPos::Early, // Default to Early position
                );
                self.operands.push(operand);
            }

            fn visit_def(&mut self, vreg: VReg, constraint: regalloc2::OperandConstraint) {
                // VReg already has the correct register class from allocation
                let operand = Operand::new(
                    vreg,
                    constraint,
                    OperandKind::Def,
                    OperandPos::Early, // Default to Early position
                );
                self.operands.push(operand);
            }

            fn visit_mod(&mut self, vreg: VReg, constraint: regalloc2::OperandConstraint) {
                // regalloc2 doesn't support Mod, so we create separate Use and Def operands
                // VReg already has the correct register class from allocation
                let use_operand = Operand::new(
                    vreg,
                    constraint,
                    OperandKind::Use,
                    OperandPos::Early,
                );
                let def_operand = Operand::new(
                    vreg,
                    constraint,
                    OperandKind::Def,
                    OperandPos::Early,
                );
                self.operands.push(use_operand);
                self.operands.push(def_operand);
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
            operands.extend(collector.operands);
            let end = operands.len();
            operand_ranges.push(Range::new(start, end));

            // Collect clobbers - PRegSet is already regalloc2::PRegSet
            if let Some(clobber_set) = inst_clone.get_clobbers() {
                clobbers.insert(InsnIndex::new(idx), clobber_set);
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
            let block_idx = BlockIndex::new(idx);
            let cold = block_order.cold_blocks.contains(&block_idx);
            let indirect_target = block_order.indirect_targets.contains(&block_idx);

            // Determine alignment requirement for this block
            // Currently, RISC-V doesn't require special block alignment for most cases
            // Indirect branch targets may require alignment in the future
            let alignment = if indirect_target {
                // Indirect branch targets may require 4-byte alignment (RISC-V instruction alignment)
                // This is a placeholder - actual alignment requirements depend on ISA and use case
                Some(4)
            } else {
                None
            };

            block_metadata.push(BlockMetadata {
                cold,
                indirect_target,
                alignment,
            });
        }

        // Validate invariants before building VCode
        Self::validate_vcode_invariants(
            &self.insts,
            &self.srclocs,
            &block_ranges,
            &operand_ranges,
            &operands,
            &block_succ_range,
            &block_succs,
            &block_pred_range,
            &block_preds,
            &self.block_params_range,
            &block_metadata,
            entry,
            &block_order,
            &self.branch_block_args,
            &self.branch_block_arg_range,
            &self.branch_block_arg_succ_range,
        );

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
            emit_info: self.emit_info,
            constants: VCodeConstants {
                constants: self.constants,
            },
            block_metadata,
            relocations: self.relocations,
            srclocs: self.srclocs,
            num_vregs: self.next_vreg as usize,
        }
    }
}
