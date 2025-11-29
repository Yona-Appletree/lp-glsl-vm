//! Register allocation integration with regalloc2
//!
//! This module implements the `regalloc2::Function` trait for VCode,
//! enabling register allocation using the regalloc2 library.
//!
//! Since VCode now uses regalloc2 types directly (Block, Inst, VReg, Operand),
//! the implementation is straightforward - we just return slices directly.

use regalloc2::{
    Block, Function as RegallocFunction, Inst, InstRange, Operand, PRegSet, RegClass, VReg,
};

use crate::backend3::vcode::{MachInst, MachTerminator, VCode};

/// Implement regalloc2::Function trait for VCode
///
/// This enables VCode to be used with regalloc2 for register allocation.
/// Since VCode now uses regalloc2 types directly, the implementation is straightforward.
impl<I: MachInst> RegallocFunction for VCode<I> {
    fn num_insts(&self) -> usize {
        self.insts.len()
    }

    fn num_blocks(&self) -> usize {
        self.block_ranges.len()
    }

    fn entry_block(&self) -> Block {
        // BlockIndex is already regalloc2::Block, so we can use it directly
        self.entry
    }

    fn block_insns(&self, block: Block) -> InstRange {
        let range = self
            .block_ranges
            .get(block.index())
            .expect("block should exist");
        InstRange::new(Inst::new(range.start), Inst::new(range.end))
    }

    fn block_succs(&self, block: Block) -> &[Block] {
        let range = self
            .block_succ_range
            .get(block.index())
            .expect("block should exist");
        // Direct slice return - BlockIndex is already regalloc2::Block!
        &self.block_succs[range.start..range.end]
    }

    fn block_preds(&self, block: Block) -> &[Block] {
        let range = self
            .block_pred_range
            .get(block.index())
            .expect("block should exist");
        // Direct slice return - BlockIndex is already regalloc2::Block!
        &self.block_preds[range.start..range.end]
    }

    fn block_params(&self, block: Block) -> &[VReg] {
        // Entry block params are handled by Args instruction, not block params
        if block.index() == self.entry.index() {
            return &[];
        }
        let range = self
            .block_params_range
            .get(block.index())
            .expect("block should exist");
        // Direct slice return - already regalloc2::VReg!
        &self.block_params[range.start..range.end]
    }

    fn branch_blockparams(&self, block: Block, _insn: Inst, succ_idx: usize) -> &[VReg] {
        // Return the VRegs passed to a specific successor block
        let succ_range = self
            .branch_block_arg_succ_range
            .get(block.index())
            .expect("block should exist");
        if succ_idx >= succ_range.len() {
            return &[];
        }
        let branch_block_args = self
            .branch_block_arg_range
            .get(succ_range.start + succ_idx)
            .expect("branch arg range should exist");
        // Direct slice return - already regalloc2::VReg!
        &self.branch_block_args[branch_block_args.start..branch_block_args.end]
    }

    fn is_ret(&self, insn: Inst) -> bool {
        match self.insts[insn.index()].is_term() {
            MachTerminator::Ret | MachTerminator::RetCall => true,
            MachTerminator::Branch => false,
            MachTerminator::None => false, // Could be trap, but not ret
        }
    }

    fn is_branch(&self, insn: Inst) -> bool {
        match self.insts[insn.index()].is_term() {
            MachTerminator::Branch => true,
            _ => false,
        }
    }

    fn inst_operands(&self, insn: Inst) -> &[Operand] {
        let range = self
            .operand_ranges
            .get(insn.index())
            .expect("instruction should exist");
        // Direct slice return - already regalloc2::Operand!
        &self.operands[range.start..range.end]
    }

    fn inst_clobbers(&self, insn: Inst) -> PRegSet {
        // Return explicitly clobbered registers for this instruction
        // (e.g., from function calls)
        // Direct return - already regalloc2::PRegSet!
        self.clobbers.get(&insn).copied().unwrap_or_default()
    }

    fn num_vregs(&self) -> usize {
        self.num_vregs
    }

    fn debug_value_labels(&self) -> &[(VReg, Inst, Inst, u32)] {
        // For debug info (optional, can return empty slice)
        &[]
    }

    fn spillslot_size(&self, regclass: RegClass) -> usize {
        // RISC-V 32: all GPRs are 4 bytes
        match regclass {
            RegClass::Int => 4,    // RISC-V 32: 4 bytes per GPR
            RegClass::Float => 4,  // RISC-V 32: 4 bytes per FPR (if we support floats)
            RegClass::Vector => 4, // RISC-V 32: vectors not supported yet, but use 4 bytes
        }
    }

    fn allow_multiple_vreg_defs(&self) -> bool {
        // Allow multiple defs of the same VReg (needed for some backends)
        true
    }
}

impl<I: MachInst> VCode<I> {
    /// Run register allocation on this VCode
    ///
    /// This performs register allocation using regalloc2 and returns the allocation results.
    ///
    /// # Errors
    ///
    /// Returns an error if register allocation fails (e.g., invalid SSA, too many live registers).
    pub fn run_regalloc(&self) -> Result<regalloc2::Output, regalloc2::RegAllocError> {
        use regalloc2::{Algorithm, RegallocOptions};

        // Get machine environment from ABI
        // Note: This requires I::ABIMachineSpec to have a machine_env method
        // For now, we'll create it directly from Riscv32ABI
        let machine_env = self.create_machine_env();

        // Use default options with Ion algorithm (the standard backtracking allocator)
        let options = RegallocOptions {
            verbose_log: false,
            validate_ssa: true, // Validate SSA form before allocation
            algorithm: Algorithm::Ion,
        };

        regalloc2::run(self, &machine_env, &options)
    }

    /// Create a MachineEnv for register allocation
    ///
    /// This is a helper that delegates to the ABI's machine_env method.
    /// For RISC-V 32, this uses Riscv32ABI::machine_env().
    fn create_machine_env(&self) -> regalloc2::MachineEnv {
        // For now, we need to create the MachineEnv directly
        // In the future, we could add a trait method to ABIMachineSpec
        use crate::isa::riscv32::backend3::abi::Riscv32ABI;
        Riscv32ABI::machine_env()
    }
}
