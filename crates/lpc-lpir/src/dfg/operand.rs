//! Operand classification for register allocation.

/// Operand kind for register allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandKind {
    /// Use: read-only operand (input)
    Use,
    /// Def: write-only operand (output)
    Def,
    /// Modify: read-write operand (input and output)
    Modify,
}

