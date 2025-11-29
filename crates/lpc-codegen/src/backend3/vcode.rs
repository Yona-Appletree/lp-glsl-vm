//! VCode structure: Virtual-register code with machine instructions
//!
//! # Operand Constraints
//!
//! The operand constraint system allows instructions to specify requirements for
//! register allocation. Constraints are collected during operand collection and
//! used by regalloc2 to assign physical registers.
//!
//! ## Constraint Types
//!
//! - **`OperandConstraint::Any`**: The operand can be assigned to any register.
//!   This is the default for most instructions.
//!
//! - **`OperandConstraint::Fixed(u32)`**: The operand must be assigned to a
//!   specific physical register (represented as u32 for now). This is used for
//!   instructions that require specific registers (e.g., system calls, ABI
//!   requirements).
//!
//! - **`OperandConstraint::RegClass(RegClass)`**: The operand must be assigned to
//!   a register in a specific register class (e.g., GPR for integer registers,
//!   FPR for floating-point registers).
//!
//! ## Operand Kinds
//!
//! - **`OperandKind::Use`**: The operand is read (input to the instruction).
//! - **`OperandKind::Def`**: The operand is written (output from the instruction).
//! - **`OperandKind::Mod`**: The operand is both read and written (read-modify-write).
//!
//! ## ISA-Specific Constraints
//!
//! ISA-specific backends implement `MachInst::get_operands()` to specify constraints
//! for each instruction type. Currently, RISC-V instructions use `OperandConstraint::Any`
//! for all operands, but the system is designed to support fixed registers and register
//! classes when needed.
//!
//! ## Example
//!
//! ```rust,ignore
//! // RISC-V ADD instruction (conceptual example)
//! // In actual implementation:
//! match inst {
//!     Riscv32MachInst::Add { rd, rs1, rs2 } => {
//!         collector.visit_def(rd.to_reg(), OperandConstraint::Any);  // Def: result register
//!         collector.visit_use(*rs1, OperandConstraint::Any);         // Use: first source
//!         collector.visit_use(*rs2, OperandConstraint::Any);         // Use: second source
//!     }
//!     _ => {}
//! }
//! ```
//!
//! ## Interaction with regalloc2
//!
//! The operand constraint system is designed to work with regalloc2. Constraints
//! are collected into flat arrays (`operands` and `operand_ranges`) that regalloc2
//! can efficiently process. The regalloc2 library uses these constraints to:
//!
//! - Assign physical registers that satisfy the constraints
//! - Handle fixed register requirements
//! - Respect register class restrictions
//! - Optimize register allocation based on operand kinds (use/def/mod)

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::fmt;

use lpc_lpir::RelSourceLoc;
use regalloc2::{Operand, VReg};

use crate::backend3::types::{BlockIndex, InsnIndex, Ranges, VReg as OurVReg};

/// Virtual-register code: machine instructions with virtual registers
///
/// This is the intermediate representation between IR lowering and register allocation.
/// All registers are virtual (VReg) and will be assigned physical registers during
/// register allocation.
pub struct VCode<I: MachInst> {
    /// Machine instructions (with VReg operands)
    pub insts: Vec<I>,

    /// Operands: flat array for regalloc2
    /// This is populated during operand collection with regalloc2::Operand
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
    pub block_params: Vec<VReg>,      // Block parameter VRegs (flat array, regalloc2::VReg)

    /// Branch arguments (values passed to blocks)
    pub branch_block_args: Vec<VReg>, // regalloc2::VReg
    pub branch_block_arg_range: Ranges,
    pub branch_block_arg_succ_range: Ranges,

    /// Entry block
    pub entry: BlockIndex,

    /// Block lowering order
    pub block_order: BlockLoweringOrder,

    /// ABI information
    pub abi: Callee<I::ABIMachineSpec>,

    /// ISA-specific emission information
    ///
    /// This contains information needed during code emission (Phase 3),
    /// such as ISA-specific flags and settings. It should be immutable
    /// across function compilations within the same module.
    pub emit_info: I::Info,

    /// Constants (inline or pool references)
    pub constants: VCodeConstants,

    /// Block metadata
    pub block_metadata: Vec<BlockMetadata>,

    /// Relocations (function calls, etc.)
    pub relocations: Vec<VCodeReloc>,

    /// Source locations for each instruction (for debugging)
    /// One RelSourceLoc per instruction, parallel to `insts` array
    pub srclocs: Vec<RelSourceLoc>,

    /// Number of virtual registers allocated
    /// This is the maximum VReg index + 1, used by regalloc2
    pub num_vregs: usize,
}

// Note: We now use regalloc2::Operand directly instead of our own Operand struct
// The conversion from our VReg/OperandConstraint/OperandKind happens during
// operand collection in VCodeBuilder::collect_operands()

/// Operand constraint for register allocation
///
/// Constraints specify requirements for register allocation. They are collected
/// during operand collection and used by regalloc2 to assign physical registers.
///
/// # Examples
///
/// ```rust,ignore
/// # // Note: This is a conceptual example. In actual code, use:
/// # // use lpc_codegen::backend3::vcode::{OperandConstraint, RegClass};
///
/// // Any register is acceptable
/// OperandConstraint::Any
///
/// // Must be assigned to physical register 5 (e.g., a0 for RISC-V)
/// OperandConstraint::Fixed(5)
///
/// // Must be in general-purpose register class
/// OperandConstraint::RegClass(RegClass::Gpr)
/// ```
///
/// Note: PReg is a trait, so we can't use it directly in an enum.
/// For now, we use a placeholder. ISA-specific code will provide
/// concrete constraint types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandConstraint {
    /// Any register is acceptable (default for most instructions)
    Any,
    /// Fixed physical register (represented as u32 for now)
    ///
    /// Used when an instruction requires a specific physical register,
    /// such as system calls or ABI requirements.
    Fixed(u32),
    /// Register class constraint
    ///
    /// Used when an operand must be in a specific register class
    /// (e.g., integer registers vs. floating-point registers).
    RegClass(RegClass),
}

/// Operand kind: how an operand is used in an instruction
///
/// This distinguishes between operands that are read, written, or both.
/// Register allocation uses this information to determine liveness and
/// optimize register assignment.
///
/// # Examples
///
/// ```rust,ignore
/// # // Note: This is a conceptual example. In actual code, use:
/// # // use lpc_codegen::backend3::vcode::OperandKind;
///
/// // Input operand (read before instruction executes)
/// OperandKind::Use
///
/// // Output operand (written by instruction)
/// OperandKind::Def
///
/// // Read-modify-write operand (read before, written after)
/// OperandKind::Mod
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandKind {
    /// Use (read): operand is read by the instruction
    Use,
    /// Def (write): operand is written by the instruction
    Def,
    /// Mod (read and write): operand is both read and written
    Mod,
}

/// Physical register (ISA-specific, will be defined per ISA)
///
/// This is a placeholder trait. ISA-specific implementations will provide
/// concrete types that implement this trait.
pub trait PReg: Copy + Clone + PartialEq + Eq + core::hash::Hash + fmt::Debug {}

/// Physical register set
///
/// We use regalloc2's PRegSet directly. During lowering, we use a placeholder
/// (BTreeSet<u32>) which is converted to regalloc2::PRegSet during operand collection.
pub type PRegSet = regalloc2::PRegSet;

/// Register class: category of registers for an operand
///
/// Register classes distinguish between different types of registers
/// (e.g., integer vs. floating-point). This allows register allocation
/// to ensure operands are assigned to appropriate registers.
///
/// # Examples
///
/// ```rust,ignore
/// # // Note: This is a conceptual example. In actual code, use:
/// # // use lpc_codegen::backend3::vcode::RegClass;
///
/// // Integer operand (must be in GPR)
/// RegClass::Gpr
///
/// // Floating-point operand (must be in FPR)
/// RegClass::Fpr
/// ```
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
    /// Constant values indexed by our VReg (used during lowering)
    /// Note: This maps our internal VReg to constants, not regalloc2::VReg
    pub constants: BTreeMap<OurVReg, Constant>,
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

/// Terminator kind for machine instructions
///
/// This distinguishes between different types of terminator instructions,
/// which is needed for control flow analysis during register allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachTerminator {
    /// Return instruction (function exit)
    Ret,
    /// Return from call instruction
    RetCall,
    /// Branch instruction (conditional or unconditional)
    Branch,
    /// Not a terminator (normal instruction)
    None,
}

/// MachInst trait: Machine instruction interface for regalloc2
///
/// This trait must be implemented by ISA-specific machine instruction types.
pub trait MachInst: Clone + fmt::Debug {
    /// ABI machine spec type
    type ABIMachineSpec: fmt::Debug;

    /// ISA-specific emission information
    ///
    /// This type contains information needed during code emission (Phase 3),
    /// such as ISA-specific flags and settings. It should be immutable across
    /// function compilations within the same module.
    type Info: Clone + fmt::Debug;

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

    /// Check if this instruction is a terminator and what kind
    ///
    /// Returns the terminator kind, or `MachTerminator::None` if this is
    /// a normal instruction. This is used by regalloc2 for control flow analysis.
    fn is_term(&self) -> MachTerminator {
        MachTerminator::None
    }
}

/// Operand visitor trait for collecting operands
///
/// This trait is used during lowering to collect operands from machine instructions.
/// The visitor receives our internal VReg type and OperandConstraint, which are
/// later converted to regalloc2 types during operand collection.
pub trait OperandVisitor {
    /// Visit a use (read) operand
    fn visit_use(&mut self, vreg: OurVReg, constraint: OperandConstraint);

    /// Visit a def (write) operand
    fn visit_def(&mut self, vreg: OurVReg, constraint: OperandConstraint);

    /// Visit a mod (read-write) operand
    fn visit_mod(&mut self, vreg: OurVReg, constraint: OperandConstraint);
}

impl<I: MachInst> VCode<I> {
    /// Create a new empty VCode
    pub fn new(
        entry: BlockIndex,
        block_order: BlockLoweringOrder,
        abi: Callee<I::ABIMachineSpec>,
        emit_info: I::Info,
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
            emit_info,
            constants: VCodeConstants {
                constants: BTreeMap::new(),
            },
            block_metadata: Vec::new(),
            relocations: Vec::new(),
            srclocs: Vec::new(),
            num_vregs: 0,
        }
    }

    /// Get the number of virtual registers
    ///
    /// This returns the total number of VRegs allocated, which is needed
    /// for regalloc2 to know the VReg index space.
    pub fn num_vregs(&self) -> usize {
        self.num_vregs
    }
}
