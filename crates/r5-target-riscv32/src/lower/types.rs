//! Type definitions for instruction lowering.

use r5_ir::Inst;
use riscv32_encoder::Gpr;

/// Instruction offset (instruction index: 0, 1, 2, ...).
///
/// This represents an offset in terms of instruction count, not bytes.
/// Use this for relocations, instruction indices, and instruction counts.
///
/// To convert to bytes, use `Into<ByteOffset>` which multiplies by 4.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstOffset(pub usize);

/// Byte offset (signed, for stack offsets in RISC-V).
///
/// This represents an offset in bytes. Stack offsets in RISC-V are signed
/// (negative for frame-relative offsets, positive for stack arguments).
///
/// To convert from instruction offset, use `From<InstOffset>` which multiplies by 4.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(pub i32);

impl InstOffset {
    /// Create a new instruction offset.
    pub fn new(value: usize) -> Self {
        Self(value)
    }

    /// Get the underlying usize value.
    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl ByteOffset {
    /// Create a new byte offset.
    pub fn new(value: i32) -> Self {
        Self(value)
    }

    /// Get the underlying i32 value.
    pub fn as_i32(self) -> i32 {
        self.0
    }
}

// Conversion: InstOffset -> ByteOffset (multiply by 4)
impl From<InstOffset> for ByteOffset {
    fn from(inst_offset: InstOffset) -> Self {
        // Check for overflow: usize * 4 might overflow i32
        // For RISC-V 32-bit, instruction count is limited, so this should be safe
        // but we use checked arithmetic for safety
        let byte_offset = inst_offset.0
            .checked_mul(4)
            .and_then(|v| i32::try_from(v).ok())
            .expect("Instruction offset too large to convert to byte offset");
        ByteOffset(byte_offset)
    }
}

// Conversion: usize -> InstOffset (for backward compatibility during migration)
impl From<usize> for InstOffset {
    fn from(value: usize) -> Self {
        InstOffset(value)
    }
}

// Conversion: i32 -> ByteOffset (for direct stack offset values)
impl From<i32> for ByteOffset {
    fn from(value: i32) -> Self {
        ByteOffset(value)
    }
}

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
    pub offset: InstOffset,
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
