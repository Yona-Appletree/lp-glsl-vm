//! IR instructions.

use alloc::{string::String, vec, vec::Vec};
use core::fmt;

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
    /// Function call: results = callee(args...)
    Call {
        /// Function name to call (must exist in module)
        callee: String,
        /// Argument values
        args: Vec<Value>,
        /// Result values (SSA)
        results: Vec<Value>,
    },
    /// System call: syscall(number, args...)
    Syscall {
        /// Syscall number
        number: i32,
        /// Argument values (passed in a0-a7)
        args: Vec<Value>,
    },
    /// Return with values
    Return { values: Vec<Value> },
    /// Halt execution (ebreak)
    Halt,

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
            Inst::Call { results, .. } => results.clone(),
            Inst::Syscall { .. } => Vec::new(),
            Inst::Jump { .. } | Inst::Br { .. } | Inst::Store { .. } | Inst::Halt => Vec::new(),
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
            Inst::Iconst { .. } | Inst::Fconst { .. } | Inst::Jump { .. } | Inst::Halt => {
                Vec::new()
            }
            Inst::Br { condition, .. } => vec![*condition],
            Inst::Call { args, .. } => args.clone(),
            Inst::Syscall { args, .. } => args.clone(),
            Inst::Return { values } => values.clone(),
            Inst::Load { address, .. } => vec![*address],
            Inst::Store { address, value, .. } => vec![*address, *value],
        }
    }
}

impl fmt::Display for Inst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Inst::Iadd { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = iadd v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Isub { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = isub v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Imul { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = imul v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Idiv { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = idiv v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Irem { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = irem v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::IcmpEq { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = icmp_eq v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::IcmpNe { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = icmp_ne v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::IcmpLt { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = icmp_lt v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::IcmpLe { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = icmp_le v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::IcmpGt { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = icmp_gt v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::IcmpGe { result, arg1, arg2 } => {
                write!(
                    f,
                    "v{} = icmp_ge v{}, v{}",
                    result.index(),
                    arg1.index(),
                    arg2.index()
                )
            }
            Inst::Iconst { result, value } => {
                write!(f, "v{} = iconst {}", result.index(), value)
            }
            Inst::Fconst { result, value_bits } => {
                // Decode f64 from bits
                let f64_value = f64::from_bits(*value_bits);
                write!(f, "v{} = fconst {}", result.index(), f64_value)
            }
            Inst::Jump { target } => {
                write!(f, "jump block{}", target)
            }
            Inst::Br {
                condition,
                target_true,
                target_false,
            } => {
                write!(
                    f,
                    "brif v{}, block{}, block{}",
                    condition.index(),
                    target_true,
                    target_false
                )
            }
            Inst::Call {
                callee,
                args,
                results,
            } => {
                write!(f, "call %{}(", callee)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "v{}", arg.index())?;
                }
                write!(f, ")")?;
                if !results.is_empty() {
                    write!(f, " -> ")?;
                    for (i, res) in results.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "v{}", res.index())?;
                    }
                }
                Ok(())
            }
            Inst::Syscall { number, args } => {
                write!(f, "syscall {}(", number)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "v{}", arg.index())?;
                }
                write!(f, ")")
            }
            Inst::Return { values } => {
                write!(f, "return")?;
                if !values.is_empty() {
                    for val in values.iter() {
                        write!(f, " v{}", val.index())?;
                    }
                }
                Ok(())
            }
            Inst::Load {
                result,
                address,
                ty,
            } => {
                write!(f, "v{} = load.{} v{}", result.index(), ty, address.index())
            }
            Inst::Store { address, value, ty } => {
                write!(f, "store.{} v{}, v{}", ty, address.index(), value.index())
            }
            Inst::Halt => write!(f, "halt"),
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
