//! Core types for backend3 (ISA-agnostic)

use core::fmt;
use regalloc2::{Block, Inst, PReg, RegClass};

/// Number of pinned VReg indices reserved for physical registers
///
/// Following Cranelift's design, the first 192 VReg indices (0-191) are reserved
/// for physical registers represented as "pinned" VRegs:
/// - Int: 0-63 (64 registers)
/// - Float: 64-127 (64 registers)
/// - Vector: 128-191 (64 registers)
///
/// Regular VRegs are allocated starting from index 192.
pub const PINNED_VREGS: usize = 192;

/// Virtual register identifier
///
/// Virtual registers are used during lowering and register allocation.
/// They are later replaced by physical registers during allocation.
/// This is a type alias for regalloc2::VReg to ensure compatibility
/// with regalloc2's Function trait.
pub type VReg = regalloc2::VReg;

/// Unified register type that can represent both virtual and physical registers
///
/// Following Cranelift's design, this unified type wraps a u32 index that can represent:
/// - Virtual registers (VRegs) - normal allocatable registers (indices >= PINNED_VREGS)
/// - Physical registers (PRegs) - represented as "pinned" VRegs (indices 0-191)
///
/// Physical registers are represented as pinned VRegs with fixed indices:
/// - Int physical registers: indices 0-63
/// - Float physical registers: indices 64-127
/// - Vector physical registers: indices 128-191
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Reg(u32);

impl Reg {
    /// Create a Reg from a virtual register
    ///
    /// The VReg's index is used directly. VRegs should be allocated starting
    /// from PINNED_VREGS (192) to avoid conflicts with physical registers.
    pub fn from_virtual_reg(vreg: VReg) -> Self {
        Reg(vreg.vreg() as u32)
    }

    /// Create a Reg from a physical register
    ///
    /// Physical registers are represented as pinned VRegs with indices based on
    /// their register class and hardware encoding:
    /// - Int: indices 0-63 (based on hw_enc)
    /// - Float: indices 64-127 (64 + hw_enc)
    /// - Vector: indices 128-191 (128 + hw_enc)
    pub fn from_real_reg(preg: PReg) -> Self {
        let hw_enc = preg.hw_enc();
        let base = match preg.class() {
            RegClass::Int => 0,
            RegClass::Float => 64,
            RegClass::Vector => 128,
        };
        Reg((base + hw_enc) as u32)
    }

    /// Convert to a virtual register, if this is a virtual register
    ///
    /// Note: This reconstructs the VReg from the stored index. The RegClass
    /// is assumed to be Int (the default for RISC-V 32). If the original
    /// VReg had a different class, it will be lost. This is acceptable since
    /// regalloc2 will handle the conversion correctly during allocation.
    pub fn to_virtual_reg(self) -> Option<VReg> {
        if self.is_virtual() {
            Some(VReg::new(self.0 as usize, RegClass::Int))
        } else {
            None
        }
    }

    /// Convert to a physical register, if this is a physical register
    pub fn to_real_reg(self) -> Option<PReg> {
        if self.is_real() {
            let (base, class) = if self.0 < 64 {
                (0, RegClass::Int)
            } else if self.0 < 128 {
                (64, RegClass::Float)
            } else if self.0 < 192 {
                (128, RegClass::Vector)
            } else {
                return None;
            };
            let hw_enc = (self.0 as usize) - base;
            Some(PReg::new(hw_enc, class))
        } else {
            None
        }
    }

    /// Check if this is a physical register (pinned VReg)
    pub fn is_real(&self) -> bool {
        self.0 < PINNED_VREGS as u32
    }

    /// Check if this is a virtual register
    pub fn is_virtual(&self) -> bool {
        self.0 >= PINNED_VREGS as u32
    }

    /// Get the underlying index
    pub fn index(&self) -> u32 {
        self.0
    }
}

impl From<VReg> for Reg {
    fn from(vreg: VReg) -> Self {
        Reg::from_virtual_reg(vreg)
    }
}

impl From<PReg> for Reg {
    fn from(preg: PReg) -> Self {
        Reg::from_real_reg(preg)
    }
}

impl From<Reg> for VReg {
    fn from(reg: Reg) -> Self {
        // For regalloc2, we need to convert Reg to VReg
        // If it's a physical register (pinned VReg), we create a VReg with the pinned index
        // regalloc2 handles pinned VRegs correctly (they're just VRegs with fixed indices)
        // For virtual registers, we use the stored index directly with Int class
        VReg::new(reg.0 as usize, RegClass::Int)
    }
}

/// Helper type for virtual registers (for clarity in conversions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VirtualReg(pub VReg);

/// Helper type for physical registers (for clarity in conversions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RealReg(pub PReg);

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_real() {
            if let Some(preg) = self.to_real_reg() {
                write!(f, "preg{}", preg.hw_enc())
            } else {
                write!(f, "reg{}", self.0)
            }
        } else {
            write!(f, "vreg{}", self.0)
        }
    }
}

/// Block index in VCode
///
/// This is a type alias for regalloc2::Block to ensure compatibility
/// with regalloc2's Function trait.
pub type BlockIndex = Block;

/// Instruction index in VCode
///
/// This is a type alias for regalloc2::Inst to ensure compatibility
/// with regalloc2's Function trait.
pub type InsnIndex = Inst;

/// Code offset (for relocations)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CodeOffset(u32);

impl CodeOffset {
    /// Create a new code offset
    pub fn new(offset: u32) -> Self {
        CodeOffset(offset)
    }

    /// Get the offset value
    pub fn offset(self) -> u32 {
        self.0
    }
}

/// Writable virtual register (for instruction results)
///
/// This is a wrapper around VReg that indicates the register
/// is written to by an instruction. This helps distinguish
/// between uses and defs during operand collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Writable<V>(pub V);

impl<V> Writable<V> {
    /// Create a new writable register
    pub fn new(v: V) -> Self {
        Writable(v)
    }

    /// Get the inner register
    pub fn to_reg(self) -> V {
        self.0
    }
}

impl<V: fmt::Display> fmt::Display for Writable<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Range structure for storing per-entity ranges in flat arrays
///
/// This is used to store ranges like "instructions 5-10 belong to block 2"
/// in a space-efficient way using flat arrays and range indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// Start index (inclusive)
    pub start: usize,
    /// End index (exclusive)
    pub end: usize,
}

impl Range {
    /// Create a new range
    pub fn new(start: usize, end: usize) -> Self {
        Range { start, end }
    }

    /// Check if range is empty
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Get the length of the range
    pub fn len(&self) -> usize {
        if self.start >= self.end {
            0
        } else {
            self.end - self.start
        }
    }
}

/// Ranges structure for storing multiple ranges
///
/// This stores ranges for entities (blocks, instructions, etc.)
/// in a flat array format. Each entity has a range that points
/// into a flat array of items.
#[derive(Debug, Clone)]
pub struct Ranges {
    /// Per-entity ranges
    ranges: alloc::vec::Vec<Range>,
}

impl Ranges {
    /// Create a new empty Ranges structure
    pub fn new() -> Self {
        Ranges {
            ranges: alloc::vec::Vec::new(),
        }
    }

    /// Add a range for an entity
    pub fn push(&mut self, range: Range) {
        self.ranges.push(range);
    }

    /// Get the range for an entity by index
    pub fn get(&self, index: usize) -> Option<Range> {
        self.ranges.get(index).copied()
    }

    /// Set the range for an entity by index
    /// Panics if index is out of bounds
    pub fn set(&mut self, index: usize, range: Range) {
        self.ranges[index] = range;
    }

    /// Get the number of ranges
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Iterate over all ranges
    pub fn iter(&self) -> impl Iterator<Item = &Range> {
        self.ranges.iter()
    }
}

impl Default for Ranges {
    fn default() -> Self {
        Self::new()
    }
}
