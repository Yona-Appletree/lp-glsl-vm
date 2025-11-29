//! Lowering: Convert IR to VCode

use alloc::{collections::BTreeMap, vec::Vec};

use lpc_lpir::{
    BlockEntity as Block, ControlFlowGraph, DominatorTree, Function, InstEntity, Opcode,
    RelSourceLoc, Value,
};
use regalloc2::RegClass;

use crate::backend3::{
    blockorder::compute_block_order,
    types::{BlockIndex, VReg, Writable},
    vcode::{BlockLoweringOrder, Callee, MachInst, VCode},
    vcode_builder::VCodeBuilder,
};

/// Lowering backend trait: ISA-specific instruction creation
///
/// This trait allows ISA-specific backends to create machine instructions
/// during lowering. The generic `Lower` struct delegates instruction creation
/// to the backend implementation.
pub trait LowerBackend {
    /// The machine instruction type for this backend
    type MInst: MachInst;

    /// Get the emission info for this backend
    ///
    /// This provides ISA-specific information needed during code emission.
    /// It should be immutable across function compilations within the same module.
    fn emit_info(&self) -> <Self::MInst as MachInst>::Info;

    /// Lower a single IR instruction to machine instructions
    ///
    /// The backend should create the appropriate machine instructions and
    /// push them to the VCodeBuilder via the Lower context.
    ///
    /// Returns `true` if the instruction was lowered, `false` if it's not
    /// supported or handled elsewhere.
    fn lower_inst(
        &self,
        ctx: &mut Lower<Self::MInst>,
        inst: InstEntity,
        srcloc: RelSourceLoc,
    ) -> bool;

    /// Create a move instruction (register copy)
    ///
    /// This is used for phi moves in edge blocks. The backend should create
    /// an appropriate move instruction that copies `src` to `dst`.
    fn create_move(&self, dst: Writable<VReg>, src: VReg) -> Self::MInst;

    /// Create a conditional branch instruction
    ///
    /// This creates a placeholder branch instruction that records the condition
    /// operand. The actual branch targets are stored in VCode branch metadata.
    fn create_branch(&self, condition: VReg) -> Self::MInst;

    /// Create an unconditional jump instruction
    ///
    /// This creates a placeholder jump instruction. The actual jump targets
    /// are stored in VCode branch metadata.
    fn create_jump(&self) -> Self::MInst;

    /// Emit entry block setup instructions (optional)
    ///
    /// This is called for the entry block after start_block() but before
    /// lowering any instructions. Backends can use this to emit pseudo-instructions
    /// like Args that define function parameters.
    ///
    /// Default implementation does nothing (for backends that don't need this).
    fn emit_entry_block_setup(
        &self,
        _ctx: &mut Lower<Self::MInst>,
        _entry_block: Block,
        _srcloc: RelSourceLoc,
    ) {
        // Default: do nothing
    }
}

/// Lowering context: converts IR to VCode
/// Generic over MachInst trait (ISA-agnostic)
pub struct Lower<I: MachInst> {
    /// Function being lowered
    func: Function,

    /// VCode being built
    pub(crate) vcode: VCodeBuilder<I>,

    /// Value to virtual register mapping (immutable after creation)
    /// Each IR Value maps to exactly one VReg. Created once in create_virtual_registers(),
    /// then only read from during lowering. In SSA form, all Values (including instruction
    /// results and block parameters) exist before lowering, so we can create VRegs upfront.
    value_to_vreg: BTreeMap<Value, VReg>,

    /// Block to block index mapping
    block_to_index: BTreeMap<Block, BlockIndex>,

    /// ABI information (ISA-specific, provided via MachInst trait)
    abi: Callee<I::ABIMachineSpec>,
}

impl<I: MachInst> Lower<I> {
    /// Lower a function to VCode using the given backend
    pub fn lower<B: LowerBackend<MInst = I>>(
        mut self,
        backend: &B,
        block_order: &BlockLoweringOrder,
    ) -> VCode<I> {
        // 1. Create virtual registers for all values
        self.create_virtual_registers();

        // 2. Build block index mapping
        self.build_block_index_mapping(block_order);

        // 3. Lower blocks in computed order (not IR layout order)
        for (idx, lowered_block) in block_order.lowered_order.iter().enumerate() {
            match lowered_block {
                crate::backend3::vcode::LoweredBlock::Orig { block } => {
                    self.lower_block(backend, *block, block_order, idx);
                }
                crate::backend3::vcode::LoweredBlock::Edge {
                    from,
                    to,
                    succ_idx: _,
                } => {
                    // Emit phi moves for edge block
                    // Edge blocks use their position in lowered_order as their BlockIndex
                    let edge_block_idx = BlockIndex::new(idx);
                    self.lower_edge_block(backend, edge_block_idx, *from, *to, block_order, idx);
                }
            }
        }

        // 4. Build VCode
        // Find entry block index - entry block must be in block_to_index
        let entry_block = self
            .func
            .entry_block()
            .expect("function must have an entry block");
        let entry = block_order
            .block_to_index
            .get(&entry_block)
            .copied()
            .expect("entry block must be in block_to_index mapping");
        self.vcode.build(entry, block_order.clone(), self.abi)
    }

    /// Create virtual registers for all values
    /// This is called once before lowering. In SSA form, all Values already exist
    /// in the IR (function params, block params, instruction results), so we can
    /// create VRegs for all of them upfront. The mapping is then immutable during lowering.
    /// Defaults to Int register class (GPR) for RISC-V 32.
    fn create_virtual_registers(&mut self) {
        // 1. Function parameters (entry block params)
        if let Some(entry_block) = self.func.entry_block() {
            if let Some(block_data) = self.func.block_data(entry_block) {
                for param_value in &block_data.params {
                    let vreg = self.vcode.alloc_vreg(RegClass::Int);
                    self.value_to_vreg.insert(*param_value, vreg);
                }
            }
        }

        // 2. Block parameters (phi nodes) - each block's params get VRegs
        for block in self.func.blocks() {
            if let Some(block_data) = self.func.block_data(block) {
                for param_value in &block_data.params {
                    if !self.value_to_vreg.contains_key(param_value) {
                        let vreg = self.vcode.alloc_vreg(RegClass::Int);
                        self.value_to_vreg.insert(*param_value, vreg);
                    }
                }
            }
        }

        // 3. Instruction results - each instruction's result Values get VRegs
        for block in self.func.blocks() {
            for inst in self.func.block_insts(block) {
                if let Some(inst_data) = self.func.dfg.inst_data(inst) {
                    for result_value in &inst_data.results {
                        let vreg = self.vcode.alloc_vreg(RegClass::Int);
                        self.value_to_vreg.insert(*result_value, vreg);
                    }
                }
            }
        }
    }

    /// Build block index mapping from block order
    fn build_block_index_mapping(&mut self, block_order: &BlockLoweringOrder) {
        for (ir_block, &lowered_idx) in &block_order.block_to_index {
            self.block_to_index.insert(*ir_block, lowered_idx);
        }
    }

    /// Lower an edge block (phi moves)
    ///
    /// Edge blocks are synthetic blocks inserted on critical edges to handle
    /// phi value moves. They need to call start_block/end_block like regular blocks.
    fn lower_edge_block<B: LowerBackend<MInst = I>>(
        &mut self,
        backend: &B,
        edge_block_idx: BlockIndex,
        from: Block,
        to: Block,
        block_order: &BlockLoweringOrder,
        lowered_idx: usize,
    ) {
        // Edge blocks have no parameters (they're just for moves)
        self.vcode.start_block(edge_block_idx, Vec::new());

        // Get phi values for target block
        if let Some(target_block_data) = self.func.block_data(to) {
            let target_params = &target_block_data.params;

            // Get corresponding source values from predecessor
            // For each parameter, find the value passed from the predecessor block
            for (param_idx, param_value) in target_params.iter().enumerate() {
                let sources = self.func.block_param_sources(to, param_idx);
                // Find the source from the 'from' block
                for (pred_block, source_value) in sources {
                    if pred_block == from {
                        // Emit move: vreg_target = vreg_source
                        let target_vreg = self.value_to_vreg[param_value];
                        let source_vreg = self.value_to_vreg[&source_value];

                        // Only emit move if source and target are different VRegs
                        if target_vreg != source_vreg {
                            use crate::backend3::types::Writable;

                            // Use default source location for synthetic moves
                            let srcloc = RelSourceLoc::default();
                            let move_inst =
                                backend.create_move(Writable::new(target_vreg), source_vreg);
                            self.vcode.push(move_inst, srcloc);
                        }
                        break;
                    }
                }
            }
        }

        // Edge blocks always jump to their target block
        // Create an unconditional jump instruction
        let jump_inst = backend.create_jump();
        let srcloc = RelSourceLoc::default();
        self.vcode.push(jump_inst, srcloc);

        // End block
        self.vcode.end_block();

        // Record branch arguments for edge block
        // Edge blocks have exactly one successor (the target block)
        // Get successors from block_order
        assert!(
            lowered_idx < block_order.lowered_succs.len(),
            "lowered_idx {} out of bounds for lowered_succs len {}",
            lowered_idx,
            block_order.lowered_succs.len()
        );
        let succs = block_order.lowered_succs[lowered_idx].clone();

        // Edge blocks pass the source values (the values being moved from) as branch args
        // The phi moves copy source->target, and then we pass the source values
        // to the target block (which expects them as parameters)
        let mut args_per_succ = Vec::new();
        for _succ_block_idx in &succs {
            let mut arg_vregs = Vec::new();
            if let Some(target_block_data) = self.func.block_data(to) {
                // For each parameter, find the source value from the predecessor block
                for (param_idx, _param_value) in target_block_data.params.iter().enumerate() {
                    let sources = self.func.block_param_sources(to, param_idx);
                    // Find the source from the 'from' block
                    for (pred_block, source_value) in sources {
                        if pred_block == from {
                            arg_vregs.push(self.value_to_vreg[&source_value]);
                            break;
                        }
                    }
                }
            }
            args_per_succ.push(arg_vregs);
        }

        if !succs.is_empty() {
            self.vcode
                .add_branch_args(edge_block_idx, &succs, &args_per_succ);
        }
    }

    /// Lower a block
    fn lower_block<B: LowerBackend<MInst = I>>(
        mut self: &mut Self,
        backend: &B,
        block: Block,
        block_order: &BlockLoweringOrder,
        lowered_idx: usize,
    ) {
        // Get block index for VCode
        let block_idx = self.block_to_index.get(&block).copied().expect(
            "Block should be in block_to_index mapping (computed during block order computation)",
        );

        // Get entry block for comparison
        let entry_block = self
            .func
            .entry_block()
            .expect("function must have an entry block");

        // Get block parameters
        // Entry block params are handled specially - they're not block params for regalloc2
        // (they'll be defined by Args instruction or handled during emission)
        // For regalloc2, entry block params must not be block params because they would be
        // live-in on entry, which regalloc2 doesn't allow. They need to be defined by an instruction.
        let block_params: Vec<VReg> = if block == entry_block {
            // Entry block: no block params (they're function parameters, not phi nodes)
            Vec::new()
        } else if let Some(block_data) = self.func.block_data(block) {
            block_data
                .params
                .iter()
                .map(|v| self.value_to_vreg[v])
                .collect()
        } else {
            Vec::new()
        };

        // Start block in VCode
        self.vcode.start_block(block_idx, block_params);

        // Emit entry block setup (e.g., Args instruction for function parameters)
        if block == entry_block {
            let base_srcloc = self.func.base_srcloc();
            let dummy_srcloc = RelSourceLoc::from_base_offset(base_srcloc, 0);
            backend.emit_entry_block_setup(&mut self, entry_block, dummy_srcloc);
        }

        // Lower each instruction and track branch information
        // Collect instructions first to avoid borrow checker issues
        let insts: Vec<_> = self.func.block_insts(block).collect();
        let mut branch_info: Option<(Vec<BlockIndex>, Vec<Vec<VReg>>)> = None;

        for inst in insts {
            // Get source location from IR instruction
            let ir_srcloc = self.func.srcloc(inst);
            let base_srcloc = self.func.base_srcloc();
            let rel_srcloc = RelSourceLoc::from_base_offset(base_srcloc, ir_srcloc);

            // Check if this is a branch/jump instruction
            // Get inst_data first and drop func() borrow before using vcode
            let (opcode, args, block_args_opt) = {
                let func = self.func();
                if let Some(inst_data) = func.dfg.inst_data(inst) {
                    (
                        inst_data.opcode.clone(),
                        inst_data.args.clone(),
                        inst_data.block_args.clone(),
                    )
                } else {
                    // No instruction data - delegate to backend
                    backend.lower_inst(&mut self, inst, rel_srcloc);
                    continue;
                }
            };

            match opcode {
                Opcode::Br => {
                    // Extract condition VReg from args[0]
                    let condition_vreg = {
                        let value_to_vreg = self.value_to_vreg();
                        args.get(0).and_then(|v| value_to_vreg.get(v).copied())
                    };

                    if let Some(condition) = condition_vreg {
                        // Create Br instruction with condition operand
                        let br_inst = backend.create_branch(condition);
                        self.vcode.push(br_inst, rel_srcloc);
                    }

                    // Get successors from block_order (this is the source of truth)
                    // Use the lowered_idx passed to this function (position in lowered_order)
                    let mut succs = Vec::new();
                    let mut args_per_succ = Vec::new();

                    // Get successors from block_order - this should always work since
                    // lowered_succs has one entry per lowered_order entry
                    assert!(
                        lowered_idx < block_order.lowered_succs.len(),
                        "lowered_idx {} out of bounds for lowered_succs len {}",
                        lowered_idx,
                        block_order.lowered_succs.len()
                    );
                    succs = block_order.lowered_succs[lowered_idx].clone();

                    // Extract arguments from block_args if available
                    let value_to_vreg = self.value_to_vreg();
                    if let Some(block_args) = &block_args_opt {
                        // Map block_args targets to VCode BlockIndices and extract args
                        let mut args_map = alloc::collections::BTreeMap::new();
                        for (target_block, args) in &block_args.targets {
                            if let Some(&target_idx) = self.block_to_index.get(target_block) {
                                let arg_vregs: Vec<VReg> = args
                                    .iter()
                                    .filter_map(|v| value_to_vreg.get(v).copied())
                                    .collect();
                                args_map.insert(target_idx, arg_vregs);
                            }
                        }
                        // Build args_per_succ in the same order as succs
                        for &succ in &succs {
                            args_per_succ.push(args_map.get(&succ).cloned().unwrap_or_default());
                        }
                    } else {
                        // No block_args - use empty args for all successors
                        args_per_succ = succs.iter().map(|_| Vec::new()).collect();
                    }

                    if !succs.is_empty() {
                        branch_info = Some((succs, args_per_succ));
                    }
                }
                Opcode::Jump => {
                    // Create Jump instruction (unconditional)
                    let jump_inst = backend.create_jump();
                    self.vcode.push(jump_inst, rel_srcloc);

                    // Get successors from block_order (this is the source of truth)
                    // Use the lowered_idx passed to this function (position in lowered_order)
                    let mut succs = Vec::new();
                    let mut args_per_succ = Vec::new();

                    // Get successors from block_order - this should always work since
                    // lowered_succs has one entry per lowered_order entry
                    assert!(
                        lowered_idx < block_order.lowered_succs.len(),
                        "lowered_idx {} out of bounds for lowered_succs len {}",
                        lowered_idx,
                        block_order.lowered_succs.len()
                    );
                    succs = block_order.lowered_succs[lowered_idx].clone();

                    // For Br/Jump instructions, we should always have successors
                    // If succs is empty, that's a bug - but we'll handle it gracefully
                    if succs.is_empty() {
                        // This shouldn't happen for valid IR, but if it does, we'll skip recording branch args
                        // The validation will catch this as an error
                    } else {
                        // Extract arguments from block_args if available
                        let value_to_vreg = self.value_to_vreg();
                        if let Some(block_args) = &block_args_opt {
                            // Map block_args targets to VCode BlockIndices and extract args
                            let mut args_map = alloc::collections::BTreeMap::new();
                            for (target_block, args) in &block_args.targets {
                                if let Some(&target_idx) = self.block_to_index.get(target_block) {
                                    let arg_vregs: Vec<VReg> = args
                                        .iter()
                                        .filter_map(|v| value_to_vreg.get(v).copied())
                                        .collect();
                                    args_map.insert(target_idx, arg_vregs);
                                }
                            }
                            // Build args_per_succ in the same order as succs
                            for &succ in &succs {
                                args_per_succ
                                    .push(args_map.get(&succ).cloned().unwrap_or_default());
                            }
                        } else {
                            // No block_args - use empty args for all successors
                            args_per_succ = succs.iter().map(|_| Vec::new()).collect();
                        }

                        // Record branch info since we have successors
                        branch_info = Some((succs, args_per_succ));
                    }
                }
                _ => {
                    // Not a branch - delegate to backend for instruction creation
                    backend.lower_inst(&mut self, inst, rel_srcloc);
                }
            }
        }

        // End block
        self.vcode.end_block();

        // Record branch arguments if we have branch information
        if let Some((succs, args_per_succ)) = branch_info {
            self.vcode
                .add_branch_args(block_idx, &succs, &args_per_succ);
        }
    }

    /// Get the value-to-VReg mapping (for backend use)
    pub(crate) fn value_to_vreg(&self) -> &BTreeMap<Value, VReg> {
        &self.value_to_vreg
    }

    /// Get mutable access to value-to-VReg mapping (for backend use)
    /// Used for cases like iconst where we need to update the mapping
    pub(crate) fn value_to_vreg_mut(&mut self) -> &mut BTreeMap<Value, VReg> {
        &mut self.value_to_vreg
    }

    /// Get the function being lowered (for backend use)
    pub(crate) fn func(&self) -> &Function {
        &self.func
    }
}

/// Lower a function to VCode using the given backend
pub fn lower_function<B: LowerBackend>(
    func: Function,
    backend: &B,
    abi: Callee<<B::MInst as MachInst>::ABIMachineSpec>,
) -> VCode<B::MInst>
where
    B::MInst: MachInst,
{
    // Build CFG and dominator tree
    let cfg = ControlFlowGraph::from_function(&func);
    let domtree = DominatorTree::from_cfg(&cfg);

    // Compute block lowering order
    let block_order = compute_block_order(&func, &cfg, &domtree);

    // Get emission info from backend
    let emit_info = backend.emit_info();

    // Create lowering context
    let lower = Lower {
        func,
        vcode: VCodeBuilder::new(emit_info.clone()),
        value_to_vreg: BTreeMap::new(),
        block_to_index: BTreeMap::new(),
        abi,
    };

    // Lower the function
    lower.lower(backend, &block_order)
}
