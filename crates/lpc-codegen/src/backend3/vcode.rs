//! VCode structure: Virtual-register code with machine instructions

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::fmt;

use lpc_lpir::RelSourceLoc;

use crate::backend3::types::{BlockIndex, InsnIndex, Ranges, VReg};

/// Virtual-register code: machine instructions with virtual registers
///
/// This is the intermediate representation between IR lowering and register allocation.
/// All registers are virtual (VReg) and will be assigned physical registers during
/// register allocation.
pub struct VCode<I: MachInst> {
    /// Machine instructions (with VReg operands)
    pub insts: Vec<I>,

    /// Operands: flat array for regalloc2
    /// Each operand has: (vreg, constraint, kind)
    /// This will be populated during operand collection
    pub operands: Vec<Operand>,

    /// Operand ranges: per-instruction ranges in operands array
    pub operand_ranges: Ranges,

    /// Clobbers: explicit clobber sets per instruction (for function calls, etc.)
    pub clobbers: BTreeMap<InsnIndex, PRegSet>,

    /// Block structure
    pub block_ranges: Ranges, // Per-block instruction ranges
    pub block_succ_range: Ranges,     // Per-block successor ranges
    pub block_succs: Vec<BlockIndex>, // Successors (flat array)
    pub block_pred_range: Ranges,     // Per-block predecessor ranges
    pub block_preds: Vec<BlockIndex>, // Predecessors (flat array)
    pub block_params_range: Ranges,   // Per-block parameter ranges
    pub block_params: Vec<VReg>,      // Block parameter VRegs (flat array)

    /// Branch arguments (values passed to blocks)
    pub branch_block_args: Vec<VReg>,
    pub branch_block_arg_range: Ranges,
    pub branch_block_arg_succ_range: Ranges,

    /// Entry block
    pub entry: BlockIndex,

    /// Block lowering order
    pub block_order: BlockLoweringOrder,

    /// ABI information
    pub abi: Callee<I::ABIMachineSpec>,

    /// Constants (inline or pool references)
    pub constants: VCodeConstants,

    /// Block metadata
    pub block_metadata: Vec<BlockMetadata>,

    /// Relocations (function calls, etc.)
    pub relocations: Vec<VCodeReloc>,

    /// Source locations for each instruction (for debugging)
    /// One RelSourceLoc per instruction, parallel to `insts` array
    pub srclocs: Vec<RelSourceLoc>,
}

/// Operand information for regalloc2
#[derive(Debug, Clone)]
pub struct Operand {
    /// Virtual register
    pub vreg: VReg,
    /// Operand constraint (fixed, any, etc.)
    pub constraint: OperandConstraint,
    /// Operand kind (use, def, etc.)
    pub kind: OperandKind,
}

/// Operand constraint
///
/// Note: PReg is a trait, so we can't use it directly in an enum.
/// For now, we use a placeholder. ISA-specific code will provide
/// concrete constraint types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandConstraint {
    /// Any register
    Any,
    /// Fixed physical register (represented as u32 for now)
    Fixed(u32),
    /// Register class constraint
    RegClass(RegClass),
}

/// Operand kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandKind {
    /// Use (read)
    Use,
    /// Def (write)
    Def,
    /// Mod (read and write)
    Mod,
}

/// Physical register (ISA-specific, will be defined per ISA)
///
/// This is a placeholder trait. ISA-specific implementations will provide
/// concrete types that implement this trait.
pub trait PReg: Copy + Clone + PartialEq + Eq + core::hash::Hash + fmt::Debug {}

/// Physical register set
///
/// Note: For now, we use a generic representation. ISA-specific code will
/// provide concrete implementations.
pub type PRegSet = alloc::collections::BTreeSet<u32>; // Placeholder: will be ISA-specific

/// Register class
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegClass {
    /// General-purpose integer registers
    Gpr,
    /// Floating-point registers
    Fpr,
}

/// Block lowering order
#[derive(Debug, Clone)]
pub struct BlockLoweringOrder {
    /// Lowered blocks in RPO order
    pub lowered_order: Vec<LoweredBlock>,
    /// Successor lists for each lowered block
    pub lowered_succs: Vec<Vec<BlockIndex>>,
    /// Mapping from IR blocks to lowered block indices
    pub block_to_index: BTreeMap<lpc_lpir::BlockEntity, BlockIndex>,
    /// Cold blocks (for layout optimization)
    pub cold_blocks: alloc::collections::BTreeSet<BlockIndex>,
    /// Indirect branch targets
    pub indirect_targets: alloc::collections::BTreeSet<BlockIndex>,
}

/// Lowered block (original or edge block)
#[derive(Debug, Clone)]
pub enum LoweredBlock {
    /// Original IR block
    Orig { block: lpc_lpir::BlockEntity },
    /// Edge block (for critical edges)
    Edge {
        /// The predecessor block
        from: lpc_lpir::BlockEntity,
        /// The successor block
        to: lpc_lpir::BlockEntity,
        /// The index of this branch in the successor edges from `from`, following the same
        /// indexing order as the CFG. This is used to distinguish multiple edges between
        /// the same CLIF blocks.
        succ_idx: u32,
    },
}

/// ABI information (Callee)
pub struct Callee<ABI> {
    /// ABI machine spec
    pub abi: ABI,
}

/// VCode constants
#[derive(Debug, Clone)]
pub struct VCodeConstants {
    /// Constant values indexed by VReg
    pub constants: BTreeMap<VReg, Constant>,
}

/// Constant representation
#[derive(Debug, Clone)]
pub enum Constant {
    /// Inline constant (fits in instruction immediate)
    Inline(i32),
    /// Large constant (needs multiple instructions)
    Large(i32),
    /// Constant pool reference (future)
    Pool(u32),
}

/// Block metadata
#[derive(Debug, Clone)]
pub struct BlockMetadata {
    /// Is this a cold block?
    pub cold: bool,
    /// Is this an indirect branch target?
    pub indirect_target: bool,
    /// Alignment requirement (in bytes, power of 2)
    pub alignment: Option<u32>,
}

/// VCode relocation
#[derive(Debug, Clone)]
pub struct VCodeReloc {
    /// Instruction index where relocation occurs
    pub inst_idx: InsnIndex,
    /// Relocation kind
    pub kind: RelocKind,
    /// Target identifier (function name, etc.)
    pub target: String,
}

/// Relocation kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocKind {
    /// Function call (needs function address)
    FunctionCall,
    /// Branch target (resolved during emission)
    Branch,
}

/// MachInst trait: Machine instruction interface for regalloc2
///
/// This trait must be implemented by ISA-specific machine instruction types.
pub trait MachInst: Clone + fmt::Debug {
    /// ABI machine spec type
    type ABIMachineSpec: fmt::Debug;

    /// Get operands for this instruction
    ///
    /// The visitor will be called for each operand (use, def, mod).
    fn get_operands(&mut self, collector: &mut impl OperandVisitor);

    /// Get clobbered registers (for function calls, etc.)
    ///
    /// Returns None if no explicit clobbers, or Some(set) if there are clobbers.
    fn get_clobbers(&self) -> Option<PRegSet> {
        None
    }
}

/// Operand visitor trait for collecting operands
pub trait OperandVisitor {
    /// Visit a use (read) operand
    fn visit_use(&mut self, vreg: VReg, constraint: OperandConstraint);

    /// Visit a def (write) operand
    fn visit_def(&mut self, vreg: VReg, constraint: OperandConstraint);

    /// Visit a mod (read-write) operand
    fn visit_mod(&mut self, vreg: VReg, constraint: OperandConstraint);
}

impl<I: MachInst> VCode<I> {
    /// Create a new empty VCode
    pub fn new(
        entry: BlockIndex,
        block_order: BlockLoweringOrder,
        abi: Callee<I::ABIMachineSpec>,
    ) -> Self {
        VCode {
            insts: Vec::new(),
            operands: Vec::new(),
            operand_ranges: Ranges::new(),
            clobbers: BTreeMap::new(),
            block_ranges: Ranges::new(),
            block_succ_range: Ranges::new(),
            block_succs: Vec::new(),
            block_pred_range: Ranges::new(),
            block_preds: Vec::new(),
            block_params_range: Ranges::new(),
            block_params: Vec::new(),
            branch_block_args: Vec::new(),
            branch_block_arg_range: Ranges::new(),
            branch_block_arg_succ_range: Ranges::new(),
            entry,
            block_order,
            abi,
            constants: VCodeConstants {
                constants: BTreeMap::new(),
            },
            block_metadata: Vec::new(),
            relocations: Vec::new(),
            srclocs: Vec::new(),
        }
    }
}
