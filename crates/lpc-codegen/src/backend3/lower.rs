//! Lowering: Convert IR to VCode

use alloc::{collections::BTreeMap, vec::Vec};

use crate::backend3::blockorder::compute_block_order;
use crate::backend3::constants::materialize_constant;
use crate::backend3::types::{BlockIndex, VReg, Writable};
use crate::backend3::vcode::{BlockLoweringOrder, Callee, MachInst, VCode};
use crate::backend3::vcode_builder::VCodeBuilder;
use lpc_lpir::{
    BlockEntity as Block, ControlFlowGraph, DominatorTree, Function, Immediate,
    InstEntity, Opcode, RelSourceLoc, Value,
};

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
    /// Lower a function to VCode
    pub fn lower(
        mut self,
        block_order: &BlockLoweringOrder,
    ) -> VCode<I> {

        // 1. Create virtual registers for all values
        self.create_virtual_registers();

        // 2. Build block index mapping
        self.build_block_index_mapping(block_order);

        // 3. Lower blocks in computed order (not IR layout order)
        for lowered_block in &block_order.lowered_order {
            match lowered_block {
                crate::backend3::vcode::LoweredBlock::Orig { block } => {
                    self.lower_block(*block);
                }
                crate::backend3::vcode::LoweredBlock::Edge { from, to } => {
                    // Emit phi moves for edge block
                    self.lower_edge_block(*from, *to);
                }
            }
        }

        // 4. Build VCode
        let entry = block_order
            .block_to_index
            .get(&self.func.entry_block().unwrap())
            .copied()
            .unwrap_or(BlockIndex::new(0));
        self.vcode.build(entry, block_order.clone(), self.abi)
    }

    /// Create virtual registers for all values
    /// This is called once before lowering. In SSA form, all Values already exist
    /// in the IR (function params, block params, instruction results), so we can
    /// create VRegs for all of them upfront. The mapping is then immutable during lowering.
    fn create_virtual_registers(&mut self) {
        // 1. Function parameters (entry block params)
        if let Some(entry_block) = self.func.entry_block() {
            if let Some(block_data) = self.func.block_data(entry_block) {
                for param_value in &block_data.params {
                    let vreg = self.vcode.alloc_vreg();
                    self.value_to_vreg.insert(*param_value, vreg);
                }
            }
        }

        // 2. Block parameters (phi nodes) - each block's params get VRegs
        for block in self.func.blocks() {
            if let Some(block_data) = self.func.block_data(block) {
                for param_value in &block_data.params {
                    if !self.value_to_vreg.contains_key(param_value) {
                        let vreg = self.vcode.alloc_vreg();
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
                        let vreg = self.vcode.alloc_vreg();
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
    fn lower_edge_block(&mut self, from: Block, to: Block) {
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
                        // TODO: Emit move instruction (will be implemented when we add move instructions)
                        // For now, this is a placeholder
                        break;
                    }
                }
            }
        }
    }

    /// Lower a block
    fn lower_block(&mut self, block: Block) {
        // Get block index for VCode
        let block_idx = self.block_to_index.get(&block).copied().unwrap_or(BlockIndex::new(0));

        // Get block parameters
        let block_params: Vec<VReg> = if let Some(block_data) = self.func.block_data(block) {
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

        // Lower each instruction
        // Collect instructions first to avoid borrow checker issues
        let insts: Vec<_> = self.func.block_insts(block).collect();
        for inst in insts {
            self.lower_inst(inst);
        }

        // End block
        self.vcode.end_block();
    }

    /// Lower an instruction
    fn lower_inst(&mut self, inst: InstEntity) {
        let inst_data = match self.func.dfg.inst_data(inst) {
            Some(data) => data,
            None => return,
        };

        // Get source location from IR instruction
        let ir_srcloc = self.func.srcloc(inst);
        let base_srcloc = self.func.base_srcloc();
        let rel_srcloc = RelSourceLoc::from_base_offset(base_srcloc, ir_srcloc);

        // Lower based on opcode
        match inst_data.opcode {
            Opcode::Iadd => {
                if inst_data.args.len() >= 2 && !inst_data.results.is_empty() {
                    let rs1 = self.value_to_vreg[&inst_data.args[0]];
                    let rs2 = self.value_to_vreg[&inst_data.args[1]];
                    let rd = Writable::new(self.value_to_vreg[&inst_data.results[0]]);
                    // Create instruction - for Phase 1, we only support RISC-V 32
                    // TODO: Make this properly generic in future phases
                    self.create_and_push_add(rd, rs1, rs2, rel_srcloc);
                }
            }
            Opcode::Isub => {
                if inst_data.args.len() >= 2 && !inst_data.results.is_empty() {
                    let rs1 = self.value_to_vreg[&inst_data.args[0]];
                    let rs2 = self.value_to_vreg[&inst_data.args[1]];
                    let rd = Writable::new(self.value_to_vreg[&inst_data.results[0]]);
                    // Create instruction - for Phase 1, we only support RISC-V 32
                    // TODO: Make this properly generic in future phases
                    self.create_and_push_sub(rd, rs1, rs2, rel_srcloc);
                }
            }
            Opcode::Iconst => {
                if !inst_data.results.is_empty() {
                    // Materialize constant
                    if let Some(imm) = &inst_data.imm {
                        let value = match imm {
                            Immediate::I32(val) => *val,
                            Immediate::I64(val) => *val as i32, // Truncate to i32
                            _ => 0,
                        };
                        let vreg = materialize_constant(&mut self.vcode, value);
                        // Map the result value to the constant VReg
                        self.value_to_vreg.insert(inst_data.results[0], vreg);
                    }
                }
            }
            _ => {
                // Other opcodes not yet implemented
            }
        }
    }

    /// Create and push ADD instruction (Phase 1: RISC-V 32 only)
    /// This will be made properly generic in future phases
    fn create_and_push_add(&mut self, rd: Writable<VReg>, rs1: VReg, rs2: VReg, srcloc: RelSourceLoc) {
        // For Phase 1, we only support RISC-V 32
        // This is a workaround - in future phases this will be properly generic via a trait
        // We know that I is Riscv32MachInst for Phase 1
        // This will be replaced with a proper trait-based approach in Phase 2+
    }

    /// Create and push SUB instruction (Phase 1: RISC-V 32 only)
    /// This will be made properly generic in future phases
    fn create_and_push_sub(&mut self, rd: Writable<VReg>, rs1: VReg, rs2: VReg, srcloc: RelSourceLoc) {
        // For Phase 1, we only support RISC-V 32
        // This is a workaround - in future phases this will be properly generic via a trait
        // We know that I is Riscv32MachInst for Phase 1
        // This will be replaced with a proper trait-based approach in Phase 2+
    }
}

/// Lower a function to VCode
pub fn lower_function<I: MachInst>(
    func: Function,
    abi: Callee<I::ABIMachineSpec>,
) -> VCode<I> {
    // Build CFG and dominator tree
    let cfg = ControlFlowGraph::from_function(&func);
    let domtree = DominatorTree::from_cfg(&cfg);

    // Compute block lowering order
    let block_order = compute_block_order(&func, &cfg, &domtree);

    // Create lowering context
    let lower = Lower {
        func,
        vcode: VCodeBuilder::new(),
        value_to_vreg: BTreeMap::new(),
        block_to_index: BTreeMap::new(),
        abi,
    };

    // Lower the function
    lower.lower(&block_order)
}

