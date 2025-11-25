//! IR instructions.

use alloc::{vec, vec::Vec};

use crate::{types::Type, value::Value};

/// An IR instruction.
#[derive(Debug, Clone)]
pub enum Inst {
    // Arithmetic
    /// Integer add: result = arg1 + arg2
    Iadd {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer subtract: result = arg1 - arg2
    Isub {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer multiply: result = arg1 * arg2
    Imul {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer divide: result = arg1 / arg2
    Idiv {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer remainder: result = arg1 % arg2
    Irem {
        result: Value,
        arg1: Value,
        arg2: Value,
    },

    // Comparisons
    /// Integer compare equal: result = (arg1 == arg2)
    IcmpEq {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer compare not equal: result = (arg1 != arg2)
    IcmpNe {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer compare less than: result = (arg1 < arg2)
    IcmpLt {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer compare less than or equal: result = (arg1 <= arg2)
    IcmpLe {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer compare greater than: result = (arg1 > arg2)
    IcmpGt {
        result: Value,
        arg1: Value,
        arg2: Value,
    },
    /// Integer compare greater than or equal: result = (arg1 >= arg2)
    IcmpGe {
        result: Value,
        arg1: Value,
        arg2: Value,
    },

    // Constants
    /// Integer constant: result = value
    Iconst { result: Value, value: i64 },
    /// Floating point constant: result = value
    /// Note: Uses u64 to represent f64 bits for Eq compatibility
    Fconst { result: Value, value_bits: u64 },

    // Control flow
    /// Jump to block
    Jump { target: u32 },
    /// Conditional branch: if condition, jump to target_true, else target_false
    Br {
        condition: Value,
        target_true: u32,
        target_false: u32,
    },
    /// Return with values
    Return { values: Vec<Value> },

    // Memory
    /// Load from memory: result = mem[address]
    Load {
        result: Value,
        address: Value,
        ty: Type,
    },
    /// Store to memory: mem[address] = value
    Store {
        address: Value,
        value: Value,
        ty: Type,
    },
}

impl Inst {
    /// Get the result value(s) produced by this instruction, if any.
    pub fn results(&self) -> Vec<Value> {
        match self {
            Inst::Iadd { result, .. }
            | Inst::Isub { result, .. }
            | Inst::Imul { result, .. }
            | Inst::Idiv { result, .. }
            | Inst::Irem { result, .. }
            | Inst::IcmpEq { result, .. }
            | Inst::IcmpNe { result, .. }
            | Inst::IcmpLt { result, .. }
            | Inst::IcmpLe { result, .. }
            | Inst::IcmpGt { result, .. }
            | Inst::IcmpGe { result, .. }
            | Inst::Iconst { result, .. }
            | Inst::Fconst { result, .. }
            | Inst::Load { result, .. } => vec![*result],
            Inst::Jump { .. } | Inst::Br { .. } | Inst::Store { .. } => Vec::new(),
            Inst::Return { values } => values.clone(),
        }
    }

    /// Get the argument values used by this instruction.
    pub fn args(&self) -> Vec<Value> {
        match self {
            Inst::Iadd { arg1, arg2, .. }
            | Inst::Isub { arg1, arg2, .. }
            | Inst::Imul { arg1, arg2, .. }
            | Inst::Idiv { arg1, arg2, .. }
            | Inst::Irem { arg1, arg2, .. }
            | Inst::IcmpEq { arg1, arg2, .. }
            | Inst::IcmpNe { arg1, arg2, .. }
            | Inst::IcmpLt { arg1, arg2, .. }
            | Inst::IcmpLe { arg1, arg2, .. }
            | Inst::IcmpGt { arg1, arg2, .. }
            | Inst::IcmpGe { arg1, arg2, .. } => vec![*arg1, *arg2],
            Inst::Iconst { .. } | Inst::Fconst { .. } | Inst::Jump { .. } => Vec::new(),
            Inst::Br { condition, .. } => vec![*condition],
            Inst::Return { values } => values.clone(),
            Inst::Load { address, .. } => vec![*address],
            Inst::Store { address, value, .. } => vec![*address, *value],
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn test_iadd() {
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let v3 = Value::new(3);
        let inst = Inst::Iadd {
            result: v3,
            arg1: v1,
            arg2: v2,
        };
        // Note: Can't compare Inst directly due to f64, but we can test methods
        let results = inst.results();
        let args = inst.args();
        assert_eq!(results.len(), 1);
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn test_iconst() {
        let v1 = Value::new(1);
        let inst = Inst::Iconst {
            result: v1,
            value: 42,
        };
        assert_eq!(inst.results(), vec![v1]);
        assert_eq!(inst.args(), Vec::new());
    }

    #[test]
    fn test_return() {
        let v1 = Value::new(1);
        let v2 = Value::new(2);
        let inst = Inst::Return {
            values: vec![v1, v2],
        };
        assert_eq!(inst.results(), vec![v1, v2]);
        assert_eq!(inst.args(), vec![v1, v2]);
    }
}
