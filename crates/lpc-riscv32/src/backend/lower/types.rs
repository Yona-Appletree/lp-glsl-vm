//! Type definitions for instruction lowering.

use lpc_lpir::Inst;

use crate::Gpr;

const INST_SIZE_BYTES: u32 = 4; // Size of one instruction in bytes
const WORD_SIZE_BYTES: u32 = 4; // Size of one word in bytes (RISC-V 32-bit)

/// Instruction offset (signed, for instruction positions and relative offsets).
///
/// This represents a position/index or relative offset in terms of instruction count, not bytes.
/// Can be negative for backward jumps/branches. Use this for relocations, instruction indices,
/// and relative instruction offsets.
///
/// To convert to bytes, use `Into<ByteOffset>` which multiplies by `INST_SIZE_BYTES`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstOffset(pub i32);

/// Instruction size (unsigned, for instruction counts).
///
/// This represents a size/count in terms of instruction count, not bytes.
/// Use this for instruction counts, code sizes, and instruction differences.
///
/// To convert to bytes, use `Into<ByteSize>` which multiplies by `INST_SIZE_BYTES`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstSize(pub u32);

/// Word offset (signed, for stack frame offsets in words).
///
/// This represents an offset in terms of word count, not bytes.
/// Can be negative for frame-relative offsets, positive for stack arguments.
/// Use this for conceptual frame layout calculations.
///
/// To convert to bytes, use `Into<ByteOffset>` which multiplies by `WORD_SIZE_BYTES`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WordOffset(pub i32);

/// Word size (unsigned, for frame sizes and storage sizes in words).
///
/// This represents a size/count in terms of words, not bytes.
/// Use this for frame sizes, storage sizes, and other word counts.
/// This is the conceptual unit for thinking about stack frames.
///
/// To convert to bytes, use `Into<ByteSize>` which multiplies by `WORD_SIZE_BYTES`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WordSize(pub u32);

/// Byte offset (signed, for stack offsets in RISC-V).
///
/// This represents an offset in bytes. Stack offsets in RISC-V are signed
/// (negative for frame-relative offsets, positive for stack arguments).
///
/// To convert from instruction offset, use `From<InstOffset>` which multiplies by `INST_SIZE_BYTES`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(pub i32);

/// Byte size (unsigned, for frame sizes and storage sizes).
///
/// This represents a size in bytes. Sizes are always non-negative.
/// Use this for frame sizes, storage sizes, and other byte counts.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteSize(pub u32);

impl InstOffset {
    /// Create a new instruction offset.
    pub fn new(value: i32) -> Self {
        Self(value)
    }

    /// Get the underlying i32 value.
    pub fn as_i32(self) -> i32 {
        self.0
    }

    /// Get as usize (for use with arrays/indices where negative doesn't make sense).
    /// Panics if the value is negative.
    pub fn as_usize(self) -> usize {
        self.0
            .try_into()
            .expect("InstOffset must be non-negative for usize conversion")
    }
}

impl InstSize {
    /// Create a new instruction size.
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Get the underlying u32 value.
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Get as usize (for use with arrays/indices).
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl WordOffset {
    /// Create a new word offset.
    pub fn new(value: i32) -> Self {
        Self(value)
    }

    /// Get the underlying i32 value.
    pub fn as_i32(self) -> i32 {
        self.0
    }

    /// Get as usize (for use with arrays/indices where negative doesn't make sense).
    /// Panics if the value is negative.
    pub fn as_usize(self) -> usize {
        self.0
            .try_into()
            .expect("WordOffset must be non-negative for usize conversion")
    }
}

impl WordSize {
    /// Create a new word size.
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Get the underlying u32 value.
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Get as usize (for use with arrays/indices).
    pub fn as_usize(self) -> usize {
        self.0 as usize
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

    /// Add bytes to this offset.
    pub fn add_bytes(self, bytes: u32) -> Self {
        ByteOffset(self.0 + bytes as i32)
    }
}

impl ByteSize {
    /// Create a new byte size.
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    /// Get the underlying u32 value.
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Add bytes to this size.
    pub fn add_bytes(self, bytes: u32) -> Self {
        ByteSize(self.0 + bytes)
    }
}

impl core::ops::Add for ByteOffset {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        ByteOffset(self.0 + other.0)
    }
}

impl core::ops::Add<i32> for ByteOffset {
    type Output = Self;

    fn add(self, other: i32) -> Self {
        ByteOffset(self.0 + other)
    }
}

impl core::ops::Add for ByteSize {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        ByteSize(self.0 + other.0)
    }
}

impl core::ops::Add<u32> for ByteSize {
    type Output = Self;

    fn add(self, other: u32) -> Self {
        ByteSize(self.0 + other)
    }
}

impl core::ops::Sub for ByteSize {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        ByteSize(self.0.saturating_sub(other.0))
    }
}

impl core::fmt::Display for ByteSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl core::ops::Add for WordSize {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        WordSize(self.0 + other.0)
    }
}

impl core::ops::Add<u32> for WordSize {
    type Output = Self;

    fn add(self, other: u32) -> Self {
        WordSize(self.0 + other)
    }
}

impl core::ops::Add for InstSize {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        InstSize(self.0 + other.0)
    }
}

impl core::ops::Add<u32> for InstSize {
    type Output = Self;

    fn add(self, other: u32) -> Self {
        InstSize(self.0 + other)
    }
}

impl core::ops::Add for WordOffset {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        WordOffset(self.0 + other.0)
    }
}

impl core::ops::Add<i32> for WordOffset {
    type Output = Self;

    fn add(self, other: i32) -> Self {
        WordOffset(self.0 + other)
    }
}

// Conversion: InstOffset -> ByteOffset (multiply by INST_SIZE_BYTES)
impl From<InstOffset> for ByteOffset {
    fn from(inst_offset: InstOffset) -> Self {
        // Multiply by INST_SIZE_BYTES, checking for overflow
        let byte_offset = inst_offset
            .0
            .checked_mul(INST_SIZE_BYTES as i32)
            .expect("Instruction offset too large to convert to byte offset");
        ByteOffset(byte_offset)
    }
}

// Conversion: ByteOffset -> InstOffset (divide by INST_SIZE_BYTES)
impl From<ByteOffset> for InstOffset {
    fn from(byte_offset: ByteOffset) -> Self {
        // Divide by INST_SIZE_BYTES, rounding towards zero
        InstOffset(byte_offset.0 / INST_SIZE_BYTES as i32)
    }
}

// Conversion: usize -> InstOffset (for absolute positions)
impl From<usize> for InstOffset {
    fn from(value: usize) -> Self {
        InstOffset(value.try_into().expect("usize too large to convert to i32"))
    }
}

// Conversion: i32 -> InstOffset
impl From<i32> for InstOffset {
    fn from(value: i32) -> Self {
        InstOffset(value)
    }
}

// Conversion: usize -> InstSize (for backward compatibility)
impl From<usize> for InstSize {
    fn from(value: usize) -> Self {
        InstSize(value.try_into().expect("usize too large to convert to u32"))
    }
}

// Conversion: u32 -> InstSize
impl From<u32> for InstSize {
    fn from(value: u32) -> Self {
        InstSize(value)
    }
}

// Conversion: InstSize -> usize
impl From<InstSize> for usize {
    fn from(size: InstSize) -> Self {
        size.0 as usize
    }
}

// Conversion: InstSize -> ByteSize (multiply by INST_SIZE_BYTES)
impl From<InstSize> for ByteSize {
    fn from(inst_size: InstSize) -> Self {
        // Multiply by INST_SIZE_BYTES, checking for overflow
        let byte_size = inst_size
            .0
            .checked_mul(INST_SIZE_BYTES)
            .expect("Instruction size too large to convert to byte size");
        ByteSize(byte_size)
    }
}

// Conversion: ByteSize -> InstSize (divide by INST_SIZE_BYTES)
impl From<ByteSize> for InstSize {
    fn from(byte_size: ByteSize) -> Self {
        // Divide by INST_SIZE_BYTES, rounding towards zero
        InstSize(byte_size.0 / INST_SIZE_BYTES)
    }
}

// Conversion: usize -> WordSize (for backward compatibility)
impl From<usize> for WordSize {
    fn from(value: usize) -> Self {
        WordSize(value.try_into().expect("usize too large to convert to u32"))
    }
}

// Conversion: u32 -> WordSize
impl From<u32> for WordSize {
    fn from(value: u32) -> Self {
        WordSize(value)
    }
}

// Conversion: WordSize -> usize
impl From<WordSize> for usize {
    fn from(size: WordSize) -> Self {
        size.0 as usize
    }
}

// Conversion: WordSize -> ByteSize (multiply by WORD_SIZE_BYTES)
impl From<WordSize> for ByteSize {
    fn from(word_size: WordSize) -> Self {
        // Multiply by WORD_SIZE_BYTES, checking for overflow
        let byte_size = word_size
            .0
            .checked_mul(WORD_SIZE_BYTES)
            .expect("Word size too large to convert to byte size");
        ByteSize(byte_size)
    }
}

// Conversion: ByteSize -> WordSize (divide by WORD_SIZE_BYTES)
impl From<ByteSize> for WordSize {
    fn from(byte_size: ByteSize) -> Self {
        // Divide by WORD_SIZE_BYTES, rounding towards zero
        WordSize(byte_size.0 / WORD_SIZE_BYTES)
    }
}

// Conversion: i32 -> WordOffset
impl From<i32> for WordOffset {
    fn from(value: i32) -> Self {
        WordOffset(value)
    }
}

// Conversion: WordOffset -> ByteOffset (multiply by WORD_SIZE_BYTES)
impl From<WordOffset> for ByteOffset {
    fn from(word_offset: WordOffset) -> Self {
        // Multiply by WORD_SIZE_BYTES, checking for overflow
        let byte_offset = word_offset
            .0
            .checked_mul(WORD_SIZE_BYTES as i32)
            .expect("Word offset too large to convert to byte offset");
        ByteOffset(byte_offset)
    }
}

// Conversion: ByteOffset -> WordOffset (divide by WORD_SIZE_BYTES)
impl From<ByteOffset> for WordOffset {
    fn from(byte_offset: ByteOffset) -> Self {
        // Divide by WORD_SIZE_BYTES, rounding towards zero
        WordOffset(byte_offset.0 / WORD_SIZE_BYTES as i32)
    }
}

// Conversion: i32 -> ByteOffset (for direct stack offset values)
impl From<i32> for ByteOffset {
    fn from(value: i32) -> Self {
        ByteOffset(value)
    }
}

// Conversion: u32 -> ByteSize (for direct size values)
impl From<u32> for ByteSize {
    fn from(value: u32) -> Self {
        ByteSize(value)
    }
}

// Conversion: ByteSize -> u32 (for backward compatibility)
impl From<ByteSize> for u32 {
    fn from(size: ByteSize) -> Self {
        size.0
    }
}

// Conversion: ByteSize -> i32 (for casting to signed offsets)
impl From<ByteSize> for i32 {
    fn from(size: ByteSize) -> Self {
        size.0 as i32
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
    ValueNotAllocated { value: lpc_lpir::Value },
    /// Unimplemented instruction
    UnimplementedInstruction { inst: Inst },
    /// Result value must be in register (internal error)
    ResultNotInRegister { value: lpc_lpir::Value },
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
