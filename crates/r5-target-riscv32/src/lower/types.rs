//! Type definitions for instruction lowering.

use r5_ir::Inst;
use riscv32_encoder::Gpr;

/// Target for a relocation.
#[derive(Debug, Clone)]
pub enum RelocationTarget {
    /// Function call target (by name)
    Function(alloc::string::String),
    /// Block target (by block index)
    Block(usize),
    /// Epilogue target (end of function)
    Epilogue,
}

/// Instruction type that needs relocation.
#[derive(Debug, Clone)]
pub enum RelocationInstType {
    /// beq instruction
    Beq { rs1: Gpr, rs2: Gpr },
    /// jal instruction
    Jal { rd: Gpr },
}

/// A relocation that needs to be fixed up.
#[derive(Debug, Clone)]
pub struct Relocation {
    /// Offset in the code buffer where the instruction is (instruction index)
    pub offset: usize,
    /// Target of the relocation
    pub target: RelocationTarget,
    /// Instruction type (needed to reconstruct the instruction)
    pub inst_type: RelocationInstType,
}

/// Lowering error.
#[derive(Debug, Clone)]
pub enum LoweringError {
    /// Value not found in register allocation
    ValueNotAllocated { value: r5_ir::Value },
    /// Unimplemented instruction
    UnimplementedInstruction { inst: Inst },
    /// Result value must be in register (internal error)
    ResultNotInRegister { value: r5_ir::Value },
}

impl core::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoweringError::ValueNotAllocated { value } => {
                write!(f, "Value {:?} not found in allocation", value)
            }
            LoweringError::UnimplementedInstruction { inst } => {
                write!(f, "Unimplemented instruction: {:?}", inst)
            }
            LoweringError::ResultNotInRegister { value } => {
                write!(f, "Result value {:?} must be in register", value)
            }
        }
    }
}
