//! Core types for backend3 (ISA-agnostic)

use core::fmt;

/// Virtual register identifier
///
/// Virtual registers are used during lowering and register allocation.
/// They are later replaced by physical registers during allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VReg(u32);

impl VReg {
    /// Create a new virtual register with the given index
    pub fn new(index: u32) -> Self {
        VReg(index)
    }

    /// Get the index of this virtual register
    pub fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for VReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// Block index in VCode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockIndex(u32);

impl BlockIndex {
    /// Create a new block index
    pub fn new(index: u32) -> Self {
        BlockIndex(index)
    }

    /// Get the index value
    pub fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for BlockIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "block{}", self.0)
    }
}

/// Instruction index in VCode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InsnIndex(u32);

impl InsnIndex {
    /// Create a new instruction index
    pub fn new(index: u32) -> Self {
        InsnIndex(index)
    }

    /// Get the index value
    pub fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for InsnIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "inst{}", self.0)
    }
}

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
